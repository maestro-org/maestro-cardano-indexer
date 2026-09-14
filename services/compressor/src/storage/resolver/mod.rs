use crate::storage::kvtable::*;
use pallas::crypto::hash::Hash;

use super::{TxoBody, TxoRef};

// block slot -> (hash, required transaction output bytes)
pub struct ResolverBySlotKV;

// txos required by block, and whether the txo was produced within the same
// block (required knowledge for rollbacking)

pub type ResolverValue = Vec<(TxoRef, TxoBody, bool)>;

impl KVTable<DBInt, DBSerde<(Hash<32>, ResolverValue)>> for ResolverBySlotKV {
    const CF_NAME: &'static str = "ResolverBySlotKV";
}
