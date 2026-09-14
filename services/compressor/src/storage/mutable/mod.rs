use std::collections::HashMap;

use futures_core::Stream;
use rocksdb::{Transaction, TransactionDB};
use serde::{Deserialize, Serialize};
use tracing::{debug, error};

use crate::storage::kvtable::*;
use crate::storage::{BlockBody, BlockHash, BlockSlot};

use super::{ChainDB, Error, TxoBody, TxoRef};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Log {
    Apply(BlockSlot, BlockHash, BlockBody, HashMap<TxoRef, TxoBody>),
    Undo(BlockSlot, BlockHash, BlockBody),
    Mark(BlockSlot, BlockHash, BlockBody),
    Origin,
}

impl Log {
    pub fn slot(&self) -> Option<BlockSlot> {
        match self {
            Log::Apply(s, _, _, _) => Some(*s),
            Log::Undo(s, _, _) => Some(*s),
            Log::Mark(s, _, _) => Some(*s),
            Log::Origin => None,
        }
    }

    pub fn hash(&self) -> Option<&BlockHash> {
        match self {
            Log::Apply(_, h, _, _) => Some(h),
            Log::Undo(_, h, _) => Some(h),
            Log::Mark(_, h, _) => Some(h),
            Log::Origin => None,
        }
    }

    pub fn body(&self) -> Option<&BlockBody> {
        match self {
            Log::Apply(_, _, b, _) => Some(b),
            Log::Undo(_, _, b) => Some(b),
            Log::Mark(_, _, b) => Some(b),
            Log::Origin => None,
        }
    }

    pub fn is_apply(&self) -> bool {
        matches!(self, Log::Apply(..))
    }

    pub fn is_mark(&self) -> bool {
        matches!(self, Log::Mark(..))
    }

    pub fn is_undo(&self) -> bool {
        matches!(self, Log::Undo(..))
    }

    pub fn is_origin(&self) -> bool {
        matches!(self, Log::Origin)
    }
}

// sequence number => WAL action
pub struct MutableKV;

impl KVTable<DBInt, DBSerde<Log>> for MutableKV {
    const CF_NAME: &'static str = "MutableKV";
}

impl MutableKV {
    pub fn prune(
        db: &TransactionDB,
        db_tx: &Transaction<TransactionDB>,
        before_slot: u64,
    ) -> Result<(), Error> {
        let mut pruned = 0;

        for entry in Self::iter_entries_start(db, db_tx) {
            let (key, val) = entry?;

            // stop deleting entries once we reach the specified slot
            if val.slot().unwrap_or_default() >= before_slot {
                break;
            }

            Self::stage_delete(db, key, db_tx)?;
            pruned += 1;
        }

        debug!("pruned {pruned} mutableKV entries");

        Ok(())
    }

    // We are looking for a particular point, we start at the lowest point in
    // the mutable, and exit early if we reach a slot is greater than what we are looking for, exit
    pub fn find_wal_seq(
        db: &TransactionDB,
        tx: &Transaction<TransactionDB>,
        slot: BlockSlot,
        hash: BlockHash,
    ) -> Result<Option<u64>, Error> {
        let found = Self::scan_until_or(
            &db,
            tx,
            rocksdb::IteratorMode::Start,
            |v| {
                (v.is_apply() || v.is_mark())
                    && (v.slot() == Some(slot))
                    && (v.hash() == Some(&hash))
            },
            |v| v.slot().unwrap_or_default() >= slot,
        )?;

        Ok(found.map(|DBInt(n)| n))
    }

    pub fn stream_mutable(
        chain_db: &ChainDB,
        init_wal_seq: u64,
    ) -> impl Stream<Item = Result<Log, ()>> {
        let db = chain_db.db.clone();
        let notifier = chain_db.notifier.clone();

        async_stream::try_stream! {
            let mut last_seq = init_wal_seq;

            // iterate from our wal seq
            let mut initial_iterator = MutableKV::iter_entries_from_no_tx(&db, DBInt(init_wal_seq));

            // skip intersect
            initial_iterator.next();

            for entry in initial_iterator {
                let (DBInt(wal_seq), DBSerde(log)) = entry.map_err(|e| {
                    error!("rocks error in mutableKV async stream: {}", e);
                    ()
                })?;

                if wal_seq != (last_seq + 1) {
                    // close stream
                    error!("unexpected next wal seq in mutableKV async stream: {} -> ({}: {:?})", last_seq, wal_seq, log);
                    Err(())?
                }

                yield log;
                last_seq = wal_seq;
            }

            // exhausted initial iterator, create new iterator from last seq
            loop {
                notifier.notified().await;
                let mut iter = MutableKV::iter_entries_from_no_tx(&db, DBInt(last_seq));

                // skip intersect
                iter.next();

                for entry in iter {
                    let (DBInt(wal_seq), DBSerde(log)) = entry.map_err(|e| {
                        error!("rocks error in mutableKV async stream: {}", e);
                        ()
                    })?;

                    if wal_seq != (last_seq + 1) {
                        // close stream
                        error!("unexpected next wal seq in mutableKV async stream: {} -> ({}: {:?})", last_seq, wal_seq, log);
                        Err(())?
                    }

                    yield log;
                    last_seq = wal_seq;
                }
            }
        }
    }
}
