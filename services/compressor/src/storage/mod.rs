use pallas::{crypto::hash::Hash, ledger::traverse::MultiEraBlock};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub mod chain;
pub mod kvtable;
pub mod mutable;
pub mod options;
pub mod resolver;
pub mod txos;

use chain::BlockBySlotKV;
use kvtable::*;
use mutable::{Log, MutableKV};
use tracing::{debug, info, warn};
use txos::TxoKV;

type Era = u16;
type TxHash = Hash<32>;
type OutputIndex = u64;
type TxoBody = (BlockSlot, Era, Vec<u8>);
pub type BlockValue = (BlockHeight, BlockHash, BlockBody);
type BlockSlot = u64;
type BlockHeight = u64;
type BlockHash = Hash<32>;
type BlockBody = Vec<u8>;

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Hash, Debug)]
pub struct TxoRef(pub TxHash, pub OutputIndex);

#[derive(Error, Debug)]
pub enum Error {
    #[error("RocksDb error: {0}")]
    Rocks(rocksdb::Error),

    #[error("Missing block on undo: {0}")]
    UndoMissingBlock(Hash<32>),

    #[error("Failed to decode data: {0}")]
    Decoding(String),

    #[error("Malformed genesis txo: {0}")]
    GenesisTxo(String),

    #[error("Missing txo to resolve for block {0}: {1}#{2}")]
    MissingTxo(Hash<32>, Hash<32>, u64),

    #[error("Missing resolver entry for block {0}")]
    MissingResolver(Hash<32>),

    #[error("Resolver for slot {0} hash mismatch: {1} v {2}")]
    ResolverHashMismatch(u64, Hash<32>, Hash<32>),

    #[error("Invalid rollback point, found: ({0:?})")]
    InvalidRollbackPoint(Option<(u64, Hash<32>)>),

    #[error("Applying block {applying} does not immediately follow last block ({received_prev:?} != {expected_prev:?})")]
    PreviousHashMismatch {
        applying: Hash<32>,
        received_prev: Option<Hash<32>>,
        expected_prev: Option<Hash<32>>,
    },
}

impl Into<String> for Error {
    fn into(self) -> String {
        self.to_string()
    }
}

use std::{collections::HashMap, path::Path, sync::Arc, time};

use rocksdb::{
    Options, SnapshotWithThreadMode, Transaction, TransactionDB, TransactionDBOptions, DB,
};

use crate::storage::resolver::ResolverBySlotKV;

#[derive(Clone)]
pub struct ChainDB {
    pub db: Arc<TransactionDB>,
    notifier: Arc<tokio::sync::Notify>,
    next_wal_seq: u64,
    last_block_hash: Option<Hash<32>>,
    pub immutable_after_slots: Option<u64>,
}

impl ChainDB {
    pub fn open(path: impl AsRef<Path>, immutable_after_slots: Option<u64>) -> Result<Self, Error> {
        let opts = crate::storage::options::db_options();

        let now = time::Instant::now();

        let cfs = [
            rocksdb::ColumnFamilyDescriptor::new(BlockBySlotKV::CF_NAME, opts.clone()),
            rocksdb::ColumnFamilyDescriptor::new(MutableKV::CF_NAME, opts.clone()),
            rocksdb::ColumnFamilyDescriptor::new(ResolverBySlotKV::CF_NAME, opts.clone()),
            rocksdb::ColumnFamilyDescriptor::new(TxoKV::CF_NAME, opts.clone()),
        ];

        // Important: open_cf uses default config! We need to call open_cf_descriptors.
        // https://github.com/rust-rocksdb/rust-rocksdb/issues/686#issuecomment-1246902659
        let db =
            TransactionDB::open_cf_descriptors(&opts, &TransactionDBOptions::default(), path, cfs)
                .map_err(Error::Rocks)?;

        let tx = db.transaction();

        let time_to_open = now.elapsed().as_secs();

        if time_to_open >= 5 * 60 {
            warn!("opened chaindb in {time_to_open} seconds");
        } else {
            info!("opened chaindb in {time_to_open} seconds");
        }

        let notifier = Arc::new(tokio::sync::Notify::new());

        let last_block_hash = BlockBySlotKV::last_entry(&db, &tx)?.map(|(_, DBSerde((_, h, _)))| h);

        tx.commit().map_err(Error::Rocks)?;

        // clear MutableKV on reboot
        MutableKV::reset(&db)?;

        Ok(Self {
            db: Arc::new(db),
            notifier,
            next_wal_seq: 1,
            last_block_hash,
            immutable_after_slots,
        })
    }

    pub fn is_empty(&self, tx: &Transaction<TransactionDB>) -> bool {
        BlockBySlotKV::is_empty(&self.db, tx)
    }

    pub fn cursor(&self) -> Result<Option<(BlockSlot, BlockHeight, BlockHash)>, Error> {
        let tx = self.db.transaction();

        let v = BlockBySlotKV::last_entry(&self.db, &tx)?;
        let out = v.map(|(DBInt(s), DBSerde((n, h, _)))| (s, n, h));

        tx.commit().map_err(Error::Rocks)?;

        Ok(out)
    }

    pub fn snapshot(&self) -> SnapshotWithThreadMode<'_, TransactionDB> {
        self.db.snapshot()
    }

    pub fn intersect_options(
        &self,
        max_items: usize,
    ) -> Result<Vec<(BlockSlot, BlockHash)>, Error> {
        let db_tx = self.db.transaction();

        let mut iter = BlockBySlotKV::iter_entries(&self.db, &db_tx, rocksdb::IteratorMode::End);

        let mut out = Vec::with_capacity(max_items);

        // crawl the adopted chain exponentially
        while let Some(entry) = iter.next() {
            let (DBInt(slot), DBSerde((_h, hash, _b))) = entry?;

            out.push((slot, hash));

            if out.len() >= max_items {
                break;
            }

            let skip = 2usize.pow(out.len() as u32) - 1;
            for _ in 0..skip {
                iter.next();
            }
        }

        Ok(out)
    }

    pub fn destroy(path: impl AsRef<Path>) -> Result<(), Error> {
        DB::destroy(&Options::default(), path).map_err(Error::Rocks)?;

        Ok(())
    }

    pub fn apply_block_with_context(
        &mut self,
        block_bytes: BlockBody,
        mutable: bool,
    ) -> Result<(), Error> {
        let block =
            MultiEraBlock::decode(&block_bytes).map_err(|e| Error::Decoding(e.to_string()))?;

        let slot = block.slot();
        let hash = block.hash();
        let height = block.number();
        let previous_hash = block.header().previous_hash();

        if self.last_block_hash.is_some() {
            if previous_hash != self.last_block_hash {
                return Err(Error::PreviousHashMismatch {
                    applying: hash,
                    received_prev: previous_hash,
                    expected_prev: self.last_block_hash,
                });
            }
        }

        let mut db_tx = self.db.transaction();

        debug!("applying block ({slot}, {hash}) (mutable: {mutable})");

        // --- resolve required utxos using utxos produced by the block and the
        // utxo table

        let resolver = TxoKV::resolve_block_tx_inputs(&self.db, &mut db_tx, block.clone())?;

        // --- if we are mutable, insert an Apply log entry in the mutableKV

        if mutable {
            let resolver_less_flag = resolver
                .clone()
                .into_iter()
                .map(|(k, (v, _))| (k, v))
                .collect();

            let log = Log::Apply(slot, hash, block_bytes.clone(), resolver_less_flag);

            MutableKV::stage_upsert(&self.db, DBInt(self.next_wal_seq), DBSerde(log), &mut db_tx)?;
        }

        // --- insert block bytes into BlockBySlotKV

        BlockBySlotKV::stage_upsert(
            &self.db,
            DBInt(slot),
            DBSerde((height, hash, block_bytes.clone())),
            &mut db_tx,
        )?;

        // --- insert produced txos into TxoKV, and removed consumed txos

        for tx in block.txs() {
            for (idx, produced) in tx.produces() {
                let body = produced.encode();
                let era = tx.era().into();
                let txo_ref = TxoRef(tx.hash(), idx as u64);

                TxoKV::stage_upsert(
                    &self.db,
                    DBSerde(txo_ref),
                    DBSerde((block.slot(), era, body)),
                    &mut db_tx,
                )?;
            }

            for input in tx.consumes() {
                let txo_ref = TxoRef(*input.hash(), input.index());

                TxoKV::stage_delete(&self.db, DBSerde(txo_ref), &mut db_tx)?;
            }
        }

        // --- insert required txos into ResolverKV

        let resolver_vec = resolver
            .into_iter()
            .map(|(k, (v1, v2))| (k, v1, v2))
            .collect();

        ResolverBySlotKV::stage_upsert(
            &self.db,
            DBInt(slot),
            DBSerde((hash, resolver_vec)),
            &mut db_tx,
        )?;

        // --- prune MutableKV if necessary

        if mutable {
            if let Some(prune_after) = self.immutable_after_slots {
                if slot >= prune_after {
                    let _ = MutableKV::prune(&self.db, &mut db_tx, slot - prune_after);
                }
            }
        }

        // --- commit tx

        db_tx.commit().map_err(Error::Rocks)?;

        // ---

        self.next_wal_seq += 1;
        self.last_block_hash = Some(hash);

        self.notifier.notify_waiters();

        Ok(())
    }

    pub fn rollback(
        &mut self,
        rb_slot: BlockSlot,
        rb_hash: BlockHash,
        mutable: bool,
    ) -> Result<(), Error> {
        info!("rolling back to ({rb_slot}, {rb_hash})");

        let mut db_tx = self.db.transaction();

        let mut next_wal_seq = self.next_wal_seq;

        // --- find points following rollback point

        let mut to_remove = BlockBySlotKV::iter_entries_from(&self.db, &db_tx, DBInt(rb_slot));

        let rb_bytes = match to_remove.next() {
            Some(entry) => {
                let (DBInt(found_slot), DBSerde((_, found_hash, found_bytes))) = entry?;

                if rb_slot != found_slot || rb_hash != found_hash {
                    return Err(Error::InvalidRollbackPoint(Some((found_slot, found_hash))));
                }

                found_bytes
            }
            None => return Err(Error::InvalidRollbackPoint(None)),
        };

        let to_remove = to_remove.collect::<Vec<_>>();

        // --- for each rollbacked point...

        for entry in to_remove.into_iter().rev() {
            let (DBInt(slot), DBSerde((height, hash, block_bytes))) = entry?;

            info!("undoing ({height}, {slot}, {hash})...");

            // --- add Undo action to MutableKV

            if mutable {
                let log = Log::Undo(slot, hash, block_bytes.clone());

                MutableKV::stage_upsert(&self.db, DBInt(next_wal_seq), DBSerde(log), &mut db_tx)?;

                next_wal_seq += 1;
            }

            // --- remove entry from BlockBySlotKV

            BlockBySlotKV::stage_delete(&self.db, DBInt(slot), &mut db_tx)?;

            // --- remove txos produced by block from TxoKV, reinsert consumed

            let block =
                MultiEraBlock::decode(&block_bytes).map_err(|e| Error::Decoding(e.to_string()))?;

            let DBSerde((res_hash, resolver)) =
                ResolverBySlotKV::get_by_key(&self.db, &mut db_tx, DBInt(slot))?
                    .ok_or(Error::MissingResolver(hash))?;

            if res_hash != hash {
                return Err(Error::ResolverHashMismatch(slot, hash, res_hash));
            }

            let mut resolver: HashMap<_, _> =
                resolver.into_iter().map(|(x, y, _)| (x, y)).collect();

            for tx in block.txs().iter().rev() {
                for input in tx.consumes() {
                    let txo_ref = TxoRef(*input.hash(), input.index());

                    let body = resolver.remove(&txo_ref).ok_or(Error::MissingTxo(
                        block.hash(),
                        *input.hash(),
                        input.index(),
                    ))?;

                    TxoKV::stage_upsert(&self.db, DBSerde(txo_ref), DBSerde(body), &mut db_tx)?;
                }

                for (idx, _) in tx.produces() {
                    TxoKV::stage_delete(
                        &self.db,
                        DBSerde(TxoRef(tx.hash(), idx as u64)),
                        &mut db_tx,
                    )?
                }
            }

            // --- remove ResolverKV entry for block

            ResolverBySlotKV::stage_delete(&self.db, DBInt(slot), &mut db_tx)?;
        }

        // --- add Mark action to MutableKV for rollback point

        if mutable {
            let log = Log::Mark(rb_slot, rb_hash, rb_bytes);

            MutableKV::stage_upsert(&self.db, DBInt(next_wal_seq), DBSerde(log), &mut db_tx)?;

            next_wal_seq += 1;
        }

        // ---

        db_tx.commit().map_err(Error::Rocks)?;

        // ---

        self.next_wal_seq = next_wal_seq;
        self.last_block_hash = Some(rb_hash);

        self.notifier.notify_waiters();

        Ok(())
    }

    // Same as `rollback` but with Origin WAL Log instead of Mark
    pub fn rollback_to_origin(&mut self, mutable: bool) -> Result<(), Error> {
        let mut db_tx = self.db.transaction();

        let mut next_wal_seq = self.next_wal_seq;

        // --- find points following rollback point (all points)

        let to_remove = BlockBySlotKV::iter_entries_start(&self.db, &db_tx).collect::<Vec<_>>();

        // --- for each rollbacked point...

        for entry in to_remove.into_iter().rev() {
            let (DBInt(slot), DBSerde((_height, hash, block_bytes))) = entry?;

            // --- remove entry from HashBySlotKV

            BlockBySlotKV::stage_delete(&self.db, DBInt(slot), &mut db_tx)?;

            // --- add Undo action to MutableKV

            if mutable {
                let log = Log::Undo(slot, hash, block_bytes.clone());

                MutableKV::stage_upsert(&self.db, DBInt(next_wal_seq), DBSerde(log), &mut db_tx)?;

                next_wal_seq += 1;
            }

            // --- remove txos produced by block from TxoKV, reinsert consumed

            let block =
                MultiEraBlock::decode(&block_bytes).map_err(|e| Error::Decoding(e.to_string()))?;

            let DBSerde((res_hash, resolver)) =
                ResolverBySlotKV::get_by_key(&self.db, &mut db_tx, DBInt(slot))?
                    .ok_or(Error::MissingResolver(hash))?;

            if res_hash != hash {
                return Err(Error::ResolverHashMismatch(slot, hash, res_hash));
            }

            let mut resolver: HashMap<_, _> =
                resolver.into_iter().map(|(x, y, _)| (x, y)).collect();

            for tx in block.txs().iter().rev() {
                for input in tx.consumes() {
                    let txo_ref = TxoRef(*input.hash(), input.index());

                    let body = resolver.remove(&txo_ref).ok_or(Error::MissingTxo(
                        block.hash(),
                        *input.hash(),
                        input.index(),
                    ))?;

                    TxoKV::stage_upsert(&self.db, DBSerde(txo_ref), DBSerde(body), &mut db_tx)?;
                }

                for (idx, _) in tx.produces() {
                    TxoKV::stage_delete(
                        &self.db,
                        DBSerde(TxoRef(tx.hash(), idx as u64)),
                        &mut db_tx,
                    )?
                }
            }

            // --- remove ResolverKV entry for block

            ResolverBySlotKV::stage_delete(&self.db, DBInt(slot), &mut db_tx)?;
        }

        // --- add Mark action to MutableKV for rollback point

        if mutable {
            let log = Log::Origin;

            MutableKV::stage_upsert(&self.db, DBInt(next_wal_seq), DBSerde(log), &mut db_tx)?;

            next_wal_seq += 1;
        }

        // ---

        db_tx.commit().map_err(Error::Rocks)?;

        // ---

        self.next_wal_seq = next_wal_seq;
        self.last_block_hash = None;

        self.notifier.notify_waiters();

        Ok(())
    }
}
