use redis::ToRedisArgs;
use tikv_client::{Timestamp as TiKVTimestamp, TimestampExt};

/// A cursor entry published to Redis after each committed block, recording the
/// TiKV commit timestamp for the chain point. These entries feed the tikv-gc
/// safepoint invariants, so the wire format matches what tikv-gc parses:
/// comma-separated `slot,block_hash,was_mempool,commit_ts,network,
/// chain_tip_slot,chain_tip_hash,mempool_view_ts`. The Cardano pipeline has no
/// mempool blocks, so `was_mempool` is always false, the chain tip mirrors the
/// entry's own point, and `mempool_view_ts` is zero.
#[derive(Debug)]
pub struct RedisEntry {
    pub slot: u64,
    pub block_hash: [u8; 32],
    pub commit_ts: TiKVTimestamp,
    pub network: String,
}

impl std::fmt::Display for RedisEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{},{},false,{},{},{},{},0",
            self.slot,
            hex::encode(self.block_hash),
            self.commit_ts.version(),
            self.network.to_lowercase(),
            self.slot,
            hex::encode(self.block_hash),
        )
    }
}

impl ToRedisArgs for RedisEntry {
    fn write_redis_args<W>(&self, out: &mut W)
    where
        W: ?Sized + redis::RedisWrite,
    {
        out.write_arg(self.to_string().as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_wire_format() {
        let entry = RedisEntry {
            slot: 1234,
            block_hash: [0xab; 32],
            commit_ts: TiKVTimestamp::from_version(42),
            network: "Preview".into(),
        };

        let s = entry.to_string();
        let parts: Vec<&str> = s.split(',').collect();
        assert_eq!(parts.len(), 8);
        assert_eq!(parts[0], "1234");
        assert_eq!(parts[2], "false");
        assert_eq!(parts[3], "42");
        assert_eq!(parts[4], "preview");
        assert_eq!(parts[7], "0");
    }
}
