// uses utxos by address keys

use std::ops::Range;

use pallas::ledger::addresses::{ShelleyAddress, ShelleyPaymentPart};

use super::{
    decode::try_decode_shelley_address,
    encode::{
        encode_shelley_address, encode_shelley_payment_cred, KeyEncoder, BREAK, PREFIX_DATA,
        REDUCER_UTXOS_BY_SHELLEY_ADDRESS,
    },
    Slot, TimbreError,
};

use base64::{engine::general_purpose as b64, Engine};

/// Given an Shelley address, returns a Key range which will include the
/// UTxO by address keys for the address. Can optionally specify a start
/// slot (inclusive) and a end slot (inclusive) to only return UTxOs created
/// in that slot range.
pub fn encode_utxos_by_payment_cred_range(
    encoder: &KeyEncoder,
    network: u8,
    payment: &ShelleyPaymentPart,
    lower: Option<UtxosByPaymentCredCursor>,
    upper: Option<UtxosByPaymentCredCursor>,
) -> Range<Vec<u8>> {
    let mut prefix = vec![
        encoder.dataplane_id(),
        encoder.instance_id(),
        PREFIX_DATA,
        REDUCER_UTXOS_BY_SHELLEY_ADDRESS,
    ];

    let start_key = match lower {
        None => {
            let mut buf = prefix.clone();
            buf.push(network);
            buf.extend(encode_shelley_payment_cred(payment));
            buf.push(BREAK);
            buf
        }
        Some(UtxosByPaymentCredCursor {
            address,
            slot,
            u_hash,
            u_index,
        }) => {
            let mut buf = prefix.clone();
            buf.extend_from_slice(&encode_shelley_address(&address));
            buf.push(BREAK);
            buf.extend_from_slice(&u64::to_be_bytes(slot));
            buf.extend_from_slice(&u_hash);
            buf.extend_from_slice(&u64::to_be_bytes(u_index));
            buf.push(0);
            buf
        }
    };

    let end_key = match upper {
        Some(UtxosByPaymentCredCursor {
            address,
            slot,
            u_hash,
            u_index,
        }) => {
            prefix.extend_from_slice(&encode_shelley_address(&address));
            prefix.push(BREAK);
            prefix.extend_from_slice(&u64::to_be_bytes(slot));
            prefix.extend_from_slice(&u_hash);
            prefix.extend_from_slice(&u64::to_be_bytes(u_index));

            prefix
        }
        None => {
            prefix.push(network);
            prefix.extend(encode_shelley_payment_cred(payment));
            prefix.push(BREAK + 1);
            prefix
        }
    };

    start_key..end_key
}

#[derive(Clone, Debug)]
pub struct UtxosByPaymentCredCursor {
    pub address: ShelleyAddress,
    pub slot: u64,
    pub u_hash: [u8; 32],
    pub u_index: u64,
}

impl Slot for UtxosByPaymentCredCursor {
    fn slot(&self) -> u64 {
        self.slot
    }
}

pub fn encode_utxos_by_payment_cred_cursor(
    address: &ShelleyAddress,
    slot: u64,
    u_hash: [u8; 32],
    u_index: u64,
) -> String {
    let mut buf = Vec::new();

    buf.extend_from_slice(&encode_shelley_address(address));
    buf.extend_from_slice(&u64::to_be_bytes(slot));
    buf.extend_from_slice(&u_hash);
    buf.extend_from_slice(&u64::to_be_bytes(u_index));

    b64::URL_SAFE_NO_PAD.encode(buf)
}

pub fn decode_utxos_by_payment_cred_cursor(
    b64_cursor: &String,
) -> Result<UtxosByPaymentCredCursor, TimbreError> {
    let cursor = b64::URL_SAFE_NO_PAD
        .decode(b64_cursor)
        .map_err(|_| TimbreError::MalformedCursor)?;

    let addr_len_with_deleg_hash = 1 + 29 + 1 + 29;
    let addr_len_with_pointer = 1 + 29 + 1 + 25;
    let addr_len_no_deleg = 1 + 29 + 1 + 1;

    // check cursor is correct size for one kind of shelley address encoding
    if cursor.len() != (addr_len_with_deleg_hash + 8 + 32 + 8)
        && cursor.len() != (addr_len_with_pointer + 8 + 32 + 8)
        && cursor.len() != (addr_len_no_deleg + 8 + 32 + 8)
    {
        return Err(TimbreError::MalformedCursor);
    };

    let (address, mut consumed) = try_decode_shelley_address(&cursor)?;

    // the decoded address shape must account for exactly the non-suffix bytes
    if consumed + 8 + 32 + 8 != cursor.len() {
        return Err(TimbreError::MalformedCursor);
    }

    let slot = u64::from_be_bytes(cursor[consumed..consumed + 8].try_into().unwrap());
    consumed += 8;

    let u_hash = cursor[consumed..consumed + 32].try_into().unwrap();
    consumed += 32;

    let u_index = u64::from_be_bytes(cursor[consumed..consumed + 8].try_into().unwrap());

    Ok(UtxosByPaymentCredCursor {
        address,
        slot,
        u_hash,
        u_index,
    })
}

#[derive(Clone, Debug)]
pub struct UtxosByPaymentCredsCursor {
    pub cred_idx: u16,
    pub address: ShelleyAddress,
    pub slot: u64,
    pub u_hash: [u8; 32],
    pub u_index: u64,
}

impl Slot for UtxosByPaymentCredsCursor {
    fn slot(&self) -> u64 {
        self.slot
    }
}

pub fn encode_utxos_by_payment_creds_cursor(
    cred_idx: u16,
    address: &ShelleyAddress,
    slot: u64,
    u_hash: [u8; 32],
    u_index: u64,
) -> String {
    let mut buf = Vec::new();

    buf.extend_from_slice(&u16::to_be_bytes(cred_idx));
    buf.extend_from_slice(&encode_shelley_address(address));
    buf.extend_from_slice(&u64::to_be_bytes(slot));
    buf.extend_from_slice(&u_hash);
    buf.extend_from_slice(&u64::to_be_bytes(u_index));

    b64::URL_SAFE_NO_PAD.encode(buf)
}

pub fn decode_utxos_by_payment_creds_cursor(
    b64_cursor: &String,
) -> Result<UtxosByPaymentCredsCursor, TimbreError> {
    let cursor = b64::URL_SAFE_NO_PAD
        .decode(b64_cursor)
        .map_err(|_| TimbreError::MalformedCursor)?;

    let addr_len_with_deleg_hash = 1 + 29 + 1 + 29;
    let addr_len_with_pointer = 1 + 29 + 1 + 25;
    let addr_len_no_deleg = 1 + 29 + 1 + 1;

    // check cursor is correct size for one kind of shelley address encoding
    if cursor.len() != (2 + addr_len_with_deleg_hash + 8 + 32 + 8)
        && cursor.len() != (2 + addr_len_with_pointer + 8 + 32 + 8)
        && cursor.len() != (2 + addr_len_no_deleg + 8 + 32 + 8)
    {
        return Err(TimbreError::MalformedCursor);
    };

    let mut c = 0;

    let cred_idx = u16::from_be_bytes(cursor[c..c + 2].try_into().unwrap());
    c += 2;

    let (address, consumed) = try_decode_shelley_address(&cursor[c..])?;

    // the decoded address shape must account for exactly the non-suffix bytes
    if c + consumed + 8 + 32 + 8 != cursor.len() {
        return Err(TimbreError::MalformedCursor);
    }

    c += consumed;

    let slot = u64::from_be_bytes(cursor[c..c + 8].try_into().unwrap());
    c += 8;

    let u_hash = cursor[c..c + 32].try_into().unwrap();
    c += 32;

    let u_index = u64::from_be_bytes(cursor[c..c + 8].try_into().unwrap());

    Ok(UtxosByPaymentCredsCursor {
        cred_idx,
        address,
        slot,
        u_hash,
        u_index,
    })
}

#[cfg(test)]
mod hardening_tests {
    use super::*;
    use base64::{engine::general_purpose as b64, Engine};

    /// A cursor with a valid total length but an unknown delegation kind byte
    /// previously hit `unreachable!` in the address decoder.
    #[test]
    fn cursor_rejects_unknown_delegation_kind() {
        let mut raw = vec![0u8; 60 + 8 + 32 + 8]; // deleg-hash shape
        raw[1] = 0; // valid payment kind
        raw[31] = 9; // invalid delegation kind
        let cursor = b64::URL_SAFE_NO_PAD.encode(&raw);
        assert!(decode_utxos_by_payment_cred_cursor(&cursor).is_err());
    }

    /// A cursor sized for the no-delegation shape but whose delegation byte
    /// claims a longer shape previously read out of bounds.
    #[test]
    fn cursor_rejects_shape_length_mismatch() {
        let mut raw = vec![0u8; 32 + 8 + 32 + 8]; // no-deleg total length
        raw[1] = 0; // valid payment kind
        raw[31] = 0; // claims deleg-hash shape (needs 60 bytes of address)
        let cursor = b64::URL_SAFE_NO_PAD.encode(&raw);
        assert!(decode_utxos_by_payment_cred_cursor(&cursor).is_err());
    }

    #[test]
    fn cursor_roundtrips_valid_input() {
        use pallas::ledger::addresses::{
            Network, ShelleyAddress, ShelleyDelegationPart, ShelleyPaymentPart,
        };

        let addr = ShelleyAddress::new(
            Network::Testnet,
            ShelleyPaymentPart::Key([7u8; 28].into()),
            ShelleyDelegationPart::Key([9u8; 28].into()),
        );

        let cursor = encode_utxos_by_payment_cred_cursor(&addr, 42, [1u8; 32], 3);
        let Ok(decoded) = decode_utxos_by_payment_cred_cursor(&cursor) else {
            panic!("valid cursor failed to decode");
        };
        assert_eq!(decoded.slot, 42);
        assert_eq!(decoded.u_index, 3);
    }
}
