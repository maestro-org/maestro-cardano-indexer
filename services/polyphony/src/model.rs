use std::{
    collections::HashMap,
    fmt::{self, Debug},
};

use pallas::{
    ledger::traverse::{Era, MultiEraOutput, MultiEraTx, OutputRef},
    network::miniprotocols::Point,
};

use crate::{prelude::*, reducers::ReducerOutput};

#[derive(Default, Debug, Clone)]
pub struct BlockContext {
    utxos: HashMap<String, (u64, Era, Vec<u8>)>,
}

impl BlockContext {
    pub fn new() -> Self {
        Self {
            utxos: HashMap::new(),
        }
    }

    pub fn insert_txo(&mut self, key: &OutputRef, slot: u64, era: Era, cbor: Vec<u8>) {
        self.utxos.insert(key.to_string(), (slot, era, cbor));
    }

    pub fn find_utxo(&'_ self, key: &OutputRef) -> Result<(u64, MultiEraOutput<'_>), Error> {
        let (slot, era, cbor) = self
            .utxos
            .get(&key.to_string())
            .ok_or_else(|| Error::missing_utxo(key))?;

        let txout = MultiEraOutput::decode(*era, cbor).map_err(crate::Error::cbor)?;

        Ok((*slot, txout))
    }

    pub fn get_all_keys(&self) -> Vec<String> {
        self.utxos.keys().cloned().collect()
    }

    #[allow(clippy::type_complexity)]
    pub fn find_consumed_txos(
        &'_ self,
        tx: &MultiEraTx,
        policy: &RuntimePolicy,
    ) -> Result<Vec<(u64, (u64, MultiEraOutput<'_>))>, Error> {
        let items = tx
            .consumes()
            .iter()
            .map(|i| i.output_ref())
            .map(|r| self.find_utxo(&r).map(|u| (r.index(), u)))
            .map(|r| r.apply_policy(policy))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();

        Ok(items)
    }
}

#[derive(Debug, Clone)]
pub enum EnrichedBlockPayload {
    RollForward(Point, Vec<u8>, BlockContext, bool),
    RollBack(Point),
}

impl EnrichedBlockPayload {
    pub fn roll_forward(
        point: Point,
        block: Vec<u8>,
        ctx: BlockContext,
        mutable: bool,
    ) -> gasket::messaging::Message<Self> {
        gasket::messaging::Message {
            payload: Self::RollForward(point, block, ctx, mutable),
        }
    }

    pub fn roll_back(point: Point) -> gasket::messaging::Message<Self> {
        gasket::messaging::Message {
            payload: Self::RollBack(point),
        }
    }
}

#[derive(Debug, Clone)]
pub enum StorageActionPayload {
    RollForward(Point, Vec<ReducerOutput>, bool),
    RollBack(Point),
}

pub type Key = Vec<u8>;
pub type Value = Vec<u8>;
pub type DeltaU64 = u64;
pub type DeltaU128 = u128;

pub type Height = u64;

#[derive(Clone)]
#[non_exhaustive]
pub enum StorageAction {
    /// Set `Key` to `Value`
    Set(Key, Value),

    /// Delete the KV pair with key `Key`
    Delete(Key),

    /// Set `Key` to `Value` only if `Key` does not already point to a value
    Insert(Key, Value),

    /// Increment the value at `Key` by `Delta`
    /// (If value at Key exists, it must be big endian u64)
    IncrementU64(Key, DeltaU64),

    /// Decrement the value at `Key` by `Delta`
    /// (If value at Key exists, it must be big endian u64)
    DecrementU64(Key, DeltaU64),

    /// Decrement the value at `Key` by `Delta`
    /// (If value at Key exists, it must be big endian u128)
    IncrementU128(Key, DeltaU128),

    /// Decrement the value at `Key` by `Delta` (do not remove the key if the
    /// resulting value is 0) (If value at Key exists, it must be big endian
    /// u64)
    DecrementU128NoDelete(Key, DeltaU128),
}

impl fmt::Debug for StorageAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Set(k, v) => write!(
                f,
                "StorageAction::Set([{}] -> [{}])",
                hex::encode(k),
                hex::encode(v)
            ),
            Self::Insert(k, v) => write!(
                f,
                "StorageAction::Insert([{}] -> [{}])",
                hex::encode(k),
                hex::encode(v)
            ),
            Self::Delete(k) => write!(f, "StorageAction::Del({})", hex::encode(k)),
            Self::IncrementU64(k, d) => {
                write!(f, "StorageAction Incr([{}] += {})", hex::encode(k), d)
            }
            Self::DecrementU64(k, d) => {
                write!(f, "StorageAction Decr([{}] -= {})", hex::encode(k), d)
            }
            Self::IncrementU128(k, d) => {
                write!(f, "StorageAction Incr128([{}] += {})", hex::encode(k), d)
            }
            Self::DecrementU128NoDelete(k, d) => {
                write!(f, "StorageAction DecrNoD128([{}] -= {})", hex::encode(k), d)
            }
        }
    }
}

impl StorageAction {
    pub fn key(&self) -> &Vec<u8> {
        match self {
            StorageAction::Set(k, _) => k,
            StorageAction::Delete(k) => k,
            StorageAction::Insert(k, _) => k,
            StorageAction::IncrementU64(k, _) => k,
            StorageAction::DecrementU64(k, _) => k,
            StorageAction::IncrementU128(k, _) => k,
            StorageAction::DecrementU128NoDelete(k, _) => k,
        }
    }

    pub fn into_key(self) -> Vec<u8> {
        match self {
            StorageAction::Set(k, _) => k,
            StorageAction::Delete(k) => k,
            StorageAction::Insert(k, _) => k,
            StorageAction::IncrementU64(k, _) => k,
            StorageAction::DecrementU64(k, _) => k,
            StorageAction::IncrementU128(k, _) => k,
            StorageAction::DecrementU128NoDelete(k, _) => k,
        }
    }

    /// Returns true if the previous value of the key is required in order to
    /// perform the storage action.
    pub fn requires_previous_value(&self) -> bool {
        matches!(
            self,
            StorageAction::IncrementU64(_, _)
                | StorageAction::DecrementU64(_, _)
                | StorageAction::IncrementU128(_, _)
                | StorageAction::DecrementU128NoDelete(_, _)
                | StorageAction::Insert(_, _)
        )
    }

    /// Panics if StorageActions can't be merged
    pub fn merge(&mut self, other: StorageAction) {
        assert_eq!(
            self.key(),
            other.key(),
            "trying to merge actions with different keys"
        );
        match self {
            Self::IncrementU64(k, pd) => {
                match other {
                    Self::IncrementU64(nk, nd) => *self = Self::IncrementU64(nk, *pd + nd),
                    Self::DecrementU64(nk, nd) => {
                        if *pd >= nd {
                            *self = Self::IncrementU64(nk, *pd - nd)
                        } else {
                            // pd < nd
                            *self = Self::DecrementU64(nk, nd - *pd)
                        }
                    }
                    a => panic!(
                        "unexpected INCR/DECR type pair in storage action merge {a:?} on {k:?}"
                    ),
                }
            }
            Self::DecrementU64(k, pd) => match other {
                Self::DecrementU64(nk, nd) => *self = Self::DecrementU64(nk, *pd + nd),
                Self::IncrementU64(nk, nd) => {
                    if *pd >= nd {
                        *self = Self::DecrementU64(nk, *pd - nd)
                    } else {
                        *self = Self::IncrementU64(nk, nd - *pd)
                    }
                }
                a => panic!("trying to merge SET/DEL/INS on DECR {a:?} {k:?} {pd:?}"),
            },
            Self::IncrementU128(k, pd) => {
                match other {
                    Self::IncrementU128(nk, nd) => *self = Self::IncrementU128(nk, *pd + nd),
                    Self::DecrementU128NoDelete(nk, nd) => {
                        if *pd >= nd {
                            *self = Self::IncrementU128(nk, *pd - nd)
                        } else {
                            // pd < nd
                            *self = Self::DecrementU128NoDelete(nk, nd - *pd)
                        }
                    }
                    a => panic!(
                        "unexpected INCR/DECR type pair in storage action merge {a:?} on {k:?}"
                    ),
                }
            }
            Self::DecrementU128NoDelete(k, pd) => match other {
                Self::DecrementU128NoDelete(nk, nd) => {
                    *self = Self::DecrementU128NoDelete(nk, *pd + nd)
                }
                Self::IncrementU128(nk, nd) => {
                    if *pd >= nd {
                        *self = Self::DecrementU128NoDelete(nk, *pd - nd)
                    } else {
                        *self = Self::IncrementU128(nk, nd - *pd)
                    }
                }
                a => panic!("trying to merge SET/DEL/INS on DECR {a:?} {k:?} {pd:?}"),
            },
            Self::Set(k, _) => match other {
                a @ (Self::IncrementU64(_, _)
                | Self::DecrementU64(_, _)
                | Self::IncrementU128(_, _)
                | Self::DecrementU128NoDelete(_, _)) => {
                    panic!("trying to merge INCR/DECR on SET {a:?} {k:?}")
                }
                a @ (Self::Set(_, _) | Self::Delete(_)) => *self = a, // overwrite SET with SET/DEL
                Self::Insert(_, _) => (), // do nothing - don't overwrite with an INS
            },
            Self::Delete(k) => match other {
                a @ (Self::IncrementU64(_, _)
                | Self::DecrementU64(_, _)
                | Self::IncrementU128(_, _)
                | Self::DecrementU128NoDelete(_, _)) => {
                    panic!("trying to merge INCR/DECR on DEL {a:?} {k:?}")
                }
                // Delete merged with Insert is a Set, because the value is cleared
                Self::Insert(k, v) => *self = Self::Set(k, v),
                // Overwrite DEL with SET/DEL
                a @ (Self::Set(_, _) | Self::Delete(_)) => *self = a,
            },
            Self::Insert(k, _) => match other {
                a @ (Self::IncrementU64(_, _)
                | Self::DecrementU64(_, _)
                | Self::IncrementU128(_, _)
                | Self::DecrementU128NoDelete(_, _)) => {
                    panic!("trying to merge INCR/DECR on INS {a:?} {k:?}")
                }
                // Overwrite INS with SET/DEL
                a @ (Self::Set(_, _) | Self::Delete(_)) => *self = a,
                // Don't overwrite an INS with another INS
                Self::Insert(_, _) => (),
            },
        }
    }

    /// Returns the total number of bytes for the key and value (estimate)
    pub fn size(&self) -> usize {
        match self {
            StorageAction::Set(k, v) => k.len() + v.len(),
            StorageAction::Delete(k) => k.len(),
            StorageAction::Insert(k, v) => k.len() + v.len(),
            StorageAction::IncrementU64(k, _) => k.len() + 8,
            StorageAction::DecrementU64(k, _) => k.len() + 8,
            StorageAction::IncrementU128(k, _) => k.len() + 16,
            StorageAction::DecrementU128NoDelete(k, _) => k.len() + 16,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::StorageAction;

    #[test]
    fn test_merge() {
        let op1 = StorageAction::IncrementU64(vec![1], 10);
        let op2 = StorageAction::DecrementU64(vec![1], 15);

        let mut map = HashMap::new();

        map.insert(vec![1], op1);
        map.insert(vec![2], StorageAction::Set(vec![2], vec![]));

        if let Some(prev) = map.get_mut(op2.key()) {
            prev.merge(op2)
        }

        // an increment of 10 merged with a decrement of 15 nets to a decrement of 5
        assert!(matches!(
            map.get(&vec![1]),
            Some(StorageAction::DecrementU64(_, 5))
        ));
    }
}
