use std::{
    ops::{Deref, Range},
    vec,
};

use cryptoxide::blake2b::Blake2b;
use pallas::{
    codec::utils::{Bytes, KeyValuePairs},
    ledger::{
        addresses::{
            Address, ByronAddress, Network, ShelleyAddress, ShelleyDelegationPart,
            ShelleyPaymentPart,
        },
        primitives::babbage::{AssetName, PolicyId},
    },
    network::miniprotocols::Point,
};

use base64::{engine::general_purpose as b64, Engine};

pub use super::{
    script_by_hash::encode_script_by_hash_key, supply_by_asset::encode_supply_by_asset_key,
    RangeBound, TxsByAddressCursor, UtxosByAddressCursor, UtxosByPaymentCredCursor, ADDRESS_BYRON,
    ADDRESS_SHELLEY, ROLLBACK_ENRICH_UTXO, ROLLBACK_INVERSE_OPERATION, *,
};

#[derive(Clone)]
pub struct KeyEncoder {
    dataplane_id: u8,
    instance_id: u8,
}

pub const TIKV_MAX_KEY_SIZE: usize = 4 * 1024;

pub const PREFIX_CURSOR: u8 = b'C';
pub const PREFIX_DATA: u8 = b'D';
pub const PREFIX_INFO: u8 = b'I';
pub const PREFIX_ROLLBACK: u8 = b'R';
pub const PREFIX_INDEX: u8 = b'X';
pub const PREFIX_TOKEN_REGISTRY: u8 = b'T';

pub const BREAK: u8 = b'`';

pub const REDUCER_TX_BY_HASH: u8 = b'a';
pub const REDUCER_TX_COUNT_BY_ADDRESS: u8 = b'b';
pub const REDUCER_HOLDERS_BY_ASSET: u8 = b'c';
pub const REDUCER_UTXOS_BY_POLICY: u8 = b'd';
pub const REDUCER_UTXOS_BY_ASSET: u8 = b'e';
pub const REDUCER_LOVELACE_BY_ADDRESS: u8 = b'f';
pub const REDUCER_DATUM_BY_HASH: u8 = b'g';
pub const REDUCER_BLOCK_BY_HEIGHT: u8 = b'h';
pub const REDUCER_TXS_BY_ADDRESS: u8 = b'i';
pub const REDUCER_TXS_BY_PAY_CRED: u8 = b'j';
// Tag bytes b'k', b'l' and b'm' are permanently reserved: they were used by
// retired partner-specific reducers, and reusing them would make historical
// data ambiguous.
pub const REDUCER_SUPPLY_BY_ASSET: u8 = b'n';
pub const REDUCER_SCRIPT_BY_HASH: u8 = b'o';
pub const REDUCER_TXS_BY_POLICY: u8 = b'p';
pub const REDUCER_UPDATES_BY_POLICY: u8 = b'q';
pub const REDUCER_MINT_METADATA_BY_ASSET: u8 = b'r';
pub const REDUCER_CIP25_METADATA_BY_ASSET: u8 = b's';
pub const REDUCER_BLOCK_BY_TX: u8 = b't';

pub const REDUCER_UTXOS_BY_SHELLEY_ADDRESS: u8 = b'y';
pub const REDUCER_UTXOS_BY_BYRON_ADDRESS: u8 = b'z';

pub const MAX_BYRON_ADDRESS_LEN: usize = 3000;

impl KeyEncoder {
    /// Create a new key encoder for the given instance ID. The instance ID will
    /// be the prefix for all keys encoded by the encoder.
    pub fn new(dataplane: u8, instance: u8) -> Self {
        KeyEncoder {
            dataplane_id: dataplane,
            instance_id: instance,
        }
    }

    pub fn dataplane_id(&self) -> u8 {
        self.dataplane_id
    }

    pub fn instance_id(&self) -> u8 {
        self.instance_id
    }

    // <DATAPLANE><INSTANCE><CURSOR TAG>
    pub fn encode_cursor_key(&self) -> Vec<u8> {
        vec![self.dataplane_id, self.instance_id, PREFIX_CURSOR]
    }

    // <INFO_TAG><BREAK><DATAPLANE><INSTANCE>
    pub fn encode_info_key(&self) -> Vec<u8> {
        vec![PREFIX_INFO, BREAK, self.dataplane_id, self.instance_id]
    }

    // <DATAPLANE><INSTANCE><DATA><TX_BY_HASH_TAG><txhash>
    pub fn encode_tx_by_hash_key(&self, txhash: &[u8; 32]) -> Vec<u8> {
        let mut key = Vec::with_capacity(4 + 32);

        key.push(self.dataplane_id);
        key.push(self.instance_id);
        key.push(PREFIX_DATA);

        key.push(REDUCER_TX_BY_HASH);

        key.extend_from_slice(txhash);

        key
    }

    // <DATAPLANE><INSTANCE><DATA><TX_COUNT_BY_ADDRESS_TAG><address with type>
    pub fn encode_tx_count_by_address_key(&self, address: Address) -> Vec<u8> {
        let mut key = vec![];

        key.push(self.dataplane_id);
        key.push(self.instance_id);
        key.push(PREFIX_DATA);

        key.push(REDUCER_TX_COUNT_BY_ADDRESS);

        key.extend(encode_address_with_type(&address));

        key
    }

    // <DATAPLANE><INSTANCE><DATA><LOVELACE_BY_ADDRESS_TAG><address with type>
    pub fn encode_lovelace_by_address_key(&self, address: Address) -> Vec<u8> {
        let mut key = vec![];

        key.push(self.dataplane_id);
        key.push(self.instance_id);
        key.push(PREFIX_DATA);

        key.push(REDUCER_LOVELACE_BY_ADDRESS);

        key.extend(encode_address_with_type(&address));

        key
    }

    // <DATAPLANE><INSTANCE><DATA><HOLDERS_BY_ASSET_TAG><policy><BREAK><asset_name><BREAK><address with type>
    pub fn encode_holders_by_asset_key(
        &self,
        policy: PolicyId,
        asset_name: AssetName,
        address: Address,
    ) -> Vec<u8> {
        let mut key = vec![];

        key.push(self.dataplane_id);
        key.push(self.instance_id);
        key.push(PREFIX_DATA);

        key.push(REDUCER_HOLDERS_BY_ASSET);

        key.extend_from_slice(policy.deref());
        key.push(BREAK);
        key.extend(encode_short_bytestring(&asset_name));

        key.push(BREAK);

        key.extend(encode_address_with_type(&address));

        key
    }

    // <DATAPLANE><INSTANCE><DATA><UTXO_BY_POLICY><policy_id><BREAK><u64(slot)><utxo_hash><u64(utxo_idx)>
    pub fn encode_utxos_by_policy_key(
        &self,
        policy: PolicyId,
        slot: u64,
        u_hash: [u8; 32],
        u_index: u64,
    ) -> Vec<u8> {
        let mut key = vec![];

        key.push(self.dataplane_id);
        key.push(self.instance_id);
        key.push(PREFIX_DATA);

        key.push(REDUCER_UTXOS_BY_POLICY);

        key.extend_from_slice(policy.deref());

        key.push(BREAK);

        key.extend_from_slice(&slot.to_be_bytes());

        key.extend_from_slice(&u_hash);
        key.extend_from_slice(&u_index.to_be_bytes());

        key
    }

    // <DATAPLANE><INSTANCE><DATA><UTXO_BY_ASSET><policy_id><PLACEHOLDER><asset_name><PLACEHOLDER><u64(slot)><utxo_hash><u64(utxo_idx)>
    pub fn encode_utxos_by_asset_key(
        &self,
        policy: PolicyId,
        asset_name: AssetName,
        slot: u64,
        u_hash: [u8; 32],
        u_index: u64,
    ) -> Vec<u8> {
        let mut key = vec![];

        key.push(self.dataplane_id);
        key.push(self.instance_id);
        key.push(PREFIX_DATA);

        key.push(REDUCER_UTXOS_BY_ASSET);

        key.extend_from_slice(policy.deref());
        key.push(BREAK);
        key.extend(encode_short_bytestring(&asset_name));

        key.push(BREAK);

        key.extend_from_slice(&slot.to_be_bytes());

        key.extend_from_slice(&u_hash);
        key.extend_from_slice(&u_index.to_be_bytes());

        key
    }

    // <DATAPLANE><INSTANCE><DATA><UTXOS_BY_ADDRESS_TAG><payment_cred><PLACEHOLDER><staking_cred><PLACEHOLDER><u64(slot)><utxo_hash><u64(utxo_idx)>
    // payment cred = 0 || <key> for key or 1 || <scripthash>
    // staking cred = 0 || <key> OR 1 || <scripthash> OR 2 || <pointer> OR 3 (null)
    pub fn encode_utxos_by_shelley_address_key(
        &self,
        address: ShelleyAddress,
        slot: u64,
        u_hash: [u8; 32],
        u_index: u64,
    ) -> Vec<u8> {
        let mut key = match address.delegation() {
            ShelleyDelegationPart::Key(_) | ShelleyDelegationPart::Script(_) => {
                Vec::with_capacity(76 + 28)
            }
            ShelleyDelegationPart::Pointer(_) => Vec::with_capacity(76 + 12),
            ShelleyDelegationPart::Null => Vec::with_capacity(76),
        };

        // <DATAPLANE>
        key.push(self.dataplane_id);

        // <INSTANCE>
        key.push(self.instance_id);

        // <DATA>
        key.push(PREFIX_DATA);

        // <UTXOS_BY_ADDRESS_TAG>
        key.push(REDUCER_UTXOS_BY_SHELLEY_ADDRESS);

        // <payment_cred><BREAK><staking_cred>
        key.extend(encode_shelley_address(&address));

        // <BREAK>
        key.push(BREAK);

        // <u64(slot)>
        key.extend_from_slice(&slot.to_be_bytes());

        // <utxo_hash>
        key.extend_from_slice(&u_hash);

        // <u64(u_index)>
        key.extend_from_slice(&u_index.to_be_bytes());

        key
    }

    // <...><UTXOS_BY_BYRON_ADDR_TAG><byron_address_or_hash><PLACEHOLDER><u64(slot)><utxo_hash><u64(utxo_index)>
    // <byron_address_or_hash> =
    //      if first byte 0, then second and third byte is address byte string length X and X next bytes are the address
    //      if first byte 1, next 32 bytes are blake2b hash of the address bytes (when address bytes were longer than MAX_BYRON_ADDRESS_LEN bytes)
    // TODO: merge with shelley?
    pub fn encode_utxos_by_byron_address_key(
        &self,
        address: ByronAddress,
        slot: u64,
        u_hash: [u8; 32],
        u_index: u64,
    ) -> Vec<u8> {
        let mut key = vec![];

        key.push(self.dataplane_id);
        key.push(self.instance_id);
        key.push(PREFIX_DATA);

        key.push(REDUCER_UTXOS_BY_BYRON_ADDRESS);

        key.extend(encode_byron_address(&address));

        key.push(BREAK);

        key.extend_from_slice(&slot.to_be_bytes());

        key.extend_from_slice(&u_hash);
        key.extend_from_slice(&u_index.to_be_bytes());

        key
    }

    // <...><DATUM_BY_HASH_TAG><datum hash>
    pub fn encode_datum_by_hash_key(&self, hash: &[u8; 32]) -> Vec<u8> {
        let mut key = Vec::with_capacity(4 + 32);

        key.push(self.dataplane_id);
        key.push(self.instance_id);
        key.push(PREFIX_DATA);

        key.push(REDUCER_DATUM_BY_HASH);

        key.extend_from_slice(hash);

        key
    }

    // <DATAPLANE><INSTANCE><PREFIX_DATA><BLOCK_BY_HEIGHT_TAG><0x30 = '0'><u64(height)>
    pub fn encode_block_by_height_key(&self, height: u64) -> Vec<u8> {
        let expected_len = 4 + 1 + 8;
        let mut key = Vec::with_capacity(expected_len);

        key.push(self.dataplane_id);
        key.push(self.instance_id);
        key.push(PREFIX_DATA);

        key.push(REDUCER_BLOCK_BY_HEIGHT);

        key.push(b'0'); // signal this is a block by height key from the reducer

        key.extend_from_slice(&height.to_be_bytes());

        assert_eq!(key.len(), expected_len);

        key
    }

    // <DATAPLANE><INSTANCE><PREFIX_DATA><BLOCK_BY_HEIGHT><0x31 = '1'><block hash>
    pub fn encode_height_by_block_hash_key(&self, hash: &[u8; 32]) -> Vec<u8> {
        let expected_len = 4 + 1 + 32;
        let mut key = Vec::with_capacity(expected_len);

        key.push(self.dataplane_id);
        key.push(self.instance_id);
        key.push(PREFIX_DATA);

        key.push(REDUCER_BLOCK_BY_HEIGHT);

        key.push(b'1'); // signal this is a height by hash key from the reducer

        key.extend_from_slice(hash);

        assert_eq!(key.len(), expected_len);

        key
    }

    // <...><TXS_BY_ADDRESS_TAG><address with tag><BREAK><u64(slot)><u16(blk_index)><tx_hash>
    pub fn encode_txs_by_address_key(
        &self,
        address: &Address,
        slot: u64,
        blk_index: u16,
        tx_hash: &[u8; 32],
    ) -> Vec<u8> {
        let mut key = Vec::new();

        key.push(self.dataplane_id);
        key.push(self.instance_id);
        key.push(PREFIX_DATA);

        key.push(REDUCER_TXS_BY_ADDRESS);

        key.extend(encode_address_with_type(address));
        key.push(BREAK);
        key.extend(u64::to_be_bytes(slot));
        key.extend(u16::to_be_bytes(blk_index));
        key.extend(tx_hash);

        key
    }

    // <...><TXS_BY_PAY_CRED_TAG><shelley payment cred><BREAK><u64(slot)><u16(blk_index)><tx_hash>
    pub fn encode_txs_by_payment_cred_key(
        &self,
        payment_cred: &ShelleyPaymentPart,
        slot: u64,
        blk_index: u16,
        tx_hash: &[u8; 32],
    ) -> Vec<u8> {
        encode_txs_by_payment_cred_key(self, payment_cred, slot, blk_index, tx_hash)
    }

    pub fn encode_supply_by_asset_key(&self, policy: &PolicyId, name: Vec<u8>) -> Vec<u8> {
        encode_supply_by_asset_key(self, policy, name)
    }

    pub fn encode_supply_by_asset_range(
        &self,
        policy: &PolicyId,
        lower: Option<RangeBound<Vec<u8>>>,
        upper: Option<RangeBound<Vec<u8>>>,
    ) -> Range<Vec<u8>> {
        encode_supply_by_asset_range(self, policy, lower, upper)
    }

    pub fn encode_script_by_hash_key(&self, hash: &[u8; 28]) -> Vec<u8> {
        encode_script_by_hash_key(self, hash)
    }

    pub fn encode_txs_by_policy_key(
        &self,
        policy: &PolicyId,
        slot: u64,
        blk_index: u16,
    ) -> Vec<u8> {
        encode_txs_by_policy_key(self, policy, slot, blk_index)
    }

    pub fn encode_txs_by_policy_range(
        &self,
        policy: &PolicyId,
        lower: Option<RangeBound<TxsByPolicyCursor>>,
        upper: Option<RangeBound<TxsByPolicyCursor>>,
    ) -> Range<Vec<u8>> {
        encode_txs_by_policy_range(self, policy, lower, upper)
    }

    pub fn encode_updates_by_policy_key(
        &self,
        policy: &PolicyId,
        slot: u64,
        blk_index: u16,
    ) -> Vec<u8> {
        encode_updates_by_policy_key(self, policy, slot, blk_index)
    }

    pub fn encode_updates_by_policy_range(
        &self,
        policy: &PolicyId,
        lower: Option<RangeBound<UpdatesByPolicyCursor>>,
        upper: Option<RangeBound<UpdatesByPolicyCursor>>,
    ) -> Range<Vec<u8>> {
        encode_updates_by_policy_range(self, policy, lower, upper)
    }

    pub fn encode_mint_metadata_by_asset_key(
        &self,
        policy: &PolicyId,
        name: Vec<u8>,
        slot: u64,
        blk_index: u16,
    ) -> Vec<u8> {
        encode_mint_metadata_by_asset_key(self, policy, name, slot, blk_index)
    }

    pub fn encode_mint_metadata_by_asset_range(
        &self,
        policy: &PolicyId,
        name: Vec<u8>,
        lower: Option<RangeBound<MintMetadataByAssetCursor>>,
        upper: Option<RangeBound<MintMetadataByAssetCursor>>,
    ) -> Range<Vec<u8>> {
        encode_mint_metadata_by_asset_range(self, policy, name, lower, upper)
    }

    pub fn encode_cip25_metadata_by_asset_key(&self, policy: &PolicyId, name: Vec<u8>) -> Vec<u8> {
        encode_cip25_metadata_by_asset_key(self, policy, name)
    }

    pub fn encode_token_registry_by_asset_key(&self, policy: &PolicyId, name: Vec<u8>) -> Vec<u8> {
        encode_token_registry_by_asset_key(self, policy, name)
    }

    pub fn encode_block_by_tx_key(&self, tx_hash: [u8; 32]) -> Vec<u8> {
        encode_block_by_tx_key(self, &tx_hash)
    }

    // ---

    // <DATAPLANE><INSTANCE><ROLLBACK><PLACEHOLDER><u64(block_slot)><block_hash><INVERSEOP_TAG><u64(operation_idx)><key>
    pub fn encode_storage_rollback_metadata_key(
        &self,
        b_slot: u64,
        b_hash: &[u8; 32],
        operation_idx: u64,
        modified_key: &[u8],
    ) -> Vec<u8> {
        // 1 + 1 + 1 + 1 + 8 + 32 + 1 + 8 + key len
        let mut key = Vec::with_capacity(53 + modified_key.len());

        key.push(self.dataplane_id);
        key.push(self.instance_id);
        key.push(PREFIX_ROLLBACK);

        key.push(BREAK);

        key.extend_from_slice(&b_slot.to_be_bytes());
        key.extend_from_slice(b_hash);

        key.push(ROLLBACK_INVERSE_OPERATION);

        key.extend_from_slice(&operation_idx.to_be_bytes());
        key.extend_from_slice(modified_key);

        assert_eq!(key.len(), 53 + modified_key.len());
        key
    }

    /// Whether the UTXO was consumed or produced can be determined from the value:
    /// if it is nil then the UTXO was produced, if it is bytes then the UTXO was
    /// consumed and the bytes are the UTXO bytes.
    ///
    /// TODO: Don't need item idx?
    ///
    /// `<DATAPLANE><INSTANCE><ROLLBACK><PLACEHOLDER><u64(block_slot)><block_hash><ENRICH_TAG><u64(item_idx)><utxohash><u64(utxoidx)>`
    pub fn encode_enrich_rollback_metadata_key(
        &self,
        b_slot: u64,
        b_hash: &[u8; 32],
        item_idx: u64,
        u_hash: [u8; 32],
        u_index: u64,
    ) -> Vec<u8> {
        let mut key = Vec::with_capacity(93);

        key.push(self.dataplane_id);
        key.push(self.instance_id);
        key.push(PREFIX_ROLLBACK);

        key.push(BREAK);

        key.extend_from_slice(&b_slot.to_be_bytes());
        key.extend_from_slice(b_hash);

        key.push(ROLLBACK_ENRICH_UTXO);

        key.extend_from_slice(&item_idx.to_be_bytes());

        key.extend_from_slice(&u_hash);
        key.extend_from_slice(&u_index.to_be_bytes());

        assert_eq!(key.len(), 93);
        key
    }

    /// Encode keys and create a range which will contain all (enrich and storage)
    /// rollback metadata keys between slot `from` (including) and slot `to` (not
    /// including). If `from` is None it is negative infinity and if `to` is None
    /// it is positive infinity.
    pub fn encode_rollback_metadata_range(
        &self,
        lower: Option<u64>,
        upper: Option<u64>,
    ) -> Range<Vec<u8>> {
        let mut common_prefix = vec![self.dataplane_id, self.instance_id, PREFIX_ROLLBACK];

        let start_key = match lower {
            // <DATAPLANE><INSTANCE><ROLLBACK><PLACEHOLDER>
            None => {
                let mut buf = common_prefix.clone();

                buf.push(BREAK);

                buf
            }
            // <DATAPLANE><INSTANCE><ROLLBACK><PLACEHOLDER><u64(start_slot)>
            Some(start_slot) => {
                let mut buf = common_prefix.clone();

                buf.push(BREAK);
                buf.extend_from_slice(&u64::to_be_bytes(start_slot));

                buf
            }
        };

        let end_key = match upper {
            // <DATAPLANE><INSTANCE><ROLLBACK><PLACEHOLDER><u64(end_slot)>
            Some(end_slot) => {
                common_prefix.push(BREAK);
                common_prefix.extend_from_slice(&u64::to_be_bytes(end_slot));

                common_prefix
            }
            // <DATAPLANE><INSTANCE><ROLLBACK><PLACEHOLDER + 1>
            None => {
                common_prefix.push(BREAK + 1);

                common_prefix
            }
        };

        start_key..end_key
    }

    /// The from is always the smaller key, regardless if we are in descending order.
    /// If the order is descending and the user provides the higher key in the `from`,
    /// that is used as the `to` here.
    pub fn encode_address_utxos_range(
        &self,
        address: &Address,
        lower: Option<RangeBound<UtxosByAddressCursor>>,
        upper: Option<RangeBound<UtxosByAddressCursor>>,
    ) -> Range<Vec<u8>> {
        match address {
            Address::Shelley(a) => self.encode_shelley_address_utxos_range(a, lower, upper),
            Address::Byron(a) => self.encode_byron_address_utxos_range(a, lower, upper), // TODO
            _ => unimplemented!("no utxos for stake address"),
        }
    }

    /// Given a Byron address, returns a Key range which will include the
    /// UTxO by address keys for the address. Can optionally specify a start
    /// slot (inclusive) and a end slot (exclusive) to only return UTxOs created
    /// in that slot range.
    pub fn encode_byron_address_utxos_range(
        &self,
        address: &ByronAddress,
        lower: Option<RangeBound<UtxosByAddressCursor>>,
        upper: Option<RangeBound<UtxosByAddressCursor>>,
    ) -> Range<Vec<u8>> {
        let mut prefix = vec![
            self.dataplane_id,
            self.instance_id,
            PREFIX_DATA,
            REDUCER_UTXOS_BY_BYRON_ADDRESS,
        ];

        prefix.extend(encode_byron_address(address));

        let start_key = match lower {
            None => {
                let mut buf = prefix.clone();
                buf.push(BREAK);
                buf
            }
            Some(RangeBound::Slot(start_slot)) => {
                let mut buf = prefix.clone();
                buf.push(BREAK);
                buf.extend_from_slice(&u64::to_be_bytes(start_slot));
                buf
            }
            Some(RangeBound::Cursor(cursor)) => {
                let mut buf = prefix.clone();
                buf.push(BREAK);
                buf.extend_from_slice(&u64::to_be_bytes(cursor.slot));
                buf.extend_from_slice(&cursor.u_hash);
                buf.extend_from_slice(&u64::to_be_bytes(cursor.u_index));
                // exclusive of the cursor item itself (it was served on the
                // previous page), matching the Shelley range below
                buf.push(0);
                buf
            }
        };

        let end_key = match upper {
            Some(RangeBound::Cursor(cursor)) => {
                prefix.push(BREAK);
                prefix.extend_from_slice(&u64::to_be_bytes(cursor.slot));
                prefix.extend_from_slice(&cursor.u_hash);
                prefix.extend_from_slice(&u64::to_be_bytes(cursor.u_index));
                prefix
            }
            Some(RangeBound::Slot(end_slot)) => {
                prefix.push(BREAK);
                // results will include those at slot `end_slot`
                prefix.extend_from_slice(&u64::to_be_bytes(end_slot + 1));
                prefix
            }
            None => {
                prefix.push(BREAK + 1);
                prefix
            }
        };

        start_key..end_key
    }

    /// Given an Shelley address, returns a Key range which will include the
    /// UTxO by address keys for the address. Can optionally specify a start
    /// slot (inclusive) and a end slot (inclusive) to only return UTxOs created
    /// in that slot range.
    pub fn encode_shelley_address_utxos_range(
        &self,
        address: &ShelleyAddress,
        lower: Option<RangeBound<UtxosByAddressCursor>>,
        upper: Option<RangeBound<UtxosByAddressCursor>>,
    ) -> Range<Vec<u8>> {
        let mut prefix = vec![
            self.dataplane_id,
            self.instance_id,
            PREFIX_DATA,
            REDUCER_UTXOS_BY_SHELLEY_ADDRESS,
        ];

        prefix.extend(encode_shelley_address(address));

        let start_key = match lower {
            None => {
                let mut buf = prefix.clone();
                buf.push(BREAK);
                buf
            }
            Some(RangeBound::Slot(start_slot)) => {
                let mut buf = prefix.clone();
                buf.push(BREAK);
                buf.extend_from_slice(&u64::to_be_bytes(start_slot));
                buf
            }
            Some(RangeBound::Cursor(cursor)) => {
                let mut buf = prefix.clone();
                buf.push(BREAK);
                buf.extend_from_slice(&u64::to_be_bytes(cursor.slot));
                buf.extend_from_slice(&cursor.u_hash);
                buf.extend_from_slice(&u64::to_be_bytes(cursor.u_index));
                buf.push(0);
                buf
            }
        };

        let end_key = match upper {
            Some(RangeBound::Cursor(cursor)) => {
                prefix.push(BREAK);
                prefix.extend_from_slice(&u64::to_be_bytes(cursor.slot));
                prefix.extend_from_slice(&cursor.u_hash);
                prefix.extend_from_slice(&u64::to_be_bytes(cursor.u_index));

                prefix
            }
            Some(RangeBound::Slot(end_slot)) => {
                prefix.push(BREAK);
                // Inclusive of end_slot: the upper bound is exclusive at the
                // key level, so we advance to end_slot + 1 to include every key
                // at end_slot.
                prefix.extend_from_slice(&u64::to_be_bytes(end_slot + 1));
                prefix
            }
            None => {
                prefix.push(BREAK + 1);
                prefix
            }
        };

        start_key..end_key
    }

    /// Given an Shelley address, returns a Key range which will include the
    /// UTxO by address keys for the address. Can optionally specify a start
    /// slot (inclusive) and a end slot (inclusive) to only return UTxOs created
    /// in that slot range.
    pub fn encode_utxos_by_payment_cred_range(
        &self,
        network: u8,
        payment: &ShelleyPaymentPart,
        lower: Option<UtxosByPaymentCredCursor>,
        upper: Option<UtxosByPaymentCredCursor>,
    ) -> Range<Vec<u8>> {
        encode_utxos_by_payment_cred_range(self, network, payment, lower, upper)
    }

    pub fn encode_asset_utxos_range(
        &self,
        policy: &PolicyId,
        asset_name: &AssetName,
        lower: Option<RangeBound<UtxosByAddressCursor>>,
        upper: Option<RangeBound<UtxosByAddressCursor>>,
    ) -> Range<Vec<u8>> {
        let mut prefix = vec![
            self.dataplane_id,
            self.instance_id,
            PREFIX_DATA,
            REDUCER_UTXOS_BY_ASSET,
        ];

        prefix.extend_from_slice(policy.deref());
        prefix.push(BREAK);
        prefix.extend(encode_short_bytestring(asset_name));

        let start_key = match lower {
            None => {
                let mut buf = prefix.clone();
                buf.push(BREAK);
                buf
            }
            Some(RangeBound::Slot(start_slot)) => {
                let mut buf = prefix.clone();
                buf.push(BREAK);
                buf.extend_from_slice(&u64::to_be_bytes(start_slot));
                buf
            }
            Some(RangeBound::Cursor(cursor)) => {
                let mut buf = prefix.clone();
                buf.push(BREAK);
                buf.extend_from_slice(&u64::to_be_bytes(cursor.slot));
                buf.extend_from_slice(&cursor.u_hash);
                buf.extend_from_slice(&u64::to_be_bytes(cursor.u_index));
                buf.push(0);
                buf
            }
        };

        let end_key = match upper {
            Some(RangeBound::Cursor(cursor)) => {
                prefix.push(BREAK);
                prefix.extend_from_slice(&u64::to_be_bytes(cursor.slot));
                prefix.extend_from_slice(&cursor.u_hash);
                prefix.extend_from_slice(&u64::to_be_bytes(cursor.u_index));

                prefix
            }
            Some(RangeBound::Slot(end_slot)) => {
                prefix.push(BREAK);
                // Inclusive of end_slot: the upper bound is exclusive at the
                // key level, so we advance to end_slot + 1 to include every key
                // at end_slot.
                prefix.extend_from_slice(&u64::to_be_bytes(end_slot + 1));
                prefix
            }
            None => {
                prefix.push(BREAK + 1);
                prefix
            }
        };

        start_key..end_key
    }

    pub fn encode_policy_utxos_range(
        &self,
        policy: &PolicyId,
        lower: Option<RangeBound<UtxosByAddressCursor>>,
        upper: Option<RangeBound<UtxosByAddressCursor>>,
    ) -> Range<Vec<u8>> {
        let mut prefix = vec![
            self.dataplane_id,
            self.instance_id,
            PREFIX_DATA,
            REDUCER_UTXOS_BY_POLICY,
        ];

        prefix.extend_from_slice(policy.deref());

        let start_key = match lower {
            None => {
                let mut buf = prefix.clone();
                buf.push(BREAK);
                buf
            }
            Some(RangeBound::Slot(start_slot)) => {
                let mut buf = prefix.clone();
                buf.push(BREAK);
                buf.extend_from_slice(&u64::to_be_bytes(start_slot));
                buf
            }
            Some(RangeBound::Cursor(cursor)) => {
                let mut buf = prefix.clone();
                buf.push(BREAK);
                buf.extend_from_slice(&u64::to_be_bytes(cursor.slot));
                buf.extend_from_slice(&cursor.u_hash);
                buf.extend_from_slice(&u64::to_be_bytes(cursor.u_index));
                buf.push(0);
                buf
            }
        };

        let end_key = match upper {
            Some(RangeBound::Cursor(cursor)) => {
                prefix.push(BREAK);
                prefix.extend_from_slice(&u64::to_be_bytes(cursor.slot));
                prefix.extend_from_slice(&cursor.u_hash);
                prefix.extend_from_slice(&u64::to_be_bytes(cursor.u_index));

                prefix
            }
            Some(RangeBound::Slot(end_slot)) => {
                prefix.push(BREAK);
                prefix.extend_from_slice(&u64::to_be_bytes(end_slot + 1));
                prefix
            }
            None => {
                prefix.push(BREAK + 1);
                prefix
            }
        };

        start_key..end_key
    }

    pub fn encode_txs_by_payment_cred_range(
        &self,
        cred: &ShelleyPaymentPart,
        lower: Option<RangeBound<TxsByPayCredCursor>>,
        upper: Option<RangeBound<TxsByPayCredCursor>>,
    ) -> Range<Vec<u8>> {
        encode_txs_by_payment_cred_range(self, cred, lower, upper)
    }

    /// Given an address, returns a Key range which will include the
    /// Tx by address keys for the address. Can optionally specify a start
    /// slot (inclusive) and a end slot (inclusive) to only return txs
    /// in that slot range.
    pub fn encode_address_txs_range(
        &self,
        address: &Address,
        lower: Option<RangeBound<TxsByAddressCursor>>,
        upper: Option<RangeBound<TxsByAddressCursor>>,
    ) -> Range<Vec<u8>> {
        let mut prefix = vec![
            self.dataplane_id,
            self.instance_id,
            PREFIX_DATA,
            REDUCER_TXS_BY_ADDRESS,
        ];

        prefix.extend(encode_address_with_type(address));

        let start_key = match lower {
            None => {
                let mut buf = prefix.clone();
                buf.push(BREAK);
                buf
            }
            Some(RangeBound::Slot(start_slot)) => {
                let mut buf = prefix.clone();
                buf.push(BREAK);
                buf.extend_from_slice(&u64::to_be_bytes(start_slot));
                buf
            }
            Some(RangeBound::Cursor(cursor)) => {
                let mut buf = prefix.clone();
                buf.push(BREAK);
                buf.extend_from_slice(&u64::to_be_bytes(cursor.slot));
                buf.extend_from_slice(&u16::to_be_bytes(cursor.block_index));
                buf.extend_from_slice(&cursor.tx_hash);
                buf.push(0);
                buf
            }
        };

        let end_key = match upper {
            Some(RangeBound::Cursor(cursor)) => {
                prefix.push(BREAK);
                prefix.extend_from_slice(&u64::to_be_bytes(cursor.slot));
                prefix.extend_from_slice(&u16::to_be_bytes(cursor.block_index));
                prefix.extend_from_slice(&cursor.tx_hash);

                prefix
            }
            Some(RangeBound::Slot(end_slot)) => {
                prefix.push(BREAK);
                // Inclusive of end_slot: the upper bound is exclusive at the
                // key level, so we advance to end_slot + 1 to include every key
                // at end_slot.
                prefix.extend_from_slice(&u64::to_be_bytes(end_slot + 1));
                prefix
            }
            None => {
                prefix.push(BREAK + 1);
                prefix
            }
        };

        start_key..end_key
    }
}

// TODO: Make helper function add to a given vector instead of allocating new ones

/// Helper function for encoding a Shelley address in the format:
///     `<network id><payment cred><BREAK><staking cred>`
///     where
///     `<network id>` is 0 for testnet, 1 for mainnet
///     `<payment cred>` is `[x00 || <key_hash>] or [x01 || <script_hash>]`
///     `<staking cred>` is `[x00 || <key_hash>]` or `[x01 || <script_hash>]` or `[x02 || <pointer>]` or `[x03]` (enterprise)
///     and
///     `<pointer>` is `[u64_be(slot) || u64_be(tx_idx) || u64_be(cert_idx)]`
pub fn encode_shelley_address(address: &ShelleyAddress) -> Vec<u8> {
    let mut out = match address.delegation() {
        ShelleyDelegationPart::Key(_) | ShelleyDelegationPart::Script(_) => {
            Vec::with_capacity(1 + 29 + 1 + 29)
        }
        ShelleyDelegationPart::Pointer(_) => Vec::with_capacity(1 + 29 + 1 + 25),
        ShelleyDelegationPart::Null => Vec::with_capacity(1 + 29 + 1 + 1),
    };

    match address.network() {
        Network::Testnet => out.push(0),
        Network::Mainnet => out.push(1),
        Network::Other(x) => out.push(x),
    }

    match address.payment() {
        ShelleyPaymentPart::Key(h) => {
            out.push(0);
            out.extend_from_slice(h.deref());
        }
        ShelleyPaymentPart::Script(h) => {
            out.push(1);
            out.extend_from_slice(h.deref())
        }
    }

    out.push(BREAK);

    match address.delegation() {
        ShelleyDelegationPart::Key(h) => {
            out.push(0);
            out.extend_from_slice(h.deref());
        }
        ShelleyDelegationPart::Script(h) => {
            out.push(1);
            out.extend_from_slice(h.deref());
        }
        // TODO: Fix Pallas pointer (truncate)
        ShelleyDelegationPart::Pointer(p) => {
            out.push(2);
            out.extend_from_slice(&p.slot().to_be_bytes());
            out.extend_from_slice(&p.tx_idx().to_be_bytes());
            out.extend_from_slice(&p.cert_idx().to_be_bytes());
        }
        ShelleyDelegationPart::Null => out.push(3),
    };

    out
}

/// Helper function for encoding a Shelley address in the format:
///     `<payment cred>` is `[x00 || <key_hash>] or [x01 || <script_hash>]`
pub fn encode_shelley_payment_cred(payment: &ShelleyPaymentPart) -> Vec<u8> {
    let mut out = Vec::with_capacity(1 + 28);

    match payment {
        ShelleyPaymentPart::Key(h) => {
            out.push(0);
            out.extend_from_slice(h.deref());
        }
        ShelleyPaymentPart::Script(h) => {
            out.push(1);
            out.extend_from_slice(h.deref())
        }
    }

    out
}

/// Helper function for encoding a Byron address in the format:
///     `[x00 || u16_be(address_bytes.len) || address_bytes]` if address_bytes.len <= `MAX_BYRON_ADDRESS_LEN`
///     `[x01 || blake2b(address_bytes)] otherwise
fn encode_byron_address(address: &ByronAddress) -> Vec<u8> {
    let mut out;

    let address_bytes = address.to_vec();

    // If address is very long then we use a hash of it in its place
    if address_bytes.len() <= MAX_BYRON_ADDRESS_LEN {
        out = vec![];

        out.push(0); // indicate that this is not a hash, it's the address
        out.extend_from_slice(&(address_bytes.len() as u16).to_be_bytes());
        out.extend(&address_bytes);
    } else {
        out = Vec::with_capacity(33);

        let mut buf = [0; 32];
        Blake2b::blake2b(&mut buf, &address_bytes, &[]);

        out.push(1); // indicate that this is a hash
        out.extend_from_slice(&buf);
    }

    out
}

/// Helper function for encoding a Byron or Shelley address in the format:
///     `[b'S' || encode_shelley_address(a)]` if address `a` is Shelley-era
///     `[b'B' || encode_byron_address(a)]` if address `a` is Byron-era
fn encode_address_with_type(address: &Address) -> Vec<u8> {
    let mut out = vec![];

    match address {
        Address::Shelley(a) => {
            out.push(ADDRESS_SHELLEY); // indicate shelley address

            out.extend(encode_shelley_address(a));
        }
        Address::Byron(a) => {
            out.push(ADDRESS_BYRON); // indicate byron address

            out.extend(encode_byron_address(a))
        }
        Address::Stake(_) => {
            unreachable!()
        }
    }

    out
}

/// Helper function for encoding bytestrings whose length can be represented by
/// a u8 (that is; maximum length of 255). The first byte is the length of the
/// bytestring which immediately follows it. Panics if the length is greater
/// than 255.
pub fn encode_short_bytestring(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(1 + bytes.len());

    out.push(bytes.len().try_into().unwrap());
    out.extend_from_slice(bytes);

    out
}

/// 'O' for Origin or 'S' || <u64(b_slot)><b_hash><u64(b_height)> for Specific
pub fn encode_cursor_value(point: Point, height: u64) -> Vec<u8> {
    match point {
        Point::Origin => vec![b'O'],
        Point::Specific(s, h) => {
            let mut value = Vec::with_capacity(1 + 8 + 32 + 8);

            value.push(b'S');
            value.extend_from_slice(&u64::to_be_bytes(s));
            value.extend_from_slice(&h);
            value.extend_from_slice(&u64::to_be_bytes(height));

            value
        }
    }
}

pub fn encode_info_value(point: Point, height: u64, timestamp: u64) -> Vec<u8> {
    match point {
        Point::Origin => vec![],
        Point::Specific(_slot, hash) => {
            let mut value = Vec::with_capacity(8 + 32 + 1 + 8);
            value.extend_from_slice(&u64::to_be_bytes(height));
            value.extend_from_slice(&hash);
            value.push(false as u8); // mempool = false
            value.extend_from_slice(&u64::to_be_bytes(timestamp));

            value
        }
    }
}

/// <address_with_type><u64(amount)>
pub fn encode_utxos_by_asset_value(address: &Address, amount: u64) -> Vec<u8> {
    let mut buf = Vec::new();

    buf.extend(encode_address_with_type(address));
    buf.extend_from_slice(&u64::to_be_bytes(amount));

    buf
}

/// <address_with_type><u64(amount1)><asset_name1><u64(amount2)><asset_name2>...
pub fn encode_utxos_by_policy_value(
    address: &Address,
    assets: KeyValuePairs<Bytes, u64>,
) -> Vec<u8> {
    let mut buf = Vec::new();

    buf.extend(encode_address_with_type(address));

    for (asset_name, amount) in assets.iter() {
        buf.extend_from_slice(&u64::to_be_bytes(*amount));
        buf.extend(encode_short_bytestring(asset_name));
    }

    buf
}

/// <hash><u16(txcount)><txhash1><txhash2>...<u32(size)><u128(output)><u64(fees)><u32(invocations)><u64(mem)><u64(steps)><headerbytes>
pub fn encode_block_by_height_value(
    hash: &[u8; 32],
    header_bytes: Vec<u8>,
    tx_hashes: Vec<[u8; 32]>,
    size: u32,
    output: u128,
    fees: u64,
    invocations: u32,
    mem: u64,
    steps: u64,
) -> Vec<u8> {
    let expected_len =
        32 + 2 + (tx_hashes.len() * 32) + 4 + 16 + 8 + 4 + 8 + 8 + header_bytes.len();
    let mut buf = Vec::with_capacity(expected_len);

    buf.extend_from_slice(hash);
    buf.extend_from_slice(&u16::to_be_bytes(tx_hashes.len() as u16));

    for tx in tx_hashes {
        buf.extend_from_slice(&tx)
    }

    buf.extend_from_slice(&u32::to_be_bytes(size));
    buf.extend_from_slice(&u128::to_be_bytes(output));
    buf.extend_from_slice(&u64::to_be_bytes(fees));
    buf.extend_from_slice(&u32::to_be_bytes(invocations));
    buf.extend_from_slice(&u64::to_be_bytes(mem));
    buf.extend_from_slice(&u64::to_be_bytes(steps));

    buf.extend(header_bytes);

    assert_eq!(buf.len(), expected_len);

    buf
}

/// bit flags, highest bit means had input involvement, second bit means had output involvement
pub fn encode_txs_by_address_value(input: bool, output: bool) -> Vec<u8> {
    let mut flags: u8 = 0b0000_0000;

    if input {
        flags |= 0b1000_0000;
    }

    if output {
        flags |= 0b0100_0000;
    }

    vec![flags]
}

// --- cursors

/// <u64(slot)><utxo hash (32b)><u64(utxo index)>
pub fn encode_address_utxos_cursor(slot: u64, u_hash: [u8; 32], u_index: u64) -> String {
    let mut buf = Vec::with_capacity(8 + 32 + 8);

    buf.extend_from_slice(&u64::to_be_bytes(slot));
    buf.extend_from_slice(&u_hash);
    buf.extend_from_slice(&u64::to_be_bytes(u_index));

    b64::URL_SAFE_NO_PAD.encode(buf)
}

/// <u16(address index)><u64(slot)><utxo hash (32b)><u64(utxo index)>
pub fn encode_addresses_utxos_cursor(
    address_idx: u16,
    slot: u64,
    u_hash: [u8; 32],
    u_index: u64,
) -> String {
    let mut buf = Vec::with_capacity(2 + 8 + 32 + 8);

    buf.extend_from_slice(&u16::to_be_bytes(address_idx));
    buf.extend_from_slice(&u64::to_be_bytes(slot));
    buf.extend_from_slice(&u_hash);
    buf.extend_from_slice(&u64::to_be_bytes(u_index));

    b64::URL_SAFE_NO_PAD.encode(buf)
}

pub fn encode_policy_holders_cursor(address: &Address) -> String {
    b64::URL_SAFE_NO_PAD.encode(address.to_vec())
}

// <u64(slot)><u16(block index)><tx_hash>
pub fn encode_address_txs_cursor(slot: u64, block_index: u16, tx_hash: [u8; 32]) -> String {
    let mut buf = Vec::new();

    buf.extend_from_slice(&u64::to_be_bytes(slot));
    buf.extend_from_slice(&u16::to_be_bytes(block_index));
    buf.extend_from_slice(&tx_hash);

    b64::URL_SAFE_NO_PAD.encode(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_DATAPLANE_ID: u8 = 1;
    const TEST_INSTANCE_ID: u8 = 4;
    const TEST_HASH: [u8; 32] = [
        12, 154, 13, 198, 78, 167, 125, 174, 3, 95, 163, 220, 122, 187, 115, 231, 201, 200, 112,
        11, 227, 7, 122, 79, 65, 12, 161, 92, 122, 233, 192, 44,
    ];
    const TEST_POLICY: [u8; 28] = [
        214, 54, 247, 59, 243, 67, 181, 254, 74, 129, 55, 251, 2, 206, 136, 22, 103, 202, 153, 209,
        158, 86, 4, 181, 117, 166, 226, 32,
    ];
    const TEST_ASSET_NAME: [u8; 30] = [
        3, 107, 65, 212, 14, 218, 56, 39, 210, 52, 4, 57, 37, 115, 141, 31, 201, 187, 41, 242, 29,
        11, 239, 8, 109, 65, 0, 26, 46, 136,
    ];
    const TEST_SHELLEY_KK_ADDRESS_HEX: &str = "010edb1553eb6b4ad6379a2141bbaec9f67e7456b4180b1e0174f959c3f98c45cd514470b21956d0f9ac816dcc6e9065a70ec717db070b9c7d";
    const TEST_SLOT: u64 = 89736247; // [0x05, 0x59, 0x44, 0x37]
    const TEST_UTXO_INDEX: u64 = 654; // [0x00, 0x00, 0x02, 0x8e]

    #[test]
    fn test_encode_tx_by_hash_key() {
        let encoder = KeyEncoder::new(TEST_DATAPLANE_ID, TEST_INSTANCE_ID);

        // <INSTANCE><DATA><TX_BY_HASH_TAG><txhash>
        let mut expected = vec![
            TEST_DATAPLANE_ID,
            TEST_INSTANCE_ID,
            PREFIX_DATA,
            REDUCER_TX_BY_HASH,
        ];
        expected.extend_from_slice(&TEST_HASH);

        let actual = encoder.encode_tx_by_hash_key(&TEST_HASH);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_encode_tx_count_by_address_key() {
        let encoder = KeyEncoder::new(TEST_DATAPLANE_ID, TEST_INSTANCE_ID);

        // <INSTANCE><DATA><TX_COUNT_BY_SHELLEY_ADDRESS_TAG><address with type>
        let mut expected = vec![
            TEST_DATAPLANE_ID,
            TEST_INSTANCE_ID,
            PREFIX_DATA,
            REDUCER_TX_COUNT_BY_ADDRESS,
        ];
        expected.push(b'S');
        expected.push(1); // network
        expected.push(0);
        expected.extend(
            hex::decode("0edb1553eb6b4ad6379a2141bbaec9f67e7456b4180b1e0174f959c3").unwrap(),
        );
        expected.push(BREAK);
        expected.push(0);
        expected.extend(
            hex::decode("f98c45cd514470b21956d0f9ac816dcc6e9065a70ec717db070b9c7d").unwrap(),
        );

        let actual = encoder.encode_tx_count_by_address_key(
            Address::from_hex(TEST_SHELLEY_KK_ADDRESS_HEX).unwrap(),
        );

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_encode_holders_by_asset_key() {
        let encoder = KeyEncoder::new(TEST_DATAPLANE_ID, TEST_INSTANCE_ID);

        // <INSTANCE><DATA><HOLDERS_BY_ASSET_TAG><policy><BREAK><asset_name><BREAK><address with type>
        let mut expected = vec![
            TEST_DATAPLANE_ID,
            TEST_INSTANCE_ID,
            PREFIX_DATA,
            REDUCER_HOLDERS_BY_ASSET,
        ];
        expected.extend_from_slice(&TEST_POLICY);
        expected.push(BREAK);
        expected.push(TEST_ASSET_NAME.len().try_into().unwrap()); // len of asset name
        expected.extend_from_slice(&TEST_ASSET_NAME);
        expected.push(BREAK);
        expected.push(b'S');
        expected.push(1); // network
        expected.push(0); // pay cred
        expected.extend(
            hex::decode("0edb1553eb6b4ad6379a2141bbaec9f67e7456b4180b1e0174f959c3").unwrap(),
        );
        expected.push(BREAK);
        expected.push(0); // stake cred
        expected.extend(
            hex::decode("f98c45cd514470b21956d0f9ac816dcc6e9065a70ec717db070b9c7d").unwrap(),
        );

        let actual = encoder.encode_holders_by_asset_key(
            TEST_POLICY.into(),
            TEST_ASSET_NAME.to_vec().into(),
            Address::from_hex(TEST_SHELLEY_KK_ADDRESS_HEX).unwrap(),
        );

        assert_eq!(actual, expected)
    }

    #[test]
    fn test_encode_utxos_by_policy_key() {
        let encoder = KeyEncoder::new(TEST_DATAPLANE_ID, TEST_INSTANCE_ID);

        // <INSTANCE><DATA><UTXO_BY_POLICY><policy_id><PLACEHOLDER><u64(slot)><utxo_hash><u64(utxo_idx)>
        let mut expected = vec![
            TEST_DATAPLANE_ID,
            TEST_INSTANCE_ID,
            PREFIX_DATA,
            REDUCER_UTXOS_BY_POLICY,
        ];
        expected.extend_from_slice(&TEST_POLICY);
        expected.push(BREAK);
        expected.extend_from_slice(&[0x00, 0x00, 0x00, 0x00, 0x05, 0x59, 0x44, 0x37]); // TEST_SLOT
        expected.extend_from_slice(&TEST_HASH);
        expected.extend_from_slice(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x8e]); // TEST_UTXO_INDEX

        let actual = encoder.encode_utxos_by_policy_key(
            TEST_POLICY.into(),
            TEST_SLOT,
            TEST_HASH,
            TEST_UTXO_INDEX,
        );

        assert_eq!(actual, expected)
    }

    #[test]
    fn test_encode_utxos_by_asset_key() {
        let encoder = KeyEncoder::new(TEST_DATAPLANE_ID, TEST_INSTANCE_ID);

        // <INSTANCE><DATA><UTXO_BY_ASSET><policy_id><PLACEHOLDER><asset_name><PLACEHOLDER><u64(slot)><utxo_hash><u64(utxo_idx)
        let mut expected = vec![
            TEST_DATAPLANE_ID,
            TEST_INSTANCE_ID,
            PREFIX_DATA,
            REDUCER_UTXOS_BY_ASSET,
        ];
        expected.extend_from_slice(&TEST_POLICY);
        expected.push(BREAK);
        expected.push(TEST_ASSET_NAME.len().try_into().unwrap());
        expected.extend_from_slice(&TEST_ASSET_NAME);
        expected.push(BREAK);
        expected.extend_from_slice(&[0x00, 0x00, 0x00, 0x00, 0x05, 0x59, 0x44, 0x37]); // TEST_SLOT
        expected.extend_from_slice(&TEST_HASH);
        expected.extend_from_slice(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x8e]); // TEST_UTXO_INDEX

        let actual = encoder.encode_utxos_by_asset_key(
            TEST_POLICY.into(),
            TEST_ASSET_NAME.to_vec().into(),
            TEST_SLOT,
            TEST_HASH,
            TEST_UTXO_INDEX,
        );

        assert_eq!(actual, expected)
    }
}
