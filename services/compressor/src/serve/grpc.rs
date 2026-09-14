use std::pin::Pin;

use futures_core::Stream;
use futures_util::StreamExt;
use tonic::{Request, Response, Status};
use tracing::{error, info};

use crate::storage::{
    chain::BlockBySlotKV,
    kvtable::{DBInt, DBSerde, KVTable},
    mutable::{Log, MutableKV},
    resolver::ResolverBySlotKV,
    ChainDB, TxoRef,
};

pub mod compressor_api {
    tonic::include_proto!("compressor.sync.v1"); // The string specified here must match the proto package name
}

use compressor_api::*;

use self::stream_updates_with_ctx_response::Action;

pub struct SyncServerImpl {
    chain_db: ChainDB,
}

impl SyncServerImpl {
    pub fn new(chain_db: ChainDB) -> Self {
        Self { chain_db }
    }
}

#[async_trait::async_trait]
impl sync_service_server::SyncService for SyncServerImpl {
    type StreamUpdatesWithContextStream =
        Pin<Box<dyn Stream<Item = Result<StreamUpdatesWithCtxResponse, Status>> + Send + 'static>>;

    async fn page_blocks_with_context(
        &self,
        request: Request<PageBlocksWithCtxRequest>,
    ) -> Result<Response<PageBlocksWithCtxResponse>, Status> {
        let request = request.into_inner();

        if request.max_items < 1 {
            return Err(Status::invalid_argument("max items must be greater than 0"));
        }

        let max_items = request.max_items as usize;

        info!(
            "received page blocks with context request, {max_items} items from cursor: {:?}",
            request.cursor.as_ref().map(|x| hex::encode(&x.hash))
        );

        let db_tx = self.chain_db.db.snapshot();

        // --- start chain iterator and check intersect on chain

        let intersect_hash = request.cursor.clone().map(|x| x.hash);
        let intersect_slot = request.cursor.as_ref().map(|x| x.slot).unwrap_or_default();

        let instant_a = tokio::time::Instant::now();

        let mut block_by_slot_iter = BlockBySlotKV::iter_entries_from_snapshot(
            &self.chain_db.db,
            &db_tx,
            DBInt(intersect_slot),
        );

        // if we have an intersect hash, get the intersect entry from  block by slot
        if let Some(hash) = intersect_hash.clone() {
            let (DBInt(found_slot), DBSerde((_, found_hash, _))) = block_by_slot_iter
                .next()
                .ok_or(Status::not_found("intersect not found (no entry)"))?
                .map_err(Status::internal)?;

            if found_slot != intersect_slot {
                return Err(Status::not_found("intersect not found (slot mismatch)"));
            };

            if found_hash.to_vec() != hash {
                return Err(Status::not_found("intersect not found (hash mismatch)"));
            }
        }

        // --- take page of blocks

        let mut page_blocks = block_by_slot_iter
            .take(max_items + 1)
            .collect::<Result<Vec<_>, _>>()
            .map_err(Status::internal)?;

        let next_cursor = if page_blocks.len() == max_items + 1 {
            page_blocks.remove(max_items);
            page_blocks
                .last()
                .map(|(DBInt(slot), DBSerde((_, hash, _)))| BlockRef {
                    slot: *slot,
                    hash: hash.to_vec(),
                })
        } else {
            None
        };

        let blockfetch_duration = instant_a.elapsed();

        // --- fetch resolvers

        let instant_b = tokio::time::Instant::now();

        let mut resolver_by_slot_iter = ResolverBySlotKV::iter_entries_from_snapshot(
            &self.chain_db.db,
            &db_tx,
            DBInt(intersect_slot),
        );

        if intersect_hash.is_some() {
            resolver_by_slot_iter.next(); // skip intersect if needed
        }

        let page_resolvers = resolver_by_slot_iter
            .take(max_items)
            .collect::<Result<Vec<_>, _>>()
            .map_err(Status::internal)?;

        let resolving_duration = instant_b.elapsed();

        // ---

        if page_blocks.len() != page_resolvers.len() {
            error!(
                "blocks/resolvers len mismatch {:?} vs {:?}",
                page_blocks
                    .iter()
                    .map(|x| x.0 .0.clone())
                    .collect::<Vec<_>>(),
                page_resolvers
                    .iter()
                    .map(|x| x.0 .0.clone())
                    .collect::<Vec<_>>()
            );
            return Err(Status::internal("blocks/resolvers len mismatch"));
        }

        let mut page = vec![];

        for (block, block_resolver) in page_blocks.into_iter().zip(page_resolvers) {
            let (DBInt(slot), DBSerde((_, block_hash, bytes))) = block;
            let (_, DBSerde((resolver_hash, resolver))) = block_resolver;

            if block_hash != resolver_hash {
                error!(
                    "blocks/resolvers hash mismatch ({}, {}, {})",
                    slot, block_hash, resolver_hash
                );

                return Err(Status::internal("blocks/resolvers hash mismatch"));
            }

            let txo_resolver = resolver
                .into_iter()
                .map(
                    |(TxoRef(ref_hash, ref_index), (txo_slot, era, raw), _)| ResolvedTxo {
                        r#ref: Some(compressor_api::TxoRef {
                            tx_hash: ref_hash.to_vec(),
                            txo_index: ref_index as u32,
                        }),
                        slot: txo_slot,
                        era: era as u32,
                        raw,
                    },
                )
                .collect();

            page.push(BlockWithContext {
                r#ref: Some(BlockRef {
                    slot,
                    hash: block_hash.to_vec(),
                }),
                raw: bytes,
                txo_resolver,
            });
        }

        // ---

        let chain_tip = match BlockBySlotKV::iter_entries_snapshot(
            &self.chain_db.db,
            &db_tx,
            rocksdb::IteratorMode::End,
        )
        .next()
        {
            Some(entry) => {
                let (DBInt(tip_slot), DBSerde((_, tip_hash, _))) =
                    entry.map_err(Status::internal)?;

                Some(BlockRef {
                    slot: tip_slot,
                    hash: tip_hash.to_vec(),
                })
            }
            None => None,
        };

        // ---

        let response = PageBlocksWithCtxResponse {
            blocks: page,
            next_cursor,
            chain_tip,
        };

        info!(
            "finished processing page with context req (fetching: {:?}ms, resolving: {:?}ms)",
            blockfetch_duration.as_millis(),
            resolving_duration.as_millis()
        );

        Ok(Response::new(response))
    }

    async fn stream_updates_with_context(
        &self,
        request: Request<StreamUpdatesWithCtxRequest>,
    ) -> std::result::Result<Response<Self::StreamUpdatesWithContextStream>, Status> {
        let db_tx = self.chain_db.db.transaction();

        for intersect in request.into_inner().intersects {
            let BlockRef {
                slot: i_slot,
                hash: i_hash,
            } = intersect;

            let i_hash: [u8; 32] = i_hash
                .try_into()
                .map_err(|_| Status::invalid_argument("invalid intersect block hash"))?;

            let maybe_wal_seq =
                MutableKV::find_wal_seq(&self.chain_db.db, &db_tx, i_slot, i_hash.into())
                    .map_err(Status::internal)?;

            db_tx.rollback().map_err(Status::internal)?;

            if let Some(wal_seq) = maybe_wal_seq {
                let stream = MutableKV::stream_mutable(&self.chain_db, wal_seq).map(|x| match x {
                    Ok(log) => Ok(log_to_response_with_ctx(log)),
                    Err(_) => Err(Status::internal("streamupdateswithctx returned error")),
                });

                return Ok(Response::new(Box::pin(stream)));
            }
        }

        return Err(Status::not_found(
            "no intersect found with mutable part of chain",
        ));
    }
}

fn log_to_response_with_ctx(log: Log) -> StreamUpdatesWithCtxResponse {
    let action = match log {
        Log::Apply(slot, hash, body, resolver) => {
            let txo_resolver = resolver
                .into_iter()
                .map(
                    |(TxoRef(ref_hash, ref_index), (txo_slot, era, raw))| ResolvedTxo {
                        r#ref: Some(compressor_api::TxoRef {
                            tx_hash: ref_hash.to_vec(),
                            txo_index: ref_index as u32,
                        }),
                        slot: txo_slot,
                        era: era as u32,
                        raw,
                    },
                )
                .collect();

            let block_with_ctx = BlockWithContext {
                r#ref: Some(BlockRef {
                    slot,
                    hash: hash.to_vec(),
                }),
                raw: body,
                txo_resolver,
            };

            Action::Apply(block_with_ctx)
        }
        Log::Undo(slot, hash, _) => Action::Undo(BlockRef {
            slot,
            hash: hash.to_vec(),
        }),
        Log::Mark(slot, hash, _) => Action::Reset(BlockRef {
            slot,
            hash: hash.to_vec(),
        }),
        Log::Origin => Action::Origin(Null {}),
    };

    StreamUpdatesWithCtxResponse {
        action: Some(action),
    }
}
