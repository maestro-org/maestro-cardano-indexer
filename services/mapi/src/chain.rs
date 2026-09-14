/*
   from https://github.com/txpipe/scrolls/blob/main/src/crosscut/args.rs
*/

use chrono::{DateTime, TimeZone, Utc};
use pallas::{
    ledger::traverse::wellknown::GenesisValues,
    network::miniprotocols::{MAINNET_MAGIC, PREVIEW_MAGIC, PRE_PRODUCTION_MAGIC},
};

#[derive(Clone)]
pub struct ChainInfo {
    pub magic: u64,
    pub byron_epoch_length: u32,
    pub byron_slot_length: u32,
    pub byron_known_slot: u64,
    pub byron_known_hash: String,
    pub byron_known_time: u64,
    pub shelley_epoch_length: u32,
    pub shelley_slot_length: u32,
    pub shelley_known_slot: u64,
    pub shelley_known_hash: String,
    pub shelley_known_time: u64,
    pub address_network_id: u8,
}

impl ChainInfo {
    pub fn network_id(&self) -> u8 {
        self.address_network_id
    }

    pub fn genesis(&self) -> GenesisValues {
        match self.magic {
            MAINNET_MAGIC => GenesisValues::mainnet(),
            PRE_PRODUCTION_MAGIC => GenesisValues::preprod(),
            PREVIEW_MAGIC => GenesisValues::preview(),
            _ => unreachable!(),
        }
    }

    pub fn slot_to_unix(&self, slot: u64) -> u64 {
        self.genesis().slot_to_wallclock(slot)
    }

    pub fn slot_to_utc(&self, slot: u64) -> String {
        let unix_ts = self.genesis().slot_to_wallclock(slot);
        let datetime: DateTime<Utc> = Utc.timestamp_opt(unix_ts.try_into().unwrap(), 0).unwrap();
        datetime.format("%Y-%m-%d %H:%M:%S").to_string()
    }
}

pub fn mainnet_chain_info() -> ChainInfo {
    ChainInfo {
        magic: MAINNET_MAGIC,
        byron_epoch_length: 432000,
        byron_slot_length: 20,
        byron_known_slot: 0,
        byron_known_time: 1506203091,
        byron_known_hash: "f0f7892b5c333cffc4b3c4344de48af4cc63f55e44936196f365a9ef2244134f"
            .to_string(),
        shelley_epoch_length: 432000,
        shelley_slot_length: 1,
        shelley_known_slot: 4492800,
        shelley_known_hash: "aa83acbf5904c0edfe4d79b3689d3d00fcfc553cf360fd2229b98d464c28e9de"
            .to_string(),
        shelley_known_time: 1596059091,
        address_network_id: 1,
    }
}

pub fn preprod_chain_info() -> ChainInfo {
    ChainInfo {
        magic: PRE_PRODUCTION_MAGIC,
        byron_epoch_length: 432000,
        byron_slot_length: 20,
        byron_known_slot: 0,
        byron_known_hash: "9ad7ff320c9cf74e0f5ee78d22a85ce42bb0a487d0506bf60cfb5a91ea4497d2"
            .to_string(),
        byron_known_time: 1654041600,
        shelley_epoch_length: 432000,
        shelley_slot_length: 1,
        shelley_known_slot: 86400,
        shelley_known_hash: "c971bfb21d2732457f9febf79d9b02b20b9a3bef12c561a78b818bcb8b35a574"
            .to_string(),
        shelley_known_time: 1655769600,
        address_network_id: 0,
    }
}

pub fn preview_chain_info() -> ChainInfo {
    ChainInfo {
        magic: PREVIEW_MAGIC,
        byron_epoch_length: 86400,
        byron_slot_length: 20,
        byron_known_slot: 0,
        byron_known_hash: "".to_string(),
        byron_known_time: 1666656000,
        shelley_epoch_length: 86400,
        shelley_slot_length: 1,
        shelley_known_slot: 0,
        shelley_known_hash: "268ae601af8f9214804735910a3301881fbe0eec9936db7d1fb9fc39e93d1e37"
            .to_string(),
        shelley_known_time: 1666656000,
        address_network_id: 0,
    }
}
