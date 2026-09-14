use gasket::runtime::{ScheduleResult, WorkSchedule};
use pallas::crypto::hash::Hash;
use pallas::{ledger::traverse::MultiEraBlock, network::miniprotocols::Point};
use tracing::{debug, error};

use crate::{
    crosscut,
    model::{self, EnrichedBlockPayload, StorageActionPayload},
    prelude::*,
};

use super::Reducer;

type InputPort = gasket::messaging::tokio::InputPort<model::EnrichedBlockPayload>;
type OutputPort = gasket::messaging::tokio::OutputPort<model::StorageActionPayload>;

pub struct Worker {
    input: InputPort,
    output: OutputPort,
    reducers: Vec<Reducer>,
    policy: crosscut::policies::RuntimePolicy,
    ops_count: gasket::metrics::Counter,
    last_block: gasket::metrics::Gauge,
    last_processed: Option<Hash<32>>,
}

impl Worker {
    pub fn new(
        reducers: Vec<Reducer>,
        input: InputPort,
        output: OutputPort,
        policy: crosscut::policies::RuntimePolicy,
    ) -> Self {
        Worker {
            reducers,
            input,
            output,
            policy,
            ops_count: Default::default(),
            last_block: Default::default(),
            last_processed: None,
        }
    }

    async fn reduce_block<'b>(
        &mut self,
        point: Point,
        block: &'b [u8],
        ctx: &model::BlockContext,
        mutable: bool,
    ) -> Result<(), gasket::error::Error> {
        debug!("reducing block {:?}", point);

        let block = MultiEraBlock::decode(block)
            .map_err(crate::Error::cbor)
            .apply_policy(&self.policy)
            .or_panic()?;

        let block = match block {
            Some(x) => x,
            None => return Ok(()),
        };

        if let Some(prev) = self.last_processed {
            if let Some(found_prev) = block.header().previous_hash() {
                if prev != found_prev {
                    error!("previous block hash mismatch: {} vs {}", prev, found_prev);
                }
            }
        }

        self.last_block.set(block.number() as i64);

        let mut outputs = Vec::new();

        // Instead of passing the output port to the reducers, we pass a vec which
        // we will add all the storage actions to, then we will send these down
        // the outport port later.
        for reducer in self.reducers.iter_mut() {
            reducer.reduce_block(&block, ctx, &mut outputs)?;
            self.ops_count.inc(1);
        }

        outputs.push(super::ReducerOutput::Cursor((
            point.clone(),
            block.number(),
        )));

        debug!(
            "finished reducing block {:?} resulting in {} outputs",
            point,
            outputs.len()
        );

        self.output
            .send(gasket::messaging::Message::from(
                StorageActionPayload::RollForward(point, outputs, mutable),
            ))
            .await?;

        self.last_processed = Some(block.hash());

        Ok(())
    }
}

#[async_trait::async_trait(?Send)]
impl gasket::runtime::Worker for Worker {
    type WorkUnit = EnrichedBlockPayload;

    fn metrics(&self) -> gasket::metrics::Registry {
        gasket::metrics::Builder::new()
            .with_counter("ops_count", &self.ops_count)
            .with_gauge("last_block", &self.last_block)
            .build()
    }

    async fn schedule(&mut self) -> ScheduleResult<Self::WorkUnit> {
        let msg = self.input.recv().await?;

        Ok(WorkSchedule::Unit(msg.payload))
    }

    async fn execute(&mut self, unit: &Self::WorkUnit) -> Result<(), gasket::error::Error> {
        match unit {
            model::EnrichedBlockPayload::RollForward(point, block, ctx, mutable) => {
                self.reduce_block(point.clone(), block, &ctx.clone(), *mutable)
                    .await
            }
            // notify storage stage of the rollback, which will handle reversing
            // the storage actions
            model::EnrichedBlockPayload::RollBack(point) => {
                match point {
                    Point::Origin => self.last_processed = None,
                    Point::Specific(_, hash) => self.last_processed = Some(hash[0..32].into()),
                };

                self.output
                    .send(gasket::messaging::Message::from(
                        StorageActionPayload::RollBack(point.clone()),
                    ))
                    .await
            }
        }
    }
}
