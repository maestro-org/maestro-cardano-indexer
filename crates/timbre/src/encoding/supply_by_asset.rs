use std::ops::{Deref, Range};

use pallas::ledger::primitives::babbage::PolicyId;

use super::{
    encode::{KeyEncoder, BREAK, PREFIX_DATA, REDUCER_SUPPLY_BY_ASSET},
    RangeBound, TimbreError,
};

use base64::{engine::general_purpose as b64, Engine};

// <DATAPLANE><INSTANCE><DATA><SUPPLY_BY_ASSET_TAG><POLICY><BREAK><ASSETNAME>
pub fn encode_supply_by_asset_key(
    encoder: &KeyEncoder,
    policy: &PolicyId,
    name: Vec<u8>,
) -> Vec<u8> {
    let expected_len = 4 + 28 + 1 + (name.len());
    let mut key = Vec::with_capacity(expected_len);

    key.push(encoder.dataplane_id());
    key.push(encoder.instance_id());
    key.push(PREFIX_DATA);

    key.push(REDUCER_SUPPLY_BY_ASSET);

    key.extend_from_slice(policy.deref());
    key.push(BREAK);
    key.extend(&name);

    assert_eq!(key.len(), expected_len);

    key
}

pub struct SupplyByAssetKey {
    pub policy: [u8; 28],
    pub asset_name: Vec<u8>,
}

pub fn decode_supply_by_asset_key(bytes: &[u8]) -> SupplyByAssetKey {
    assert_eq!(bytes[3], REDUCER_SUPPLY_BY_ASSET);

    let mut c = 4; // cursor

    let policy: [u8; 28] = bytes[c..c + 28].try_into().unwrap();
    c += 28;

    c += 1; // BREAK

    let asset_name = bytes[c..].to_vec();

    SupplyByAssetKey { policy, asset_name }
}

pub fn encode_supply_by_asset_range(
    encoder: &KeyEncoder,
    policy: &PolicyId,
    lower: Option<RangeBound<Vec<u8>>>, // Asset name
    upper: Option<RangeBound<Vec<u8>>>, // Asset name
) -> Range<Vec<u8>> {
    let mut prefix = vec![
        encoder.dataplane_id(),
        encoder.instance_id(),
        PREFIX_DATA,
        REDUCER_SUPPLY_BY_ASSET,
    ];

    prefix.extend_from_slice(policy.deref());

    let start_key = match lower {
        None => {
            let mut buf = prefix.clone();
            buf.push(BREAK);
            buf
        }
        Some(RangeBound::Slot(_)) => unreachable!(),
        Some(RangeBound::Cursor(asset_name)) => {
            let mut buf = prefix.clone();
            buf.push(BREAK);
            buf.extend_from_slice(&asset_name);
            buf.push(0);
            buf
        }
    };

    let end_key = match upper {
        Some(RangeBound::Cursor(asset_name)) => {
            prefix.push(BREAK);
            prefix.extend_from_slice(&asset_name);

            prefix
        }
        Some(RangeBound::Slot(_)) => unreachable!(),
        None => {
            prefix.push(BREAK + 1);
            prefix
        }
    };

    start_key..end_key
}

/// <asset_name>
pub fn encode_supply_by_asset_cursor(asset_name: Vec<u8>) -> String {
    b64::URL_SAFE_NO_PAD.encode(asset_name)
}

pub fn decode_supply_by_asset_cursor(b64_cursor: &String) -> Result<Vec<u8>, TimbreError> {
    let cursor = b64::URL_SAFE_NO_PAD
        .decode(b64_cursor)
        .map_err(|_| TimbreError::MalformedCursor)?;

    Ok(cursor)
}
