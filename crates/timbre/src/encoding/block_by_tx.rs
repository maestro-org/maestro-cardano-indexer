use super::encode::{KeyEncoder, PREFIX_DATA, REDUCER_BLOCK_BY_TX};

// <DATAPLANE><INSTANCE><DATA><BLOCK_BY_TX_TAG><HASH>
pub fn encode_block_by_tx_key(encoder: &KeyEncoder, hash: &[u8; 32]) -> Vec<u8> {
    let expected_len = 4 + 32;
    let mut key = Vec::with_capacity(expected_len);

    key.push(encoder.dataplane_id());
    key.push(encoder.instance_id());
    key.push(PREFIX_DATA);

    key.push(REDUCER_BLOCK_BY_TX);

    key.extend_from_slice(hash);

    assert_eq!(key.len(), expected_len);

    key
}

pub struct BlockByTxValue {
    pub height: u64,
    pub index: u16,
}

// <u64(height)><u16(index)>
pub fn encode_block_by_tx_value(height: u64, index: u16) -> Vec<u8> {
    let expected_len = 8 + 2;
    let mut key = Vec::with_capacity(expected_len);

    key.extend_from_slice(&u64::to_be_bytes(height));
    key.extend_from_slice(&u16::to_be_bytes(index));

    assert_eq!(key.len(), expected_len);

    key
}

pub fn decode_block_by_tx_value(bytes: &[u8]) -> BlockByTxValue {
    let mut c = 0; // cursor

    let height = u64::from_be_bytes(bytes[c..c + 8].try_into().unwrap());
    c += 8;

    let index = u16::from_be_bytes(bytes[c..c + 2].try_into().unwrap());

    BlockByTxValue { height, index }
}
