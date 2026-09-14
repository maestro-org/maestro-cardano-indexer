use std::fmt;

use encoding::decode::DecodedAddress;
use pallas::{
    ledger::{
        addresses::{Address, ByronAddress, ShelleyAddress},
        primitives::babbage::{AssetName, PolicyId},
        traverse::OutputRef,
    },
    network::miniprotocols::Point,
};

pub mod encoding;

type Dataplane = u8;
type Instance = u8;

#[derive(Debug)]
pub enum Key {
    Cursor((Dataplane, Instance)),
    Reducer((Dataplane, Instance, ReducerKey)),
    Rollback((Dataplane, Instance, RollbackKey)),
}

// REDUCER / DATA

#[derive(Debug)]
pub enum ReducerKey {
    Tx(TxByHashKey),
    TxCount(TxCountByAddressKey),
    AssetHolders(HoldersByAssetKey),
    PolicyUtxos(UtxosByPolicyKey),
    AssetUtxos(UtxosByAssetKey),
    AddressBalance(LovelaceByAddressKey),
    AddressUtxos(UtxosByAddressKey),
    Datum(DatumByHashKey),
}

pub struct TxByHashKey {
    pub tx_hash: [u8; 32],
}

impl fmt::Debug for TxByHashKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TxByHash [{}]", hex::encode(self.tx_hash))
    }
}

pub struct TxCountByAddressKey {
    pub address: DecodedAddress,
}

impl fmt::Debug for TxCountByAddressKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TxCountByAddress [{:?}]", self.address)
    }
}

pub struct LovelaceByAddressKey {
    pub address: DecodedAddress,
}

impl fmt::Debug for LovelaceByAddressKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "LovelaceByAddress [{:?}]", self.address)
    }
}

#[derive(Debug)]
pub struct HoldersByAssetKey {
    pub policy: PolicyId,
    pub asset_name: AssetName,
    pub address: Address,
}

pub struct UtxosByPolicyKey {
    pub policy: PolicyId,
    pub slot: u64,
    pub utxo_hash: [u8; 32],
    pub utxo_index: u64,
}

impl fmt::Debug for UtxosByPolicyKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "UtxosByPolicyKey [policy: {}, slot: {}, utxo: {}#{}]",
            self.policy,
            self.slot,
            hex::encode(self.utxo_hash),
            self.utxo_index
        )
    }
}

pub struct UtxosByAssetKey {
    pub policy: PolicyId,
    pub asset_name: AssetName,
    pub slot: u64,
    pub utxo_hash: [u8; 32],
    pub utxo_index: u64,
}

impl fmt::Debug for UtxosByAssetKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "UtxosByAssetKey [policy: {}, name: {}, slot: {}, utxo: {}#{}]",
            self.policy,
            hex::encode(self.asset_name.to_vec()),
            self.slot,
            hex::encode(self.utxo_hash),
            self.utxo_index
        )
    }
}

pub struct UtxosByShelleyAddressKey {
    pub address: ShelleyAddress,
    pub slot: u64,
    pub utxo_hash: [u8; 32],
    pub utxo_index: u64,
}

pub struct UtxosByByronAddressKey {
    pub address: ByronAddress,
    pub slot: u64,
    pub utxo_hash: [u8; 32],
    pub utxo_index: u64,
}

pub struct UtxosByAddressKey {
    pub address: DecodedAddress,
    pub slot: u64,
    pub utxo_hash: [u8; 32],
    pub utxo_index: u64,
}

impl fmt::Debug for UtxosByAddressKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "UtxosByAddress [{:?}] [slot: {}] [utxo: {}#{}]",
            self.address,
            self.slot,
            hex::encode(self.utxo_hash),
            self.utxo_index
        )
    }
}

pub struct DatumByHashKey {
    pub datum_hash: [u8; 32],
}

impl fmt::Debug for DatumByHashKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "DatumByHashKey [{}]", hex::encode(self.datum_hash))
    }
}

pub struct BlockByHeightKey {
    pub block_height: u64,
}

impl fmt::Debug for BlockByHeightKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "BlockByHeightKey [{}]", self.block_height)
    }
}

pub struct HeightByBlockHashKey {
    pub block_hash: [u8; 32],
}

impl fmt::Debug for HeightByBlockHashKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "HeightByBlockHashKey [{}]", hex::encode(self.block_hash))
    }
}

pub struct TxsByAddressKey {
    pub address: DecodedAddress,
    pub slot: u64,
    pub block_index: u16,
    pub tx_hash: [u8; 32],
}

impl fmt::Debug for TxsByAddressKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "TxsByAddressKey [addr: {:?}, slot: {}, tx: {}, block index: {}]",
            self.address,
            self.slot,
            hex::encode(self.tx_hash),
            self.block_index
        )
    }
}

// ROLLBACK

#[derive(Debug)]
pub enum RollbackKey {
    Enrich(EnrichRollbackKey),
    Storage(StorageRollbackKey),
}

impl RollbackKey {
    pub fn point(&self) -> &Point {
        match self {
            RollbackKey::Enrich(e) => &e.point,
            RollbackKey::Storage(e) => &e.point,
        }
    }
}

#[derive(Debug)]
pub struct EnrichRollbackKey {
    pub point: Point,
    pub item_idx: u64,
    pub utxo_ref: OutputRef,
}

#[derive(Debug)]
pub struct StorageRollbackKey {
    pub point: Point,
    pub item_idx: u64,
    pub key: Vec<u8>,
}
