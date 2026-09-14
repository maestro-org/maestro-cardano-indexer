use super::encode::{KeyEncoder, PREFIX_DATA, REDUCER_SCRIPT_BY_HASH};

// <DATAPLANE><INSTANCE><DATA><SCRIPT_BY_HASH_TAG><HASH>
pub fn encode_script_by_hash_key(encoder: &KeyEncoder, hash: &[u8; 28]) -> Vec<u8> {
    let expected_len = 4 + 28;
    let mut key = Vec::with_capacity(expected_len);

    key.push(encoder.dataplane_id());
    key.push(encoder.instance_id());
    key.push(PREFIX_DATA);

    key.push(REDUCER_SCRIPT_BY_HASH);

    key.extend_from_slice(hash);

    assert_eq!(key.len(), expected_len);

    key
}

pub struct ScriptByHashValue {
    pub tx_hash: [u8; 32],
    pub slot: u64,
    pub kind: u8,
    pub bytes: Vec<u8>,
}

// <txhash (32b)><u64(slot)><kind><script bytes>
// kind: 0 for native, 1 for PV1, 2 for PV2
pub fn encode_script_by_hash_value(
    tx_hash: [u8; 32],
    slot: u64,
    kind: u8,
    bytes: Vec<u8>,
) -> Vec<u8> {
    let expected_len = 32 + 8 + 1 + bytes.len();
    let mut key = Vec::with_capacity(expected_len);

    key.extend_from_slice(&tx_hash);
    key.extend_from_slice(&u64::to_be_bytes(slot));
    key.push(kind);
    key.extend(bytes);

    assert_eq!(key.len(), expected_len);

    key
}

pub fn decode_script_by_hash_value(bytes: &[u8]) -> ScriptByHashValue {
    let mut c = 0; // cursor

    let tx_hash: [u8; 32] = bytes[0..32].try_into().unwrap();
    c += 32;

    let slot = u64::from_be_bytes(bytes[c..c + 8].try_into().unwrap());
    c += 8;

    let kind = bytes[c];
    c += 1;

    let script_bytes = bytes[c..].to_vec();
    c += script_bytes.len();

    assert_eq!(c, bytes.len());

    ScriptByHashValue {
        tx_hash,
        slot,
        kind,
        bytes: script_bytes,
    }
}
