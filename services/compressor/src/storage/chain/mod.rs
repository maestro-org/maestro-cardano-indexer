use crate::storage::kvtable::*;

use super::BlockValue;

// block slot -> block hash, height, body bytes
pub struct BlockBySlotKV;

impl KVTable<DBInt, DBSerde<BlockValue>> for BlockBySlotKV {
    const CF_NAME: &'static str = "BlockBySlotKV";
}
