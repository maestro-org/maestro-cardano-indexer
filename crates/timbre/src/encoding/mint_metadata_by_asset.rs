use std::ops::Range;

use crate::encoding::{
    decode::decode_short_bytestring,
    encode::{
        encode_short_bytestring, KeyEncoder, BREAK, PREFIX_DATA, REDUCER_MINT_METADATA_BY_ASSET,
    },
};

use super::{RangeBound, Slot};

pub struct MintMetadataByAssetKey {
    pub policy: [u8; 28],
    pub name: Vec<u8>,
    pub slot: u64,
    pub blk_index: u16,
}

// <DATAPLANE><INSTANCE><DATA><MINT_METADATA_BY_ASSET_TAG><policy><sbs(assetname)><BREAK><u64(slot)><u16(block index)>
pub fn encode_mint_metadata_by_asset_key(
    encoder: &KeyEncoder,
    policy: &[u8; 28],
    name: Vec<u8>,
    slot: u64,
    blk_index: u16,
) -> Vec<u8> {
    let expected_len = 4 + 28 + 1 + name.len() + 1 + 8 + 2;
    let mut key = Vec::with_capacity(expected_len);

    key.push(encoder.dataplane_id());
    key.push(encoder.instance_id());
    key.push(PREFIX_DATA);

    key.push(REDUCER_MINT_METADATA_BY_ASSET);

    key.extend_from_slice(policy);
    key.extend(encode_short_bytestring(&name));

    key.push(BREAK);

    key.extend_from_slice(&u64::to_be_bytes(slot));
    key.extend_from_slice(&u16::to_be_bytes(blk_index));

    assert_eq!(key.len(), expected_len);

    key
}

pub fn decode_mint_metadata_by_asset_key(bytes: &[u8]) -> MintMetadataByAssetKey {
    assert_eq!(bytes[3], REDUCER_MINT_METADATA_BY_ASSET);

    let mut c = 4; // cursor

    let policy: [u8; 28] = bytes[c..c + 28].try_into().unwrap();
    c += 28;

    let (name, name_len) = decode_short_bytestring(&bytes[c..]);
    c += name_len;

    c += 1; // BREAK

    let slot = u64::from_be_bytes(bytes[c..c + 8].try_into().unwrap());
    c += 8;

    let blk_index = u16::from_be_bytes(bytes[c..c + 2].try_into().unwrap());
    // c += 2;

    MintMetadataByAssetKey {
        policy,
        name: name.to_vec(),
        slot,
        blk_index,
    }
}

#[derive(Clone, Debug)]
pub struct MintMetadataByAssetCursor {
    pub slot: u64,
    pub blk_index: u16,
}

impl Slot for MintMetadataByAssetCursor {
    fn slot(&self) -> u64 {
        self.slot
    }
}

pub fn encode_mint_metadata_by_asset_range(
    encoder: &KeyEncoder,
    policy: &[u8; 28],
    name: Vec<u8>,
    lower: Option<RangeBound<MintMetadataByAssetCursor>>,
    upper: Option<RangeBound<MintMetadataByAssetCursor>>,
) -> Range<Vec<u8>> {
    let mut prefix = vec![
        encoder.dataplane_id(),
        encoder.instance_id(),
        PREFIX_DATA,
        REDUCER_MINT_METADATA_BY_ASSET,
    ];

    prefix.extend_from_slice(policy);
    prefix.extend(encode_short_bytestring(&name));

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
            buf.extend_from_slice(&u16::to_be_bytes(cursor.blk_index));
            buf
        }
    };

    let end_key = match upper {
        Some(RangeBound::Cursor(cursor)) => {
            prefix.push(BREAK);
            prefix.extend_from_slice(&u64::to_be_bytes(cursor.slot));
            prefix.extend_from_slice(&u16::to_be_bytes(cursor.blk_index));
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

// <txhash><i64(mint amount)><metadata bytes>
pub fn encode_mint_metadata_by_asset_value(
    tx_hash: [u8; 32],
    amount: i64,
    metadata: Vec<u8>,
) -> Vec<u8> {
    let expected_len = 32 + 8 + metadata.len();
    let mut key = Vec::with_capacity(expected_len);

    key.extend_from_slice(&tx_hash);
    key.extend_from_slice(&i64::to_be_bytes(amount));

    key.extend(metadata);

    assert_eq!(key.len(), expected_len);

    key
}

pub fn decode_mint_metadata_by_asset_value(bytes: &[u8]) -> ([u8; 32], i64, Vec<u8>) {
    let mut c = 0; // cursor

    let tx_hash: [u8; 32] = bytes[0..32].try_into().unwrap();
    c += 32;

    let amount = i64::from_be_bytes(bytes[c..c + 8].try_into().unwrap());
    c += 8;

    let metadata_bytes = bytes[c..].to_vec();
    c += metadata_bytes.len();

    assert_eq!(c, bytes.len());

    (tx_hash, amount, metadata_bytes)
}
