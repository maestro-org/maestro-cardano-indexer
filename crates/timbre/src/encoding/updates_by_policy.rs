use std::ops::Range;

use crate::encoding::encode::{
    encode_short_bytestring, KeyEncoder, BREAK, PREFIX_DATA, REDUCER_UPDATES_BY_POLICY,
};

use super::{decode::decode_short_bytestring, RangeBound, Slot, TimbreError};

use base64::{engine::general_purpose as b64, Engine};

pub struct UpdatesByPolicyKey {
    pub policy: [u8; 28],
    pub slot: u64,
    pub blk_index: u16,
}

// <DATAPLANE><INSTANCE><DATA><UPDATES_BY_POLICY_TAG><policy><BREAK><u64(slot)><u16(block index)>
pub fn encode_updates_by_policy_key(
    encoder: &KeyEncoder,
    policy: &[u8; 28],
    slot: u64,
    blk_index: u16,
) -> Vec<u8> {
    let expected_len = 4 + 28 + 1 + 8 + 2;
    let mut key = Vec::with_capacity(expected_len);

    key.push(encoder.dataplane_id());
    key.push(encoder.instance_id());
    key.push(PREFIX_DATA);

    key.push(REDUCER_UPDATES_BY_POLICY);

    key.extend_from_slice(policy);

    key.push(BREAK);

    key.extend_from_slice(&u64::to_be_bytes(slot));
    key.extend_from_slice(&u16::to_be_bytes(blk_index));

    assert_eq!(key.len(), expected_len);

    key
}

pub fn decode_updates_by_policy_key(bytes: &[u8]) -> UpdatesByPolicyKey {
    assert_eq!(bytes[3], REDUCER_UPDATES_BY_POLICY);

    let mut c = 4; // cursor

    let policy: [u8; 28] = bytes[c..c + 28].try_into().unwrap();
    c += 28;

    c += 1; // BREAK

    let slot = u64::from_be_bytes(bytes[c..c + 8].try_into().unwrap());
    c += 8;

    let blk_index = u16::from_be_bytes(bytes[c..c + 2].try_into().unwrap());
    // c += 2;

    UpdatesByPolicyKey {
        policy,
        slot,
        blk_index,
    }
}

#[derive(Clone, Debug)]
pub struct UpdatesByPolicyCursor {
    pub slot: u64,
    pub blk_index: u16,
}

impl Slot for UpdatesByPolicyCursor {
    fn slot(&self) -> u64 {
        self.slot
    }
}

pub fn encode_updates_by_policy_range(
    encoder: &KeyEncoder,
    policy: &[u8; 28],
    lower: Option<RangeBound<UpdatesByPolicyCursor>>,
    upper: Option<RangeBound<UpdatesByPolicyCursor>>,
) -> Range<Vec<u8>> {
    let mut prefix = vec![
        encoder.dataplane_id(),
        encoder.instance_id(),
        PREFIX_DATA,
        REDUCER_UPDATES_BY_POLICY,
    ];

    prefix.extend(policy);

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
            buf.push(0);
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

// <txhash (32b)><sbs(assetname1)><i64(assetamount1)><sbs(assetname2)><i64(assetamount2)>
pub fn encode_updates_by_policy_value(tx_hash: [u8; 32], assets: Vec<(Vec<u8>, i64)>) -> Vec<u8> {
    let expected_len = 32 + assets.iter().map(|x| 1 + x.0.len() + 8).sum::<usize>();
    let mut key = Vec::with_capacity(expected_len);

    key.extend_from_slice(&tx_hash);

    for (asset, amount) in assets {
        key.extend(encode_short_bytestring(&asset));
        key.extend_from_slice(&i64::to_be_bytes(amount))
    }

    assert_eq!(key.len(), expected_len);

    key
}

pub fn decode_updates_by_policy_value(bytes: &[u8]) -> ([u8; 32], Vec<(Vec<u8>, i64)>) {
    let mut c = 0; // cursor

    let tx_hash: [u8; 32] = bytes[0..32].try_into().unwrap();
    c += 32;

    let mut assets = Vec::new();

    loop {
        if c == bytes.len() {
            return (tx_hash, assets);
        } else {
            let (name, name_len) = decode_short_bytestring(&bytes[c..]);
            c += name_len;

            let amount = i64::from_be_bytes(bytes[c..c + 8].try_into().unwrap());
            c += 8;

            assets.push((name.to_vec(), amount))
        }
    }
}

/// <u64(slot)><u16(blk_index)>
pub fn encode_updates_by_policy_cursor(slot: u64, blk_index: u16) -> String {
    let mut buf = Vec::with_capacity(8 + 2);

    buf.extend_from_slice(&u64::to_be_bytes(slot));
    buf.extend_from_slice(&u16::to_be_bytes(blk_index));

    b64::URL_SAFE_NO_PAD.encode(buf)
}

pub fn decode_updates_by_policy_cursor(
    b64_cursor: &String,
) -> Result<UpdatesByPolicyCursor, TimbreError> {
    let cursor = b64::URL_SAFE_NO_PAD
        .decode(b64_cursor)
        .map_err(|_| TimbreError::MalformedCursor)?;

    if cursor.len() != (8 + 2) {
        return Err(TimbreError::MalformedCursor);
    }

    let slot = u64::from_be_bytes(cursor[0..8].try_into().unwrap());
    let blk_index = u16::from_be_bytes(cursor[8..10].try_into().unwrap());

    Ok(UpdatesByPolicyCursor { slot, blk_index })
}
