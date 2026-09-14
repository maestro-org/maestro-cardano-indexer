pub mod decode;
pub mod encode;
pub mod namespace;
pub use namespace::{prefix_key_range, Namespace};

mod block_by_tx;
mod cip25_metadata_by_asset;
mod mint_metadata_by_asset;
mod script_by_hash;
mod supply_by_asset;
mod token_registry;
mod txs_by_payment_cred;
mod txs_by_policy;
mod updates_by_policy;
mod utxos_by_payment_cred;

pub use block_by_tx::*;
pub use cip25_metadata_by_asset::*;
pub use mint_metadata_by_asset::*;
pub use script_by_hash::*;
pub use supply_by_asset::*;
pub use token_registry::*;
pub use txs_by_payment_cred::*;
pub use txs_by_policy::*;
pub use updates_by_policy::*;
pub use utxos_by_payment_cred::*;

pub enum TimbreError {
    MalformedCursor,
}

pub const ROLLBACK_ENRICH_UTXO: u8 = b'E';
pub const ROLLBACK_INVERSE_OPERATION: u8 = b'O';

pub const ADDRESS_SHELLEY: u8 = b'S';
pub const ADDRESS_BYRON: u8 = b'B';

pub trait Slot {
    fn slot(&self) -> u64;
}

#[derive(Clone, Debug)]
pub struct UtxosByAddressCursor {
    pub slot: u64,
    pub u_hash: [u8; 32],
    pub u_index: u64,
}

impl Slot for UtxosByAddressCursor {
    fn slot(&self) -> u64 {
        self.slot
    }
}

/// Address index is the index of the address which holds the cursor UTxO in the
/// lexicographically sorted list of all addresses being scanned
#[derive(Clone, Debug)]
pub struct UtxosByAddressesCursor {
    pub address_idx: u16,
    pub slot: u64,
    pub u_hash: [u8; 32],
    pub u_index: u64,
}

#[derive(Clone, Debug)]
pub struct TxsByAddressCursor {
    pub slot: u64,
    pub tx_hash: [u8; 32],
    pub block_index: u16,
}

impl Slot for TxsByAddressCursor {
    fn slot(&self) -> u64 {
        self.slot
    }
}

#[derive(Clone, Debug)]
pub enum RangeBound<T> {
    Slot(u64),
    Cursor(T),
}
