use core::fmt;
use std::ops::Range;

use pallas::ledger::addresses::ShelleyPaymentPart;

use super::{
    encode::{
        encode_shelley_payment_cred, KeyEncoder, BREAK, PREFIX_DATA, REDUCER_TXS_BY_PAY_CRED,
    },
    RangeBound, Slot, TimbreError,
};

use base64::{engine::general_purpose as b64, Engine};

pub struct TxsByPayCredKey {
    pub payment_cred: ShelleyPaymentPart,
    pub slot: u64,
    pub block_index: u16,
    pub tx_hash: [u8; 32],
}

impl fmt::Debug for TxsByPayCredKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "TxsByPayCredKey [cred: {}, slot: {}, tx: {}, block_idx: {}]",
            self.payment_cred.to_bech32(),
            self.slot,
            hex::encode(self.tx_hash),
            self.block_index
        )
    }
}

pub fn encode_txs_by_payment_cred_key(
    encoder: &KeyEncoder,
    payment_cred: &ShelleyPaymentPart,
    slot: u64,
    blk_index: u16,
    tx_hash: &[u8; 32],
) -> Vec<u8> {
    let mut key = Vec::new();

    key.push(encoder.dataplane_id());
    key.push(encoder.instance_id());
    key.push(PREFIX_DATA);

    key.push(REDUCER_TXS_BY_PAY_CRED);

    key.extend(encode_shelley_payment_cred(payment_cred));
    key.push(BREAK);
    key.extend(u64::to_be_bytes(slot));
    key.extend(u16::to_be_bytes(blk_index));
    key.extend(tx_hash);

    key
}

pub fn decode_txs_by_payment_cred_key(bytes: &[u8]) -> TxsByPayCredKey {
    assert_eq!(bytes[3], REDUCER_TXS_BY_PAY_CRED);

    let mut cursor = 4;

    let is_key = match bytes[cursor] {
        0 => true,
        1 => false,
        _ => unreachable!(),
    };

    cursor += 1;

    let cred_hash: [u8; 28] = bytes[cursor..cursor + 28].try_into().unwrap();
    cursor += 28;

    // BREAK
    cursor += 1;

    let slot = u64::from_be_bytes(bytes[cursor..cursor + 8].try_into().unwrap());
    cursor += 8;

    let block_index = u16::from_be_bytes(bytes[cursor..cursor + 2].try_into().unwrap());
    cursor += 2;

    let tx_hash: [u8; 32] = bytes[cursor..cursor + 32].try_into().unwrap();

    let payment_cred = if is_key {
        ShelleyPaymentPart::Key(cred_hash.into())
    } else {
        ShelleyPaymentPart::Script(cred_hash.into())
    };

    TxsByPayCredKey {
        payment_cred,
        slot,
        tx_hash,
        block_index,
    }
}

pub struct TxsByPayCredValue {
    pub input: bool,
    pub output: bool,
    pub required_signer: bool,
}

/// bit flags, highest bit means had input involvement
/// second bit means had output involvement
/// third bit means was a required vkey signer
pub fn encode_txs_by_payment_cred_value(
    input: bool,
    output: bool,
    required_signer: bool,
) -> Vec<u8> {
    let mut flags: u8 = 0b0000_0000;

    if input {
        flags |= 0b1000_0000;
    }

    if output {
        flags |= 0b0100_0000;
    }

    if required_signer {
        flags |= 0b0010_0000;
    }

    vec![flags]
}

pub fn decode_txs_by_payment_cred_value(byte: u8) -> TxsByPayCredValue {
    let input = (byte & 0b1000_0000) != 0;
    let output = (byte & 0b0100_0000) != 0;
    let required_signer = (byte & 0b0010_0000) != 0;

    TxsByPayCredValue {
        input,
        output,
        required_signer,
    }
}

#[derive(Clone, Debug)]
pub struct TxsByPayCredCursor {
    pub slot: u64,
    pub tx_hash: [u8; 32],
    pub block_index: u16,
}

impl Slot for TxsByPayCredCursor {
    fn slot(&self) -> u64 {
        self.slot
    }
}

/// Given an address, returns a Key range which will include the
/// Tx by address keys for the address. Can optionally specify a start
/// slot (inclusive) and a end slot (inclusive) to only return txs
/// in that slot range.
pub fn encode_txs_by_payment_cred_range(
    encoder: &KeyEncoder,
    cred: &ShelleyPaymentPart,
    lower: Option<RangeBound<TxsByPayCredCursor>>,
    upper: Option<RangeBound<TxsByPayCredCursor>>,
) -> Range<Vec<u8>> {
    let mut prefix = vec![
        encoder.dataplane_id(),
        encoder.instance_id(),
        PREFIX_DATA,
        REDUCER_TXS_BY_PAY_CRED,
    ];

    prefix.extend(encode_shelley_payment_cred(cred));

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
            // inclusive of results at `end_slot`
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

// <u64(slot)><u16(block index)><tx_hash>
pub fn encode_txs_by_payment_cred_cursor(slot: u64, block_index: u16, tx_hash: [u8; 32]) -> String {
    let mut buf = Vec::new();

    buf.extend_from_slice(&u64::to_be_bytes(slot));
    buf.extend_from_slice(&u16::to_be_bytes(block_index));
    buf.extend_from_slice(&tx_hash);

    b64::URL_SAFE_NO_PAD.encode(buf)
}

pub fn decode_txs_by_payment_cred_cursor(
    b64_cursor: &String,
) -> Result<TxsByPayCredCursor, TimbreError> {
    let cursor = b64::URL_SAFE_NO_PAD
        .decode(b64_cursor)
        .map_err(|_| TimbreError::MalformedCursor)?;

    if cursor.len() != (8 + 2 + 32) {
        return Err(TimbreError::MalformedCursor);
    }

    let slot = u64::from_be_bytes(cursor[0..8].try_into().unwrap());
    let block_index = u16::from_be_bytes(cursor[8..10].try_into().unwrap());
    let tx_hash = cursor[10..42].try_into().unwrap();

    Ok(TxsByPayCredCursor {
        slot,
        tx_hash,
        block_index,
    })
}
