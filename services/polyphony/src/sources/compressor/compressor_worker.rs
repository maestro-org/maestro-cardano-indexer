use futures_util::StreamExt;
use gasket::runtime::{ScheduleResult, WorkSchedule};
use pallas::ledger::traverse::{Era, OutputRef};
use tonic::codec::CompressionEncoding;
use tracing::{debug, warn};

use gasket::error::AsWorkError;
use tonic::transport::Channel;
use tonic::{Code, Streaming};
use tracing::info;

use crate::model::{BlockContext, EnrichedBlockPayload};
use crate::sources::utils::{block_ref_to_point, point_to_block_ref, IMMUTABLE_AFTER_SLOTS};
use crate::{crosscut, model, sources::utils, storage, Error};

use super::compressor_api::stream_updates_with_ctx_response::Action;
use super::compressor_api::sync_service_client::SyncServiceClient;
use super::compressor_api::{
    BlockRef, PageBlocksWithCtxRequest, StreamUpdatesWithCtxRequest, StreamUpdatesWithCtxResponse,
};
use super::Config;

pub type OutputPort = gasket::messaging::tokio::OutputPort<model::EnrichedBlockPayload>;

pub struct SourceWorkUnit {
    actions: Vec<Action>,
    mutable: bool,
}

enum Mode {
    Init,
    Dumping(Option<BlockRef>),
    Streaming,
}

pub struct Worker {
    config: Config,
    intersect: crosscut::IntersectConfig,
    cursor: storage::Cursor,
    client: Option<SyncServiceClient<Channel>>,
    mode: Mode,
    stream: Option<Streaming<StreamUpdatesWithCtxResponse>>,
    output: OutputPort,
    upstream_slot: Option<u64>,
    block_count: gasket::metrics::Counter,
    chain_tip: gasket::metrics::Gauge,
}

impl Worker {
    pub fn new(
        config: Config,
        intersect: crosscut::IntersectConfig,
        cursor: storage::Cursor,
        output: OutputPort,
    ) -> Self {
        Self {
            config,
            intersect,
            cursor,
            mode: Mode::Init,
            output,
            client: None,
            stream: None,
            upstream_slot: None,
            block_count: Default::default(),
            chain_tip: Default::default(),
        }
    }

    async fn process_action(
        &mut self,
        action: Action,
        mutable: bool,
    ) -> Result<(), gasket::error::Error> {
        match action {
            Action::Apply(block) => {
                let block_ref = block
                    .r#ref
                    .ok_or(Error::source("missing block_ref"))
                    .or_panic()?;

                let mut ctx = BlockContext::new();

                for txo in block.txo_resolver {
                    let txo_ref = txo
                        .r#ref
                        .ok_or(Error::source("missing txoref"))
                        .or_panic()?;

                    let ref_tx_hash: [u8; 32] = txo_ref
                        .tx_hash
                        .try_into()
                        .map_err(|_| Error::source("malformed txoref txhash"))
                        .or_panic()?;

                    let output_ref = OutputRef::new(ref_tx_hash.into(), txo_ref.txo_index.into());

                    let era: Era = (txo.era as u16)
                        .try_into()
                        .map_err(|_| Error::source("unrecognised era"))
                        .or_panic()?;

                    ctx.insert_txo(&output_ref, txo.slot, era, txo.raw)
                }

                let payload = EnrichedBlockPayload::roll_forward(
                    block_ref_to_point(Some(block_ref.clone())),
                    block.raw,
                    ctx,
                    mutable,
                );

                self.output.send(payload).await.or_panic()?;

                self.chain_tip.set(block_ref.slot as i64)
            }
            Action::Reset(reset) => {
                let payload =
                    EnrichedBlockPayload::roll_back(block_ref_to_point(Some(reset.clone())));

                self.output.send(payload).await.or_panic()?;

                self.chain_tip.set(reset.slot as i64);
            }
            Action::Undo(_) => (),
            Action::Origin(_) => (),
        }

        Ok(())
    }
}

#[async_trait::async_trait(?Send)]
impl gasket::runtime::Worker for Worker {
    type WorkUnit = SourceWorkUnit;

    fn metrics(&self) -> gasket::metrics::Registry {
        gasket::metrics::Builder::new()
            .with_counter("received_blocks", &self.block_count)
            .with_gauge("chain_tip", &self.chain_tip)
            .build()
    }

    async fn bootstrap(&mut self) -> Result<(), gasket::error::Error> {
        debug!("bootstrapping, config: {:?}", self.config);

        let mut client = SyncServiceClient::connect(self.config.url.clone())
            .await
            .or_panic()?
            .accept_compressed(CompressionEncoding::Gzip)
            .max_decoding_message_size(256 * 1024 * 1024)
            .max_encoding_message_size(256 * 1024 * 1024);

        // first try intersect with persistent rollback buffer if one is found
        let intersect = if let Some(point) =
            utils::try_intersect_with_rollback_buf(&mut self.cursor, &mut client)
                .await
                .or_panic()?
        {
            // send rollback to intersect point, in case it is behind our tip
            let payload = EnrichedBlockPayload::roll_back(point.clone());
            self.output.send(payload).await.or_panic()?;

            point
        } else {
            // otherwise try intersect with cursor in storage, if one is found,
            // or use config if not
            utils::try_intersect_with_last_point_or_config(
                &self.intersect,
                &mut self.cursor,
                &mut client,
            )
            .await
            .or_panic()?
        };

        self.upstream_slot = client
            .page_blocks_with_context(PageBlocksWithCtxRequest {
                cursor: None,
                max_items: 1,
            })
            .await
            .or_panic()?
            .into_inner()
            .chain_tip
            .map(|x| x.slot);

        self.chain_tip.set(intersect.slot_or_default() as i64);

        self.client = Some(client);
        self.mode = Mode::Dumping(point_to_block_ref(intersect));

        Ok(())
    }

    async fn schedule(&mut self) -> ScheduleResult<Self::WorkUnit> {
        debug!("scheduling");

        // if we are dumping and in mutable zone, try switch to streaming by
        // intersecting with last processed block
        if let Some(upstream_slot) = self.upstream_slot {
            if let Mode::Dumping(Some(cursor)) = &self.mode {
                if cursor.slot + IMMUTABLE_AFTER_SLOTS >= upstream_slot {
                    let stream_req = StreamUpdatesWithCtxRequest {
                        intersects: vec![cursor.clone()],
                    };

                    info!("within mutable zone ({} v {}), trying to intersect with mutable with {cursor:?}", cursor.slot, upstream_slot);

                    match self
                        .client
                        .as_mut()
                        .unwrap()
                        .stream_updates_with_context(stream_req)
                        .await
                    {
                        Ok(stream) => {
                            info!("switching to streaming (intersected using {cursor:?})");

                            self.mode = Mode::Streaming;
                            self.stream = Some(stream.into_inner())
                        }
                        Err(err) if err.code() == Code::NotFound => (),
                        e @ Err(_) => {
                            e.or_panic()?;
                        }
                    }
                } else {
                    debug!(
                        "not yet in mutable zone ({} v {})",
                        cursor.slot, upstream_slot
                    )
                }
            }
        }

        match &self.mode {
            Mode::Init => unreachable!(),
            // if we are streaming, await next action and schedule it
            Mode::Streaming => {
                let stream = self.stream.as_mut().unwrap();
                let next = stream
                    .next()
                    .await
                    .ok_or(Error::source("source stream ended"))
                    .or_panic()?;

                if let Some(action) = next.or_panic()?.action {
                    return Ok(WorkSchedule::Unit(SourceWorkUnit {
                        actions: vec![action],
                        mutable: true,
                    }));
                };

                warn!("stream entry had no action");

                return Ok(WorkSchedule::Idle);
            }
            // if we are dumping, fetch the next page and schedule all the
            // blocks as Apply actions
            Mode::Dumping(cursor) => {
                let dump_request = PageBlocksWithCtxRequest {
                    cursor: cursor.clone(),
                    max_items: self.config.max_items_per_page.unwrap_or(20),
                };

                debug!("compressor requesting page: {dump_request:?}");

                let result = self
                    .client
                    .as_mut()
                    .unwrap()
                    .page_blocks_with_context(dump_request)
                    .await
                    .or_panic()?
                    .into_inner();

                if let Some(last_block) = result.blocks.last() {
                    self.mode = Mode::Dumping(last_block.r#ref.clone());
                }

                self.upstream_slot = result.chain_tip.map(|x| x.slot);

                let actions: Vec<Action> = result.blocks.into_iter().map(Action::Apply).collect();

                if !actions.is_empty() {
                    debug!("compressor scheduling {} actions", actions.len());
                    Ok(WorkSchedule::Unit(SourceWorkUnit {
                        actions,
                        mutable: false,
                    }))
                } else {
                    warn!("page contained no blocks");
                    Ok(WorkSchedule::Idle)
                }
            }
        }
    }

    async fn execute(&mut self, unit: &Self::WorkUnit) -> Result<(), gasket::error::Error> {
        let mutable = unit.mutable;

        debug!(
            "compressor processing actions (first: {:?})",
            unit.actions.last()
        );
        for action in unit.actions.clone() {
            self.process_action(action, mutable).await.or_panic()?;
        }
        debug!("compressor finished processing actions");

        Ok(())
    }
}
