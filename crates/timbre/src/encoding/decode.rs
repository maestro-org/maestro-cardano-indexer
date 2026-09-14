use std::fmt;

use pallas::{
    ledger::{
        addresses::{
            Address, ByronAddress, Network, Pointer, ShelleyAddress, ShelleyDelegationPart,
            ShelleyPaymentPart,
        },
        traverse::OutputRef,
    },
    network::miniprotocols::Point,
};

use base64::{engine::general_purpose as b64, Engine};

use crate::{
    encoding::encode::{REDUCER_BLOCK_BY_HEIGHT, REDUCER_TXS_BY_ADDRESS},
    BlockByHeightKey, DatumByHashKey, EnrichRollbackKey, HeightByBlockHashKey, Key, ReducerKey,
    RollbackKey, StorageRollbackKey, TxByHashKey, TxsByAddressKey, UtxosByAddressKey,
    UtxosByAssetKey,
};

use super::{
    encode::{
        PREFIX_CURSOR, PREFIX_DATA, PREFIX_ROLLBACK, REDUCER_DATUM_BY_HASH,
        REDUCER_HOLDERS_BY_ASSET, REDUCER_LOVELACE_BY_ADDRESS, REDUCER_TX_BY_HASH,
        REDUCER_TX_COUNT_BY_ADDRESS, REDUCER_UTXOS_BY_ASSET, REDUCER_UTXOS_BY_BYRON_ADDRESS,
        REDUCER_UTXOS_BY_POLICY, REDUCER_UTXOS_BY_SHELLEY_ADDRESS,
    },
    TimbreError, TxsByAddressCursor, UtxosByAddressCursor, UtxosByAddressesCursor, ADDRESS_BYRON,
    ADDRESS_SHELLEY, ROLLBACK_ENRICH_UTXO, ROLLBACK_INVERSE_OPERATION,
};

/// Byron addresses in keys are stored either as the address or a hash of the
/// address depending on the length of the address.
#[derive(PartialEq, Eq, Hash, Clone)]
pub enum DecodedAddress {
    Address(Address),
    Hash([u8; 32]),
}

impl fmt::Debug for DecodedAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Address(a) => write!(f, "{}", a),
            Self::Hash(h) => write!(f, "AddrHash({})", hex::encode(h)),
        }
    }
}

/// `<u64(b_slot)><b_hash><u64(b_height)>`
pub fn decode_cursor_value(bytes: &[u8]) -> (Point, u64) {
    match bytes[0] {
        b'O' => (Point::Origin, 0),
        b'S' => {
            let b_slot = u64::from_be_bytes(bytes[1..9].try_into().unwrap());
            let b_hash: [u8; 32] = bytes[9..41].try_into().unwrap();
            let b_height = u64::from_be_bytes(bytes[41..49].try_into().unwrap());

            (Point::new(b_slot, b_hash.into()), b_height)
        }
        _ => unreachable!(),
    }
}

pub fn decode_rollback_metadata_key(bytes: &[u8]) -> RollbackKey {
    // 0, 1, 2, 3...
    let b_slot = u64::from_be_bytes(bytes[4..12].try_into().unwrap());
    let b_hash: [u8; 32] = bytes[12..44].try_into().unwrap();

    let point = Point::Specific(b_slot, b_hash.into());

    match bytes[44] {
        ROLLBACK_ENRICH_UTXO => {
            let item_idx = u64::from_be_bytes(bytes[45..53].try_into().unwrap());
            let u_hash: [u8; 32] = bytes[53..85].try_into().unwrap();
            let u_index = u64::from_be_bytes(bytes[85..93].try_into().unwrap());

            RollbackKey::Enrich(EnrichRollbackKey {
                point,
                item_idx,
                utxo_ref: OutputRef::new(u_hash.into(), u_index),
            })
        }
        ROLLBACK_INVERSE_OPERATION => {
            let item_idx = u64::from_be_bytes(bytes[45..53].try_into().unwrap());
            let modified_key = &bytes[53..];

            RollbackKey::Storage(StorageRollbackKey {
                point,
                item_idx,
                key: modified_key.into(),
            })
        }
        _ => panic!("unexpected rb md tag"),
    }
}

pub fn decode_reducer_key(bytes: &[u8]) -> ReducerKey {
    match bytes[3] {
        REDUCER_TX_BY_HASH => {
            let tx_hash: [u8; 32] = bytes[4..36].try_into().unwrap();

            ReducerKey::Tx(crate::TxByHashKey { tx_hash })
        }
        REDUCER_TX_COUNT_BY_ADDRESS => {
            let address = decode_address_with_type(&bytes[4..]).0;

            ReducerKey::TxCount(crate::TxCountByAddressKey { address })
        }
        REDUCER_HOLDERS_BY_ASSET => unimplemented!(),
        REDUCER_UTXOS_BY_POLICY => {
            let policy_bytes: [u8; 28] = bytes[4..32].try_into().unwrap();
            let _break = bytes[32];

            let slot = u64::from_be_bytes(bytes[33..41].try_into().unwrap());
            let utxo_hash: [u8; 32] = bytes[41..73].try_into().unwrap();
            let utxo_index = u64::from_be_bytes(bytes[73..81].try_into().unwrap());

            ReducerKey::PolicyUtxos(crate::UtxosByPolicyKey {
                policy: policy_bytes.into(),
                slot,
                utxo_hash,
                utxo_index,
            })
        }
        REDUCER_UTXOS_BY_ASSET => {
            let policy_bytes: [u8; 28] = bytes[4..32].try_into().unwrap();

            let (asset_name_bytes, consumed) = decode_short_bytestring(&bytes[33..]);
            let rem = &bytes[(33 + consumed)..];

            let slot = u64::from_be_bytes(rem[1..9].try_into().unwrap());
            let utxo_hash: [u8; 32] = rem[9..41].try_into().unwrap();
            let utxo_index = u64::from_be_bytes(rem[41..49].try_into().unwrap());

            ReducerKey::AssetUtxos(crate::UtxosByAssetKey {
                policy: policy_bytes.into(),
                asset_name: asset_name_bytes.to_vec().into(),
                slot,
                utxo_hash,
                utxo_index,
            })
        }
        REDUCER_LOVELACE_BY_ADDRESS => {
            let address = decode_address_with_type(&bytes[4..]).0;

            ReducerKey::AddressBalance(crate::LovelaceByAddressKey { address })
        }
        REDUCER_UTXOS_BY_SHELLEY_ADDRESS => {
            let (address, addr_consumed) = decode_shelley_address(&bytes[4..]);

            let mut cursor = 4 + addr_consumed;

            // BREAK
            cursor += 1;

            let slot = u64::from_be_bytes(bytes[cursor..cursor + 8].try_into().unwrap());
            let utxo_hash: [u8; 32] = bytes[cursor + 8..cursor + 8 + 32].try_into().unwrap();
            let utxo_index = u64::from_be_bytes(
                bytes[cursor + 8 + 32..cursor + 8 + 32 + 8]
                    .try_into()
                    .unwrap(),
            );

            ReducerKey::AddressUtxos(crate::UtxosByAddressKey {
                address: DecodedAddress::Address(address.into()),
                slot,
                utxo_hash,
                utxo_index,
            })
        }
        REDUCER_UTXOS_BY_BYRON_ADDRESS => {
            let (address, addr_consumed) = decode_byron_address(&bytes[4..]);

            let mut cursor = 4 + addr_consumed;

            // BREAK
            cursor += 1;

            let slot = u64::from_be_bytes(bytes[cursor..cursor + 8].try_into().unwrap());
            let utxo_hash: [u8; 32] = bytes[cursor + 8..cursor + 8 + 32].try_into().unwrap();
            let utxo_index = u64::from_be_bytes(
                bytes[cursor + 8 + 32..cursor + 8 + 32 + 8]
                    .try_into()
                    .unwrap(),
            );

            ReducerKey::AddressUtxos(crate::UtxosByAddressKey {
                address,
                slot,
                utxo_hash,
                utxo_index,
            })
        }
        REDUCER_DATUM_BY_HASH => {
            let datum_hash: [u8; 32] = bytes[4..36].try_into().unwrap();

            ReducerKey::Datum(crate::DatumByHashKey { datum_hash })
        }
        _ => unreachable!("unexpected reducer tag"),
    }
}

pub fn decode_tx_by_hash_key(bytes: &[u8]) -> TxByHashKey {
    assert_eq!(bytes[3], REDUCER_TX_BY_HASH);

    TxByHashKey {
        tx_hash: bytes[4..36].try_into().unwrap(),
    }
}

pub fn decode_utxos_by_asset_key(bytes: &[u8]) -> UtxosByAssetKey {
    assert_eq!(bytes[3], REDUCER_UTXOS_BY_ASSET);

    let policy_bytes: [u8; 28] = bytes[4..32].try_into().unwrap();

    let (asset_name_bytes, consumed) = decode_short_bytestring(&bytes[33..]);
    let rem = &bytes[(33 + consumed)..];

    let slot = u64::from_be_bytes(rem[1..9].try_into().unwrap());
    let utxo_hash: [u8; 32] = rem[9..41].try_into().unwrap();
    let utxo_index = u64::from_be_bytes(rem[41..49].try_into().unwrap());

    UtxosByAssetKey {
        policy: policy_bytes.into(),
        asset_name: asset_name_bytes.to_vec().into(),
        slot,
        utxo_hash,
        utxo_index,
    }
}

pub fn decode_utxos_by_address_key(bytes: &[u8]) -> UtxosByAddressKey {
    match bytes[3] {
        REDUCER_UTXOS_BY_SHELLEY_ADDRESS => {
            let (address, addr_consumed) = decode_shelley_address(&bytes[4..]);

            let mut cursor = 4 + addr_consumed;

            // BREAK
            cursor += 1;

            let slot = u64::from_be_bytes(bytes[cursor..cursor + 8].try_into().unwrap());
            let utxo_hash: [u8; 32] = bytes[cursor + 8..cursor + 8 + 32].try_into().unwrap();
            let utxo_index = u64::from_be_bytes(
                bytes[cursor + 8 + 32..cursor + 8 + 32 + 8]
                    .try_into()
                    .unwrap(),
            );

            UtxosByAddressKey {
                address: DecodedAddress::Address(address.into()),
                slot,
                utxo_hash,
                utxo_index,
            }
        }
        REDUCER_UTXOS_BY_BYRON_ADDRESS => {
            let (address, addr_consumed) = decode_byron_address(&bytes[4..]);

            let mut cursor = 4 + addr_consumed;

            // BREAK
            cursor += 1;

            let slot = u64::from_be_bytes(bytes[cursor..cursor + 8].try_into().unwrap());
            let utxo_hash: [u8; 32] = bytes[cursor + 8..cursor + 8 + 32].try_into().unwrap();
            let utxo_index = u64::from_be_bytes(
                bytes[cursor + 8 + 32..cursor + 8 + 32 + 8]
                    .try_into()
                    .unwrap(),
            );

            UtxosByAddressKey {
                address,
                slot,
                utxo_hash,
                utxo_index,
            }
        }
        _ => panic!(),
    }
}

pub fn decode_datum_by_hash_key(bytes: &[u8]) -> DatumByHashKey {
    assert_eq!(bytes[3], REDUCER_DATUM_BY_HASH);

    DatumByHashKey {
        datum_hash: bytes[4..36].try_into().unwrap(),
    }
}

// two keys relating to same reducer (1)
pub fn decode_block_by_height_key(bytes: &[u8]) -> BlockByHeightKey {
    assert_eq!(bytes[3], REDUCER_BLOCK_BY_HEIGHT);
    assert_eq!(bytes[4], 0);

    BlockByHeightKey {
        block_height: u64::from_be_bytes(bytes[5..13].try_into().unwrap()),
    }
}

// two keys relating to same reducer (2)
pub fn decode_height_by_block_hash_key(bytes: &[u8]) -> HeightByBlockHashKey {
    assert_eq!(bytes[3], REDUCER_BLOCK_BY_HEIGHT);
    assert_eq!(bytes[4], 1);

    HeightByBlockHashKey {
        block_hash: bytes[5..37].try_into().unwrap(),
    }
}

pub fn decode_txs_by_address_key(bytes: &[u8]) -> TxsByAddressKey {
    assert_eq!(bytes[3], REDUCER_TXS_BY_ADDRESS);

    let mut cursor = 4;
    let (address, addr_len) = decode_address_with_type(&bytes[cursor..]);
    cursor += addr_len;

    // BREAK
    cursor += 1;

    let slot = u64::from_be_bytes(bytes[cursor..cursor + 8].try_into().unwrap());
    cursor += 8;

    let block_index = u16::from_be_bytes(bytes[cursor..cursor + 2].try_into().unwrap());
    cursor += 2;

    let tx_hash: [u8; 32] = bytes[cursor..cursor + 32].try_into().unwrap();

    TxsByAddressKey {
        address,
        slot,
        tx_hash,
        block_index,
    }
}

pub fn decode_key(bytes: &[u8]) -> Key {
    let dataplane = bytes[0];
    let instance = bytes[1];

    match bytes[2] {
        PREFIX_CURSOR => Key::Cursor((dataplane, instance)),
        PREFIX_DATA => Key::Reducer((dataplane, instance, decode_reducer_key(bytes))),
        PREFIX_ROLLBACK => {
            Key::Rollback((dataplane, instance, decode_rollback_metadata_key(bytes)))
        }
        _ => unreachable!("unexpected prefix type"),
    }
}

pub fn decode_utxos_by_address_cursor(
    b64_cursor: &String,
) -> Result<UtxosByAddressCursor, TimbreError> {
    let cursor = b64::URL_SAFE_NO_PAD
        .decode(b64_cursor)
        .map_err(|_| TimbreError::MalformedCursor)?;

    if cursor.len() != (8 + 32 + 8) {
        return Err(TimbreError::MalformedCursor);
    }

    let slot = u64::from_be_bytes(cursor[0..8].try_into().unwrap());
    let u_hash = cursor[8..40].try_into().unwrap();
    let u_index = u64::from_be_bytes(cursor[40..48].try_into().unwrap());

    Ok(UtxosByAddressCursor {
        slot,
        u_hash,
        u_index,
    })
}

pub fn decode_utxos_by_addresses_cursor(
    b64_cursor: &String,
) -> Result<UtxosByAddressesCursor, TimbreError> {
    let cursor = b64::URL_SAFE_NO_PAD
        .decode(b64_cursor)
        .map_err(|_| TimbreError::MalformedCursor)?;

    if cursor.len() != (2 + 8 + 32 + 8) {
        return Err(TimbreError::MalformedCursor);
    }

    let address_idx = u16::from_be_bytes(cursor[0..2].try_into().unwrap());
    let slot = u64::from_be_bytes(cursor[2..10].try_into().unwrap());
    let u_hash = cursor[10..42].try_into().unwrap();
    let u_index = u64::from_be_bytes(cursor[42..50].try_into().unwrap());

    Ok(UtxosByAddressesCursor {
        address_idx,
        slot,
        u_hash,
        u_index,
    })
}

pub fn decode_policy_holders_cursor(b64_cursor: &String) -> Result<Address, TimbreError> {
    let cursor = b64::URL_SAFE_NO_PAD
        .decode(b64_cursor)
        .map_err(|_| TimbreError::MalformedCursor)?;

    Address::from_bytes(&cursor).map_err(|_| TimbreError::MalformedCursor)
}

pub fn decode_txs_by_address_cursor(
    b64_cursor: &String,
) -> Result<TxsByAddressCursor, TimbreError> {
    let cursor = b64::URL_SAFE_NO_PAD
        .decode(b64_cursor)
        .map_err(|_| TimbreError::MalformedCursor)?;

    if cursor.len() != (8 + 2 + 32) {
        return Err(TimbreError::MalformedCursor);
    }

    let slot = u64::from_be_bytes(cursor[0..8].try_into().unwrap());
    let block_index = u16::from_be_bytes(cursor[8..10].try_into().unwrap());
    let tx_hash = cursor[10..42].try_into().unwrap();

    Ok(TxsByAddressCursor {
        slot,
        tx_hash,
        block_index,
    })
}

fn decode_address_with_type(bytes: &[u8]) -> (DecodedAddress, usize) {
    let (addr, addr_len) = match bytes[0] {
        ADDRESS_SHELLEY => {
            let (shelley, len) = decode_shelley_address(&bytes[1..]);
            (DecodedAddress::Address(shelley.into()), len)
        }
        ADDRESS_BYRON => decode_byron_address(&bytes[1..]),
        _ => unreachable!("unexpected address type"),
    };

    // addr_len + 1 to include the type byte
    (addr, addr_len + 1)
}

/// Fallible variant of [`decode_shelley_address`] for bytes that originate
/// outside the database — e.g. user-supplied pagination cursors. Returns an
/// error on truncated input or unknown payment/delegation kinds instead of
/// panicking.
pub fn try_decode_shelley_address(bytes: &[u8]) -> Result<(ShelleyAddress, usize), TimbreError> {
    if bytes.len() < 32 {
        return Err(TimbreError::MalformedCursor);
    }

    let network = match bytes[0] {
        0 => Network::Testnet,
        1 => Network::Mainnet,
        x => Network::Other(x),
    };

    let payment_hash: [u8; 28] = bytes[2..30].try_into().unwrap();
    let payment = match bytes[1] {
        0 => ShelleyPaymentPart::Key(payment_hash.into()),
        1 => ShelleyPaymentPart::Script(payment_hash.into()),
        _ => return Err(TimbreError::MalformedCursor),
    };

    let (delegation, consumed) = match bytes[31] {
        0 | 1 => {
            let stake_hash: [u8; 28] = bytes
                .get(32..60)
                .ok_or(TimbreError::MalformedCursor)?
                .try_into()
                .unwrap();

            let part = if bytes[31] == 0 {
                ShelleyDelegationPart::Key(stake_hash.into())
            } else {
                ShelleyDelegationPart::Script(stake_hash.into())
            };

            (part, 60)
        }
        2 => {
            let raw = bytes.get(32..56).ok_or(TimbreError::MalformedCursor)?;
            let slot = u64::from_be_bytes(raw[0..8].try_into().unwrap());
            let tx_idx = u64::from_be_bytes(raw[8..16].try_into().unwrap());
            let cert_idx = u64::from_be_bytes(raw[16..24].try_into().unwrap());

            (
                ShelleyDelegationPart::Pointer(Pointer::new(slot, tx_idx, cert_idx)),
                56,
            )
        }
        3 => (ShelleyDelegationPart::Null, 32),
        _ => return Err(TimbreError::MalformedCursor),
    };

    Ok((ShelleyAddress::new(network, payment, delegation), consumed))
}

pub fn decode_shelley_address(bytes: &[u8]) -> (ShelleyAddress, usize) {
    let network = match bytes[0] {
        0 => Network::Testnet,
        1 => Network::Mainnet,
        x => Network::Other(x),
    };

    let payment_hash: [u8; 28] = bytes[2..30].try_into().unwrap();
    let payment = match bytes[1] {
        0 => ShelleyPaymentPart::Key(payment_hash.into()),
        1 => ShelleyPaymentPart::Script(payment_hash.into()),
        _ => unreachable!("unexpected shelley payment part kind"),
    };

    let _break = bytes[30];

    let (delegation, consumed) = match bytes[31] {
        0 => {
            let stake_hash: [u8; 28] = bytes[32..60].try_into().unwrap();

            (ShelleyDelegationPart::Key(stake_hash.into()), 60)
        }
        1 => {
            let stake_hash: [u8; 28] = bytes[32..60].try_into().unwrap();

            (ShelleyDelegationPart::Script(stake_hash.into()), 60)
        }
        2 => {
            let slot = u64::from_be_bytes(bytes[32..40].try_into().unwrap());
            let tx_idx = u64::from_be_bytes(bytes[40..48].try_into().unwrap());
            let cert_idx = u64::from_be_bytes(bytes[48..56].try_into().unwrap());

            let pointer = Pointer::new(slot, tx_idx, cert_idx);

            (ShelleyDelegationPart::Pointer(pointer), 56)
        }
        3 => (ShelleyDelegationPart::Null, 32),
        _ => unreachable!("unexpected shelley deleg part kind"),
    };

    let address = ShelleyAddress::new(network, payment, delegation);

    (address, consumed)
}

fn decode_byron_address(bytes: &[u8]) -> (DecodedAddress, usize) {
    match bytes[0] {
        0 => {
            let address_bytes_len = u16::from_be_bytes(bytes[1..3].try_into().unwrap());
            let address = ByronAddress::from_bytes(&bytes[3..(3 + address_bytes_len as usize)])
                .unwrap()
                .into();

            (
                DecodedAddress::Address(address),
                1 + 2 + address_bytes_len as usize,
            )
        }
        1 => {
            let address_bytes_hash: [u8; 32] = bytes[1..33].try_into().unwrap();

            (DecodedAddress::Hash(address_bytes_hash), 1 + 32)
        }
        _ => unreachable!("unexpected byron address encoding"),
    }
}

pub fn decode_short_bytestring(bytes: &[u8]) -> (&[u8], usize) {
    let len = bytes[0] as usize;

    let out = &bytes[1..(1 + len)];

    (out, len + 1)
}

pub fn decode_utxos_by_asset_value(bytes: &[u8]) -> (DecodedAddress, u64) {
    let mut cursor = 0;
    let (address, addr_len) = decode_address_with_type(bytes);
    cursor += addr_len;

    let amount = u64::from_be_bytes(bytes[cursor..cursor + 8].try_into().unwrap());

    (address, amount)
}

pub fn decode_utxos_by_policy_value(bytes: &[u8]) -> (DecodedAddress, Vec<(Vec<u8>, u64)>) {
    let mut cursor = 0;
    let (address, addr_len) = decode_address_with_type(bytes);
    cursor += addr_len;

    let mut assets = Vec::new();

    loop {
        if cursor == bytes.len() {
            return (address, assets);
        } else {
            let amount = u64::from_be_bytes(bytes[cursor..cursor + 8].try_into().unwrap());
            cursor += 8;

            let (name, name_len) = decode_short_bytestring(&bytes[cursor..]);
            cursor += name_len;

            assets.push((name.to_vec(), amount))
        }
    }
}

pub struct BlockByHeightValue {
    pub hash: [u8; 32],
    pub header_bytes: Vec<u8>,
    pub tx_hashes: Vec<[u8; 32]>,
    pub size: u32,
    pub output: u128,
    pub fees: u64,
    pub invocations: u32,
    pub mem: u64,
    pub steps: u64,
}

pub fn decode_block_by_height_value(bytes: &[u8]) -> BlockByHeightValue {
    let mut c = 0; // cursor

    let hash: [u8; 32] = bytes[0..32].try_into().unwrap();
    c += 32;

    let tx_count = u16::from_be_bytes(bytes[c..c + 2].try_into().unwrap());
    c += 2;

    let mut tx_hashes = Vec::with_capacity(tx_count as usize);
    for _ in 0..tx_count {
        tx_hashes.push(bytes[c..c + 32].try_into().unwrap());
        c += 32;
    }

    let size = u32::from_be_bytes(bytes[c..c + 4].try_into().unwrap());
    c += 4;

    let output = u128::from_be_bytes(bytes[c..c + 16].try_into().unwrap());
    c += 16;

    let fees = u64::from_be_bytes(bytes[c..c + 8].try_into().unwrap());
    c += 8;

    let invocations = u32::from_be_bytes(bytes[c..c + 4].try_into().unwrap());
    c += 4;

    let mem = u64::from_be_bytes(bytes[c..c + 8].try_into().unwrap());
    c += 8;

    let steps = u64::from_be_bytes(bytes[c..c + 8].try_into().unwrap());
    c += 8;

    let header_bytes = bytes[c..].to_vec();

    BlockByHeightValue {
        hash,
        header_bytes,
        tx_hashes,
        size,
        output,
        fees,
        invocations,
        mem,
        steps,
    }
}

pub struct TxsByAddressValue {
    pub input: bool,
    pub output: bool,
}

pub fn decode_txs_by_address_value(byte: u8) -> TxsByAddressValue {
    let input = (byte & 0b1000_0000) != 0;
    let output = (byte & 0b0100_0000) != 0;

    TxsByAddressValue { input, output }
}
