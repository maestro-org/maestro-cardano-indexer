use core::fmt;
use std::{
    collections::HashMap,
    ops::{Deref, Range},
    str::FromStr,
    time::Duration,
};

use axum::{
    http::{HeaderMap, StatusCode},
    Json,
};
use bech32::ToBase32;
use chrono::{DateTime, TimeZone, Utc};
use cryptoxide::{blake2b::Blake2b, digest::Digest};
use itertools::Itertools;
use pallas::codec::minicbor;
use pallas::ledger::traverse::{ComputeHash, MultiEraOutput, MultiEraTx};
use pallas::ledger::{
    addresses::{Address, ShelleyPaymentPart},
    traverse::wellknown::GenesisValues,
};
use pallas::ledger::{
    primitives::{
        babbage::{
            AssetName, BigInt, Constr, DatumOption, Metadatum, NativeScript, PlutusData, PolicyId,
        },
        conway::ScriptRef,
        Fragment, ToCanonicalJson,
    },
    traverse::OriginalHash,
};
use pallas::network::miniprotocols::Point;
use serde::{de, Deserialize, Deserializer};
use serde_json::{json, Map, Value};
use sqlx::{postgres::PgRow, types::BigDecimal, PgPool, Row};
use strum::IntoEnumIterator;
use tikv_client::{KvPair, Snapshot};
use timbre::{
    encoding::{
        decode::{
            decode_cursor_value, decode_datum_by_hash_key, decode_key, decode_tx_by_hash_key,
            decode_utxos_by_asset_value, decode_utxos_by_policy_value, DecodedAddress,
        },
        decode_cip25_metadata_by_asset_key, decode_script_by_hash_value,
        encode::KeyEncoder,
        RangeBound, Slot, TimbreError,
    },
    Key, ReducerKey, UtxosByAssetKey, UtxosByPolicyKey,
};
use tracing::warn;

use crate::{
    chain::ChainInfo,
    responses::{
        self, Cip68AssetType, Cip68Metadata, ErrorBody, ErrorResponse, LastUpdated,
        MapiDecodeError, NumOrString, PoolMetaJson, ScriptFirstSeen, ScriptType,
        TimestampedTransaction,
    },
    utils, CursorPagination, OrderParam, PolyphonyWrapper, SlotPagination,
};

pub static DEFAULT_MAX_ITEMS_PER_PAGE: usize = 100;
pub static MAX_ITEMS_PER_SCAN: usize = 300;
pub static MORE_MAX_ITEMS_PER_SCAN: usize = 300;
pub static MAX_CHECKED_SCAN_RETRIES: u32 = 5;

pub static AMOUNT_AS_STR_HEADER: &str = "amounts-as-strings";

pub fn error_response(status: StatusCode, message: &str) -> ErrorResponse {
    let body = ErrorBody {
        code: status.as_u16(),
        error: status.canonical_reason().unwrap().into(),
        message: message.into(),
    };

    (status, Json(body))
}

pub fn internal_server_error(e: impl std::error::Error) -> ErrorResponse {
    tracing::error!("returning 500: {e}");

    let status = StatusCode::INTERNAL_SERVER_ERROR;

    let body = ErrorBody {
        code: status.as_u16(),
        error: status.canonical_reason().unwrap().into(),
        message: "Server-side error was encountered, please contact the operator".into(),
    };

    (status, Json(body))
}

pub fn internal_server_error_str(reason: &str) -> ErrorResponse {
    tracing::error!("returning 500: {reason}");

    let status = StatusCode::INTERNAL_SERVER_ERROR;

    let body = ErrorBody {
        code: status.as_u16(),
        error: status.canonical_reason().unwrap().into(),
        message: "Server-side error was encountered, please contact the operator".into(),
    };

    (status, Json(body))
}

pub fn timeout() -> ErrorResponse {
    tracing::warn!("returning 504");

    let status = StatusCode::GATEWAY_TIMEOUT;

    let body = ErrorBody {
        code: status.as_u16(),
        error: status.canonical_reason().unwrap().into(),
        message: "Request timed out".into(),
    };

    (status, Json(body))
}

pub fn service_unavailable() -> ErrorResponse {
    tracing::warn!("returning 503");

    error_response(
        StatusCode::SERVICE_UNAVAILABLE,
        "Service temporarily unavailable, please retry",
    )
}

pub fn map_ogmios_error(e: crate::ogmios_v6::OgmiosError) -> ErrorResponse {
    use crate::ogmios_v6::OgmiosError;

    match e {
        OgmiosError::Timeout => {
            tracing::error!("returning 504: ogmios query timed out");
            timeout()
        }
        OgmiosError::Transient(msg) => {
            tracing::error!("returning 503: ogmios transient error: {msg}");
            service_unavailable()
        }
        OgmiosError::JsonRpc { code, message } => {
            tracing::error!("returning 503: ogmios json-rpc error {code}: {message}");
            service_unavailable()
        }
        OgmiosError::Decode(msg) => {
            internal_server_error_str(&format!("ogmios decode error: {msg}"))
        }
    }
}

pub fn bad_request(message: &str) -> ErrorResponse {
    tracing::debug!("returning 400: {message}");

    let status = StatusCode::BAD_REQUEST;

    let body = ErrorBody {
        code: status.as_u16(),
        error: status.canonical_reason().unwrap().into(),
        message: message.into(),
    };

    (status, Json(body))
}

pub fn not_found() -> ErrorResponse {
    tracing::debug!("returning 404");

    let status = StatusCode::NOT_FOUND;

    let body = ErrorBody {
        code: status.as_u16(),
        error: status.canonical_reason().unwrap().into(),
        message: "Could not find the requested resource".into(),
    };

    (status, Json(body))
}

// dbsync deserialiser helper funcs

fn string_to_lovelace(str: String) -> Result<u64, sqlx::Error> {
    str.parse()
        .map_err(|_| sqlx::Error::Decode(anyhow::Error::new(MapiDecodeError).into()))
}

pub fn get_maybe_lovelace_from_string(
    row: &PgRow,
    column_name: &str,
) -> Result<Option<u64>, sqlx::Error> {
    row.try_get::<Option<String>, _>(column_name)?
        .map(string_to_lovelace)
        .transpose()
}

pub fn get_lovelace_from_string(row: &PgRow, column_name: &str) -> Result<u64, sqlx::Error> {
    let string_val = row.try_get::<String, _>(column_name)?;

    string_to_lovelace(string_val)
}

pub fn get_negative_lovelace_from_string(
    row: &PgRow,
    column_name: &str,
) -> Result<i64, sqlx::Error> {
    let string_val = row.try_get::<String, _>(column_name)?;

    string_to_negative_lovelace(string_val)
}

pub fn string_to_negative_lovelace(str: String) -> Result<i64, sqlx::Error> {
    str.parse()
        .map_err(|_| sqlx::Error::Decode(anyhow::Error::new(MapiDecodeError).into()))
}

pub fn get_maybe_string_from_numeric(
    row: &PgRow,
    column_name: &str,
) -> Result<Option<String>, sqlx::Error> {
    Ok(row
        .try_get::<Option<BigDecimal>, _>(column_name)?
        .map(|bd| bd.to_string()))
}

pub fn get_string_from_numeric(row: &PgRow, column_name: &str) -> Result<String, sqlx::Error> {
    let bd_val = row.try_get::<BigDecimal, _>(column_name)?;

    Ok(bd_val.to_string())
}

pub fn de_i64_from_str<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: Deserializer<'de>,
{
    match Value::deserialize(deserializer)? {
        Value::Number(n) => Ok(n.as_i64().unwrap()),
        Value::String(s) => s.parse().map_err(de::Error::custom),
        _ => todo!(),
    }
}

pub fn de_u64_from_str<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    match Value::deserialize(deserializer)? {
        Value::Number(n) => Ok(n.as_u64().unwrap()),
        Value::String(s) => s.parse().map_err(de::Error::custom),
        _ => Err(de::Error::custom("unexpected type")),
    }
}

pub fn get_and_unwrap_json_array<'a, T: Clone + Deserialize<'a> + 'a>(
    row: &'a PgRow,
    column_name: &str,
) -> Result<Vec<T>, sqlx::Error> {
    let vec: Vec<T> = row
        .try_get::<sqlx::types::Json<Vec<T>>, _>(column_name)?
        .deref()
        .to_vec();

    Ok(vec)
}

pub fn deserialise_u64_string<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    let s: &str = Deserialize::deserialize(deserializer)?;

    s.parse::<u64>().map_err(de::Error::custom)
}

pub fn u64_str_conv(amount: u64, headers: &HeaderMap) -> NumOrString {
    match headers.get(AMOUNT_AS_STR_HEADER) {
        Some(x) => match x.to_str() {
            Ok("true") => NumOrString::String(amount.to_string()),
            _ => NumOrString::U64(amount),
        },
        None => NumOrString::U64(amount),
    }
}

pub fn u128_str_conv(amount: u128, headers: &HeaderMap) -> NumOrString {
    match headers.get(AMOUNT_AS_STR_HEADER) {
        Some(x) => match x.to_str() {
            Ok("true") => NumOrString::String(amount.to_string()),
            _ => NumOrString::U128(amount),
        },
        None => NumOrString::U128(amount),
    }
}

pub fn i64_str_conv(amount: i64, headers: &HeaderMap) -> NumOrString {
    match headers.get(AMOUNT_AS_STR_HEADER) {
        Some(x) => match x.to_str() {
            Ok("true") => NumOrString::String(amount.to_string()),
            _ => NumOrString::I64(amount),
        },
        None => NumOrString::I64(amount),
    }
}

pub fn f64_str_conv(amount: f64, headers: &HeaderMap) -> NumOrString {
    match headers.get(AMOUNT_AS_STR_HEADER) {
        Some(x) => match x.to_str() {
            Ok("true") => NumOrString::String(amount.to_string()),
            _ => NumOrString::F64(amount),
        },
        None => NumOrString::F64(amount),
    }
}

/// Serde deserialization decorator to map empty Strings to None
pub fn empty_string_as_none<'de, D, T>(de: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: FromStr,
    T::Err: fmt::Display,
{
    let opt = Option::<String>::deserialize(de)?;
    match opt.as_deref() {
        None | Some("") => Ok(None),
        Some(s) => FromStr::from_str(s).map_err(de::Error::custom).map(Some),
    }
}

pub fn assets(output: &MultiEraOutput, headers: &HeaderMap) -> Vec<responses::Asset> {
    let mut assets = Vec::new();

    assets.push(responses::Asset {
        unit: "lovelace".into(),
        amount: utils::u64_str_conv(output.value().coin(), headers),
    });

    for policy_assets in output.value().assets() {
        let policy_hex = hex::encode(policy_assets.policy());
        for asset in policy_assets.assets() {
            let asset_hex = hex::encode(asset.name());

            assets.push(responses::Asset {
                unit: format!("{policy_hex}{asset_hex}"),
                amount: utils::u64_str_conv(asset.output_coin().unwrap(), headers),
            })
        }
    }

    assets
}

/// Given a TxOut build a Mapi datum object, and optionally store the hash for
/// later in case we want to resolve many datum hashes at once
pub fn datum_option(
    txout: &MultiEraOutput<'_>,
    resolved_dhs: Option<&HashMap<[u8; 32], Vec<u8>>>,
) -> Option<responses::DatumOption> {
    let datum = match txout.datum() {
        None => None,
        Some(DatumOption::Hash(dh)) => {
            let mut bytes = None;
            let mut json = None;

            // try resolve datum hash if we have resolved dh map
            if let Some(resolved) = resolved_dhs {
                if let Some(data) = resolved.get(&*dh) {
                    // minicbor::decode fails on empty bytestring
                    if !data.is_empty() {
                        // On-chain datum bytes: always surface the raw CBOR; only
                        // add the decoded JSON when it parses, rather than panicking.
                        bytes = Some(hex::encode(data));
                        if let Ok(pd) = minicbor::decode::<PlutusData>(data) {
                            json = Some(pd.to_json());
                        }
                    } else {
                        bytes = Some("".into());
                        json = Some(json!(null));
                    }
                }
            }

            Some(responses::DatumOption {
                datum_type: responses::DatumOptionType::Hash,
                hash: dh.to_string(),
                bytes,
                json,
            })
        }
        Some(DatumOption::Data(d)) => Some(responses::DatumOption {
            datum_type: responses::DatumOptionType::Inline,
            hash: d.original_hash().to_string(),
            bytes: Some(hex::encode(d.raw_cbor())),
            json: Some(d.to_json()),
        }),
    };

    datum
}

/// Given a TxOut, build a Mapi reference script object.
pub fn reference_script(txout: &MultiEraOutput<'_>) -> Option<responses::Script> {
    match txout.script_ref() {
        None => None,
        Some(sr) => match &sr {
            ScriptRef::NativeScript(s) => Some(responses::Script {
                script_type: responses::ScriptType::Native,
                hash: s.original_hash().to_string(),
                bytes: hex::encode(s.raw_cbor()),
                json: Some(s.to_json()),
            }),
            ScriptRef::PlutusV1Script(s) => Some(responses::Script {
                script_type: responses::ScriptType::PlutusV1,
                hash: s.compute_hash().to_string(),
                bytes: hex::encode(s),
                json: None,
            }),
            ScriptRef::PlutusV2Script(s) => Some(responses::Script {
                script_type: responses::ScriptType::PlutusV2,
                hash: s.compute_hash().to_string(),
                bytes: hex::encode(s),
                json: None,
            }),
            ScriptRef::PlutusV3Script(s) => Some(responses::Script {
                script_type: responses::ScriptType::PlutusV3,
                hash: s.compute_hash().to_string(),
                bytes: hex::encode(s),
                json: None,
            }),
        },
    }
}

/// Given a JSON object of transaction metadata at key 721, try parse the
/// metadata according to CIP-25 for the given policy and asset name.
pub fn try_parse_cip25_metadata(
    md_721: &serde_json::Value,
    policy: [u8; 28],
    asset_name: &Vec<u8>,
) -> Option<serde_json::Value> {
    let mut md = None;

    let policy_hex = hex::encode(policy);
    let asset_name_hex = hex::encode(asset_name);

    if let Some(ver) = md_721.get("version") {
        if *ver == json!(1) || *ver == json!("1") || *ver == json!("1.0") {
            if let Some(policy_md) = md_721.get(policy_hex) {
                if let Some(asset_md) =
                    policy_md.get(String::from_utf8_lossy(asset_name).to_string())
                {
                    md = Some(asset_md.clone());
                }
            }
        } else if *ver == json!(2) || *ver == json!("2") || *ver == json!("2.0") {
            if let Some(policy_md) = md_721.get(format!("0x{policy_hex}")) {
                if let Some(asset_md) = policy_md.get(format!("0x{asset_name_hex}")) {
                    md = Some(asset_md.clone());
                }
            }
        } else {
            return None;
        }
    } else if let Some(policy_md) = md_721.get(policy_hex) {
        if let Some(asset_md) = policy_md.get(String::from_utf8_lossy(asset_name).to_string()) {
            md = Some(asset_md.clone());
        }
    }

    // try join up "image" field if it is split into array
    if let Some(ref mut j) = md {
        if let Some(image) = j.get_mut("image") {
            if let Some(a) = image.as_array_mut() {
                let mut outstring = String::new();

                for chunk in a {
                    if let Some(s) = chunk.as_str() {
                        outstring.push_str(s);
                    } else {
                        return None;
                    }
                }

                *image = json!(outstring);
            }
        }
    }

    md
}

/// Given an asset name, try fetch the CIP68 metadata for the
/// specified policy and asset.
pub async fn try_parse_cip68_metadata_from_policy_utxos(
    polyphony: &PolyphonyWrapper,
    policy_utxo_kvs: &Vec<KvPair>,
    asset_name: Vec<u8>,
) -> Result<Option<Cip68Metadata>, ErrorResponse> {
    if asset_name.len() < 4 {
        return Ok(None);
    };

    // check that the tag is either 100 or 222 or the FT one
    let token_type = match &asset_name[..4] {
        [0x00, 0x06, 0x43, 0xb0] => Cip68AssetType::ReferenceNft, // 100
        [0x00, 0x0d, 0xe1, 0x40] => Cip68AssetType::UserNft,      // 222
        [0x00, 0x14, 0xdf, 0x10] => Cip68AssetType::UserFt,       // 333
        _ => return Ok(None),
    };

    // use scrolls to fetch the UTxO of the reference NFT by crafting the
    // reference nft asset name if we don't already have that
    let reference_nft_name = match token_type {
        Cip68AssetType::ReferenceNft => asset_name,
        _ => {
            let mut buf = vec![0x00, 0x06, 0x43, 0xb0];
            buf.extend_from_slice(&asset_name[4..]);

            buf
        }
    };

    // --- fetch cursor for last updated ---

    // TODO ?
    // let last_updated = get_last_updated(&mut txn, &encoder).await?;

    // --- scan all utxos for this policy

    let mut utxos = Vec::new();

    for KvPair(k, v) in policy_utxo_kvs {
        let decoded_key = decode_key(&Into::<Vec<u8>>::into(k.clone()));

        let (tx_hash, index) =
            if let Key::Reducer((_, _, ReducerKey::PolicyUtxos(key @ UtxosByPolicyKey { .. }))) =
                decoded_key
            {
                (key.utxo_hash, key.utxo_index)
            } else {
                return Err(internal_server_error_str(
                    format!("unexpected key scanned: {:?}", decoded_key).as_str(),
                ));
            };

        let (_address, assets_list) = decode_utxos_by_policy_value(v);

        // check if desired asset in policy utxo
        for (name, amount) in assets_list {
            if name == reference_nft_name {
                utxos.push((tx_hash, index));

                if utxos.len() == 2 {
                    warn!("multiple reference NFTs utxos found for cip68 asset");
                    return Ok(None);
                } else if amount > 1 {
                    warn!("multiple reference NFTs found in utxo for cip68 asset");
                    return Ok(None);
                } else {
                    break;
                }
            }
        }
    }

    if utxos.is_empty() {
        warn!("no reference NFT found for cip68 asset");
        return Ok(None);
    }

    let (tx_hash, index) = utxos[0];

    // -- fetch the tx bytes for the reference nft output

    let (cbor, _) = polyphony.get_tx_bytes(&hex::encode(tx_hash)).await?;

    let tx = MultiEraTx::decode(&cbor).map_err(internal_server_error)?;

    let txout = tx.produces_at(index as usize).ok_or_else(not_found)?;

    let datum_data = match txout.datum() {
        Some(DatumOption::Data(d)) => Some(d.deref().deref().clone()),
        Some(DatumOption::Hash(h)) => tx.find_plutus_data(&h).map(|x| x.deref().clone()),
        None => return Ok(None),
    };

    let datum_data = match datum_data {
        Some(d) => d,
        None => return Ok(None),
    };

    let fields = match datum_data {
        PlutusData::Constr(Constr {
            tag: 121, fields, ..
        }) => fields,
        _ => return Ok(None),
    };

    let pd_metadata = match fields.first() {
        Some(PlutusData::Map(m)) => m.clone().to_vec(),
        _ => return Ok(None),
    };

    let version: u64 = match fields.get(1) {
        Some(PlutusData::BigInt(BigInt::Int(v))) => match (i128::from(*v.deref())).try_into() {
            Ok(v_u64) => v_u64,
            Err(_) => return Ok(None),
        },
        _ => return Ok(None),
    };

    let extra = fields.get(2);

    // convert metadata KV maps into
    let mut serde_map = serde_json::Map::new();
    // mark if we see fields "name", "image", and "description" to check
    // metadata conforms to the standards
    let mut name_seen = false;
    let mut image_seen = false;
    let mut description_seen = false;

    for (k, v) in pd_metadata {
        let (key, encode_hex) = match k {
            PlutusData::BoundedBytes(b) => {
                let x: Vec<u8> = b.into();

                // represent as utf8 if possible, otherwise hex
                match std::str::from_utf8(&x) {
                    Ok(key_str) => (key_str.to_string(), false),
                    Err(_) => (hex::encode(x), true),
                }
            }
            _ => {
                return Ok(None);
            }
        };

        match key.as_str() {
            "name" => name_seen = true,
            "image" => image_seen = true,
            "description" => description_seen = true,
            _ => (),
        }

        let value = match plutus_data_to_json(encode_hex, v) {
            Some(json_val) => json_val,
            None => {
                return Ok(None);
            }
        };

        if serde_map.insert(key, value).is_some() {
            return Ok(None);
        }
    }

    // 222 NFT standard requires "name" and "image" fields in metadata
    // 333 FT standard requires "name" and "description" fields in metadata
    match token_type {
        Cip68AssetType::UserNft if !(name_seen && image_seen) => return Ok(None),
        Cip68AssetType::UserFt if !(name_seen && description_seen) => return Ok(None),
        _ => (),
    }

    let cip68 = Cip68Metadata {
        purpose: token_type,
        version,
        metadata: json!(serde_map),
        extra: extra.map(|x| hex::encode(x.encode_fragment().unwrap())),
    };

    Ok(Some(cip68))
}

/// Given an asset name, try fetch the CIP68 metadata for the
/// specified policy and asset.
pub async fn try_fetch_cip68_metadata(
    polyphony: &PolyphonyWrapper,
    policy: [u8; 28],
    asset_name: Vec<u8>,
) -> Result<Option<Cip68Metadata>, ErrorResponse> {
    if asset_name.len() < 4 {
        return Ok(None);
    };

    // check that the tag is either 100 or 222 or the FT one
    let token_type = match &asset_name[..4] {
        [0x00, 0x06, 0x43, 0xb0] => Cip68AssetType::ReferenceNft, // 100
        [0x00, 0x0d, 0xe1, 0x40] => Cip68AssetType::UserNft,      // 222
        [0x00, 0x14, 0xdf, 0x10] => Cip68AssetType::UserFt,       // 333
        _ => return Ok(None),
    };

    // use scrolls to fetch the UTxO of the reference NFT by crafting the
    // reference nft asset name if we don't already have that
    let reference_nft_name = match token_type {
        Cip68AssetType::ReferenceNft => asset_name,
        _ => {
            let mut buf = vec![0x00, 0x06, 0x43, 0xb0];
            buf.extend_from_slice(&asset_name[4..]);

            buf
        }
    };

    // --- scan all utxos for this policy

    let encoder = polyphony.utxos_by_policy_encoder()?;
    let mut txn = polyphony.begin_snapshot_latest().await?;
    let range = encoder.encode_policy_utxos_range(&policy.into(), None, None);

    let kvs = utils::batch_scan_entire_range(&mut txn, range).await?;

    let mut utxos = Vec::new();

    for KvPair(k, v) in kvs {
        let decoded_key = decode_key(&Into::<Vec<u8>>::into(k));

        let (tx_hash, index) =
            if let Key::Reducer((_, _, ReducerKey::PolicyUtxos(key @ UtxosByPolicyKey { .. }))) =
                decoded_key
            {
                (key.utxo_hash, key.utxo_index)
            } else {
                return Err(internal_server_error_str(
                    format!("unexpected key scanned: {:?}", decoded_key).as_str(),
                ));
            };

        let (_address, assets_list) = decode_utxos_by_policy_value(&v);

        // check if desired asset in policy utxo
        for (name, amount) in assets_list {
            if name == reference_nft_name {
                utxos.push((tx_hash, index));

                if utxos.len() == 2 {
                    warn!("multiple reference NFTs utxos found for cip68 asset");
                    return Ok(None);
                } else if amount > 1 {
                    warn!("multiple reference NFTs found in utxo for cip68 asset");
                    return Ok(None);
                } else {
                    break;
                }
            }
        }
    }

    if utxos.is_empty() {
        warn!("no reference NFT found for cip68 asset");
        return Ok(None);
    }

    let (tx_hash, index) = utxos[0];

    // -- fetch the tx bytes for the reference nft output

    let (cbor, _) = polyphony.get_tx_bytes(&hex::encode(tx_hash)).await?;

    let tx = MultiEraTx::decode(&cbor).map_err(internal_server_error)?;

    let txout = tx.produces_at(index as usize).ok_or_else(not_found)?;

    let datum_data = match txout.datum() {
        Some(DatumOption::Data(d)) => Some(d.deref().deref().clone()),
        Some(DatumOption::Hash(h)) => tx.find_plutus_data(&h).map(|x| x.deref().clone()),
        None => return Ok(None),
    };

    let datum_data = match datum_data {
        Some(d) => d,
        None => return Ok(None),
    };

    let fields = match datum_data {
        PlutusData::Constr(Constr {
            tag: 121, fields, ..
        }) => fields,
        _ => return Ok(None),
    };

    let pd_metadata = match fields.first() {
        Some(PlutusData::Map(m)) => m.clone().to_vec(),
        _ => return Ok(None),
    };

    let version: u64 = match fields.get(1) {
        Some(PlutusData::BigInt(BigInt::Int(v))) => match (i128::from(*v.deref())).try_into() {
            Ok(v_u64) => v_u64,
            Err(_) => return Ok(None),
        },
        _ => return Ok(None),
    };

    let extra = fields.get(2);

    // convert metadata KV maps into
    let mut serde_map = serde_json::Map::new();
    // mark if we see fields "name", "image", and "description" to check
    // metadata conforms to the standards
    let mut name_seen = false;
    let mut image_seen = false;
    let mut description_seen = false;

    for (k, v) in pd_metadata {
        let (key, encode_hex) = match k {
            PlutusData::BoundedBytes(b) => {
                let x: Vec<u8> = b.into();

                // represent as utf8 if possible, otherwise hex
                match std::str::from_utf8(&x) {
                    Ok(key_str) => (key_str.to_string(), false),
                    Err(_) => (hex::encode(x), true),
                }
            }
            _ => return Ok(None),
        };

        match key.as_str() {
            "name" => name_seen = true,
            "image" => image_seen = true,
            "description" => description_seen = true,
            _ => (),
        }

        let value = match plutus_data_to_json(encode_hex, v) {
            Some(json_val) => json_val,
            None => return Ok(None),
        };

        if serde_map.insert(key, value).is_some() {
            return Ok(None);
        }
    }

    // 222 NFT standard requires "name" and "image" fields in metadata
    // 333 FT standard requires "name" and "description" fields in metadata
    match token_type {
        Cip68AssetType::UserNft if !(name_seen && image_seen) => return Ok(None),
        Cip68AssetType::UserFt if !(name_seen && description_seen) => return Ok(None),
        _ => (),
    }

    let cip68 = Cip68Metadata {
        purpose: token_type,
        version,
        metadata: json!(serde_map),
        extra: extra.map(|x| hex::encode(x.encode_fragment().unwrap())),
    };

    Ok(Some(cip68))
}

fn plutus_data_to_json(encode_hex: bool, pd: PlutusData) -> Option<serde_json::Value> {
    match pd {
        PlutusData::Array(a) => {
            let xs: Vec<Option<serde_json::Value>> = a
                .iter()
                .map(|pd| plutus_data_to_json(encode_hex, pd.clone()))
                .collect();

            if xs.iter().any(|x| x.is_none()) {
                return None;
            }

            Some(json!(xs))
        }
        PlutusData::Map(m) => {
            let kvs = m.to_vec();
            let mut serde_map = serde_json::Map::new();

            for (k, v) in kvs {
                let (key, encode_hex) = match k {
                    PlutusData::BoundedBytes(b) => {
                        let x: Vec<u8> = b.into();

                        // represent as utf8 if possible, otherwise hex
                        match std::str::from_utf8(&x) {
                            Ok(key_str) => (key_str.to_string(), false),
                            Err(_) => (hex::encode(x), true),
                        }
                    }
                    _ => return None,
                };

                let value = plutus_data_to_json(encode_hex, v)?;

                if serde_map.insert(key, value).is_some() {
                    tracing::warn!("duplicate keys in pd json map");
                    return None;
                };
            }

            Some(json!(serde_map))
        }
        PlutusData::BigInt(BigInt::Int(i)) => Some(json!(i.to_string())),
        PlutusData::BoundedBytes(b) => {
            let x: Vec<u8> = b.into();

            if encode_hex {
                Some(json!(hex::encode(x)))
            } else {
                // represent as utf8 if possible, otherwise hex
                match std::str::from_utf8(&x) {
                    Ok(val_str) => Some(json!(val_str.to_string())),
                    Err(_) => Some(json!(hex::encode(x))),
                }
            }
        }
        // TODO: This does not retain original encoding (indef / definite).
        PlutusData::Constr(c) => Some(json!(hex::encode(c.encode_fragment().unwrap()))),
        PlutusData::BigInt(BigInt::BigUInt(i)) => {
            Some(json!(hex::encode(i.encode_fragment().unwrap())))
        }
        PlutusData::BigInt(BigInt::BigNInt(i)) => {
            Some(json!(hex::encode(i.encode_fragment().unwrap())))
        }
    }
}

/// Represents a UTxO which contains an asset which should only ever exist in a
/// single UTxO (for example an NFT or asset whose amount is limited to 1, but
/// potentially others where multiple of the asset must always be in the same
/// UTxO)
pub struct UniqueAssetUtxo {
    pub utxo_hash: [u8; 32],
    pub utxo_index: u64,
    pub address: Address,
    pub amount: u64,
}

/// Return the single UTxO which contains a given asset. Returns Error if more
/// than 1 UTxO found, None if no UTxO found.
pub async fn fetch_single_utxo_containing_asset(
    txn: &mut Snapshot,
    encoder: &KeyEncoder,
    policy: &PolicyId,
    asset: &AssetName,
) -> Result<Option<UniqueAssetUtxo>, ErrorResponse> {
    let range = encoder.encode_asset_utxos_range(policy, asset, None, None);

    let kvs: Vec<KvPair> = txn
        .scan(range, 2) // if we find 2 utxos for the asset then we need to error
        .await
        .map_err(internal_server_error)?
        .collect();

    if kvs.len() == 2 {
        return Err(internal_server_error_str(
            format!(
                "multiple UTxOs for asset {} {:?}",
                hex::encode(asset.clone().to_vec()),
                kvs
            )
            .as_str(),
        ));
    }

    if let Some(KvPair(k, v)) = kvs.first() {
        let decoded_key = decode_key(&Into::<Vec<u8>>::into(k.clone()));

        if let Key::Reducer((
            _,
            _,
            ReducerKey::AssetUtxos(UtxosByAssetKey {
                utxo_hash,
                utxo_index,
                ..
            }),
        )) = decoded_key
        {
            let (address, amount) = decode_utxos_by_asset_value(v);

            let address = match address {
                DecodedAddress::Address(a) => a,
                // TODO
                DecodedAddress::Hash(h) => {
                    return Err(internal_server_error_str(
                        format!("decoded address hash: {}", hex::encode(h)).as_str(),
                    ))
                }
            };

            Ok(Some(UniqueAssetUtxo {
                utxo_hash,
                utxo_index,
                address,
                amount,
            }))
        } else {
            Err(internal_server_error_str(
                format!("unexpected key scanned: {:?}", decoded_key).as_str(),
            ))
        }
    } else {
        Ok(None)
    }
}

pub async fn fetch_cip25_metadata(
    txn: &mut Snapshot,
    polyphony: &PolyphonyWrapper,
    assets: Vec<([u8; 28], Vec<u8>)>,
) -> Result<HashMap<([u8; 28], Vec<u8>), Value>, ErrorResponse> {
    let encoder = polyphony.cip25_metadata_by_asset_encoder()?;

    let required_keys: Vec<Vec<u8>> = assets
        .into_iter()
        .map(|(p, n)| encoder.encode_cip25_metadata_by_asset_key(&(p.into()), n))
        .collect();

    let mut acc = HashMap::new();

    for key_batch in required_keys.chunks(500) {
        let kvs: HashMap<([u8; 28], Vec<u8>), _> = txn
            .batch_get(key_batch.to_vec())
            .await
            .map_err(internal_server_error)?
            .map(|KvPair(k, v)| {
                let decoded_key = decode_cip25_metadata_by_asset_key((&k).into());

                (
                    (decoded_key.policy, decoded_key.name),
                    Metadatum::decode_fragment(&v).unwrap(),
                )
            })
            .collect();

        acc.extend(kvs)
    }

    let mut acc_2 = HashMap::new();

    for ((policy, name), md_721) in acc {
        let md_721_json = match metadatum_to_json(md_721) {
            Some(md) => md,
            None => continue,
        };

        if let Some(cip25_md) = try_parse_cip25_metadata(&md_721_json, policy, &name) {
            acc_2.insert((policy, name), cip25_md);
        }
    }

    Ok(acc_2)
}

/// Returns None if any keys used in a Map are Map or Array
pub fn metadatum_to_json(md: Metadatum) -> Option<Value> {
    match md {
        Metadatum::Array(xs) => {
            let mut out = vec![];

            for x in xs {
                out.push(metadatum_to_json(x)?)
            }

            Some(Value::Array(out))
        }
        Metadatum::Bytes(bs) => Some(Value::String(hex::encode(bs.to_vec()))),
        Metadatum::Int(i) => Some(json!(i128::from(i))),
        Metadatum::Text(s) => Some(Value::String(s)),
        Metadatum::Map(kvs) => {
            let mut out = Map::new();

            for (k, v) in kvs.to_vec() {
                let k_string = metadatum_to_string(k)?;
                let v_json = metadatum_to_json(v)?;

                out.insert(k_string, v_json);
            }

            Some(Value::Object(out))
        }
    }
}

fn metadatum_to_string(md: Metadatum) -> Option<String> {
    match md {
        Metadatum::Text(t) => Some(t),
        Metadatum::Int(i) => Some(i128::from(i).to_string()),
        Metadatum::Bytes(b) => Some(format!("0x{}", hex::encode(b.to_vec()))),
        Metadatum::Array(_) => None,
        Metadatum::Map(_) => None,
    }
}

pub async fn fetch_datums(
    txn: &mut Snapshot,
    encoder: &KeyEncoder,
    hashes: Vec<[u8; 32]>,
) -> Result<HashMap<[u8; 32], Vec<u8>>, ErrorResponse> {
    let required_keys: Vec<Vec<u8>> = hashes
        .iter()
        .map(|h| encoder.encode_datum_by_hash_key(h))
        .collect();

    let mut acc = HashMap::new();

    for key_batch in required_keys.chunks(500) {
        let kvs: HashMap<[u8; 32], Vec<u8>> = txn
            .batch_get(key_batch.to_vec())
            .await
            .map_err(internal_server_error)?
            .map(|KvPair(k, v)| (decode_datum_by_hash_key((&k).into()).datum_hash, v))
            .collect();

        acc.extend(kvs)
    }

    // currently we store an empty byte string as the value in the situation
    // where the datum hash was seen on chain BUT the corresponding bytes have
    // not been seen - this leads to a problem as it means we can't distinguish
    // the datum hash which actually corresponds to the empty byte string from
    // datum hashes for which we have not seen the corresponding bytes. for that
    // reason we remove all entries with empty byte strings unless the datum
    // hash is the hash of the empty byte string, hardcoded below. not sure if
    // empty byte string is even an accepted datum on-chain though.

    let hash_for_empty_bs: [u8; 32] = [
        0x0e, 0x57, 0x51, 0xc0, 0x26, 0xe5, 0x43, 0xb2, 0xe8, 0xab, 0x2e, 0xb0, 0x60, 0x99, 0xda,
        0xa1, 0xd1, 0xe5, 0xdf, 0x47, 0x77, 0x8f, 0x77, 0x87, 0xfa, 0xab, 0x45, 0xcd, 0xf1, 0x2f,
        0xe3, 0xa8,
    ];

    let modified = acc
        .into_iter()
        .filter(|(h, b)| {
            if b.is_empty() {
                h == &hash_for_empty_bs
            } else {
                true
            }
        })
        .collect();

    Ok(modified)
}

pub async fn fetch_txos(
    txn: &mut Snapshot,
    polyphony: &PolyphonyWrapper,
    txo_refs: Vec<([u8; 32], u64)>,
) -> Result<HashMap<([u8; 32], u64), Vec<u8>>, ErrorResponse> {
    let encoder = polyphony.tx_by_hash_encoder()?;

    let required_keys: Vec<Vec<u8>> = txo_refs
        .iter()
        .map(|(h, _)| encoder.encode_tx_by_hash_key(h))
        .collect();

    let mut acc = HashMap::new();

    for key_batch in required_keys.chunks(500) {
        let kvs: HashMap<[u8; 32], Vec<u8>> = txn
            .batch_get(key_batch.to_vec())
            .await
            .map_err(internal_server_error)?
            .map(|KvPair(k, v)| (decode_tx_by_hash_key((&k).into()).tx_hash, v))
            .collect();

        acc.extend(kvs)
    }

    let mut out = HashMap::new();

    for (tx_hash, txo_index) in txo_refs {
        let cbor = acc
            .get(&tx_hash)
            .ok_or_else(|| internal_server_error_str("missing tx"))?;

        let tx = MultiEraTx::decode(cbor).map_err(internal_server_error)?;

        let txout = tx
            .produces_at(txo_index as usize)
            .ok_or_else(|| internal_server_error_str("missing txo"))?;

        out.insert((tx_hash, txo_index), txout.encode());
    }

    Ok(out)
}

pub async fn fetch_script(
    txn: &mut Snapshot,
    polyphony: &PolyphonyWrapper,
    chain: &ChainInfo,
    hash: [u8; 28],
) -> Result<ScriptFirstSeen, ErrorResponse> {
    let key = polyphony
        .script_by_hash_encoder()?
        .encode_script_by_hash_key(&hash);

    let bytes = txn
        .get(key)
        .await
        .map_err(internal_server_error)?
        .ok_or_else(not_found)?;

    let script = decode_script_by_hash_value(&bytes);

    let script_type = match script.kind {
        0 => ScriptType::Native,
        1 => ScriptType::PlutusV1,
        2 => ScriptType::PlutusV2,
        3 => ScriptType::PlutusV3,
        _ => return Err(internal_server_error_str("unrecognised script type")),
    };

    let json = if script_type == ScriptType::Native {
        let ns: NativeScript = minicbor::decode(&script.bytes).map_err(internal_server_error)?;

        Some(ns.to_json())
    } else {
        None
    };

    let timestamp = chain.slot_to_utc(script.slot);

    let first_seen = TimestampedTransaction {
        tx_hash: hex::encode(script.tx_hash),
        slot: script.slot,
        timestamp,
    };

    Ok(ScriptFirstSeen {
        hash: hex::encode(hash),
        script_type,
        bytes: hex::encode(script.bytes),
        json,
        first_seen,
    })
}

pub fn asset_fingerprint(policy: [u8; 28], name: &[u8]) -> String {
    let mut digest = [0u8; 20];
    let mut context = Blake2b::new(20);
    context.input(&policy);
    context.input(name);
    context.result(&mut digest);

    bech32::encode("asset", digest.to_base32(), bech32::Variant::Bech32).unwrap()
}

pub struct PaginateParams {
    page: usize,
    max: usize,
}

impl PaginateParams {
    pub fn new(page: Option<usize>, max: Option<usize>) -> Result<Self, ErrorResponse> {
        // max count cannot be more than 100
        if let Some(m) = max {
            if m > 100 {
                return Err(bad_request("Page result count must not exceed 100"));
            }
        };

        // page must be at least 1
        if let Some(p) = page {
            if p < 1 {
                return Err(bad_request("Page number must be greater than 0"));
            }
        };

        Ok(Self {
            page: page.unwrap_or(1) - 1, // page 1 is index 0
            max: max.unwrap_or(DEFAULT_MAX_ITEMS_PER_PAGE),
        })
    }

    pub fn page(&self) -> usize {
        self.page
    }

    pub fn max(&self) -> usize {
        self.max
    }

    /// Max number of results needed to satisfy pagination request (i.e, page 1
    /// with max results per page 100 requires 100 results to satisfy)
    pub fn needed(&self) -> usize {
        (self.page() + 1) * self.max()
    }
}

/// Given a vector of items, a optional page number (default 1, must be at least
/// 1), and an optional custom maximum number of items per page (default 100),
/// split the items into pages and return the page of the given page number.
/// Page number is one-indexed, so we subtract one in the call to `nth`.
pub fn paginate<T>(items: Vec<T>, pageparams: PaginateParams) -> Vec<T> {
    items
        .into_iter()
        .chunks(pageparams.max)
        .into_iter()
        .map(|chunk| chunk.collect::<Vec<T>>())
        .nth(pageparams.page)
        .unwrap_or_default()
}

pub struct ParsedSlotPageParams<C> {
    pub count: usize,
    pub order: OrderParam,
    pub lower: Option<RangeBound<C>>,
    pub upper: Option<RangeBound<C>>,
}

/// Generic over Cursors which contain a slot
/// TODO: TimbreResult, EndpointResult
pub fn parse_slot_page_params<C: Slot>(
    params: SlotPagination,
    cursor_decoder: fn(&String) -> Result<C, TimbreError>,
) -> Result<ParsedSlotPageParams<C>, ErrorResponse> {
    let count = match params.count {
        Some(max) if max.0 > DEFAULT_MAX_ITEMS_PER_PAGE => {
            return Err(bad_request("Result count parameter too large"))
        }
        Some(max) if max.0 == 0 => {
            return Err(bad_request("Result count parameter must be greater than 0"))
        }
        Some(max) => max.0,
        None => DEFAULT_MAX_ITEMS_PER_PAGE,
    };

    let order = params.order.unwrap_or(OrderParam::Asc);

    let cursor = if let Some(s) = &params.cursor {
        match cursor_decoder(s) {
            Ok(c) => Some(c),
            Err(_) => return Err(bad_request("Malformed cursor")),
        }
    } else {
        None
    };

    // check that cursor lies within slot range or error.
    let mut lower = params.from.map(|s| RangeBound::Slot(s));
    let mut upper = params.to.map(|s| RangeBound::Slot(s));

    // check if lower param greater than upper, if both provided
    if let (Some(l), Some(u)) = (params.from, params.to) {
        if l > u {
            return Err(bad_request(
                "Range parameter 'from' must not be greater than parameter 'to'",
            ));
        }
    }

    // if a cursor is provided, the cursor slot must be within the parameter ranges
    // generic on cursors which have a slot
    if let Some(c) = cursor {
        if let Some(l) = params.from {
            if c.slot() < l {
                return Err(bad_request(
                    "Cursor outside of slot range parameters (lower)",
                ));
            }
        }

        if let Some(u) = params.to {
            if c.slot() > u {
                return Err(bad_request(
                    "Cursor outside of slot range parameters (upper)",
                ));
            }
        }

        // if the order is ascending, the cursor is the lower bound
        // if the order is descending, the cursor is the upper bound
        match order {
            OrderParam::Asc => lower = Some(RangeBound::Cursor(c)),
            OrderParam::Desc => upper = Some(RangeBound::Cursor(c)),
        }
    }

    Ok(ParsedSlotPageParams {
        count,
        order,
        lower,
        upper,
    })
}

pub struct ParsedCursorPageParams<C> {
    pub count: usize,
    pub cursor: Option<C>,
}

/// TODO: TimbreResult, EndpointResult
pub fn parse_cursor_page_params<C, E>(
    params: CursorPagination,
    cursor_decoder: fn(&String) -> Result<C, E>,
) -> Result<ParsedCursorPageParams<C>, ErrorResponse> {
    let count = match params.count {
        Some(max) if max.0 > DEFAULT_MAX_ITEMS_PER_PAGE => {
            return Err(bad_request("Result count parameter too large"))
        }
        Some(max) if max.0 == 0 => {
            return Err(bad_request("Result count parameter must be greater than 0"))
        }
        Some(max) => max.0,
        None => DEFAULT_MAX_ITEMS_PER_PAGE,
    };

    let cursor = if let Some(s) = &params.cursor {
        match cursor_decoder(s) {
            Ok(c) => Some(c),
            Err(_) => return Err(bad_request("Malformed cursor")),
        }
    } else {
        None
    };

    Ok(ParsedCursorPageParams { count, cursor })
}

pub fn decode_payment_address(addr_str: &str) -> Result<Address, ErrorResponse> {
    let address =
        Address::from_str(addr_str).map_err(|_| bad_request("Could not decode address"))?;

    if matches!(address, Address::Stake(_)) {
        return Err(bad_request(
            "This endpoint does not support reward addresses",
        ));
    }

    Ok(address)
}

pub fn decode_payment_credential(credential: &str) -> Result<ShelleyPaymentPart, ErrorResponse> {
    let cred =
        match decode_bech32(credential) {
            Ok((hrp, bytes)) => {
                let hash: [u8; 28] = bytes
                    .get(..28)
                    .and_then(|b| b.try_into().ok())
                    .ok_or_else(|| bad_request("Could not decode credential, invalid length"))?;

                match hrp.as_str() {
                    "addr_vkh" => ShelleyPaymentPart::Key(hash.into()),
                    "addr_shared_vkh" | "script" => ShelleyPaymentPart::Script(hash.into()),
                    _ => return Err(bad_request(
                        "Credential should begin with 'addr_vkh', 'addr_shared_vkh' or 'script'",
                    )),
                }
            }
            Err(_) => return Err(bad_request("Could not decode credential, must be bech32")),
        };

    Ok(cred)
}

pub fn decode_policy_id(hex_policy: &String) -> Result<PolicyId, ErrorResponse> {
    let policy_bytes: [u8; 28] = match hex::decode(hex_policy) {
        Ok(b) => b
            .try_into()
            .map_err(|_| bad_request("Malformed policy ID"))?,
        Err(_) => return Err(bad_request("Policy ID must be hex encoded")),
    };

    let policy: PolicyId = policy_bytes.into();

    Ok(policy)
}

pub fn decode_asset(hex_policy_and_name: &String) -> Result<(PolicyId, AssetName), ErrorResponse> {
    let decoded_concat = hex::decode(hex_policy_and_name).map_err(|_| bad_request("Invalid encoding of asset parameter: asset must be concatenation of hex encoded policy ID and asset name"))?;

    if decoded_concat.len() < 28 || decoded_concat.len() > (28 + 32) {
        return Err(bad_request("Invalid length of asset parameter: asset must be concatenation of hex encoded policy ID and asset name"));
    }

    let (policy_bytes, name) = decoded_concat.split_at(28);

    let policy: PolicyId = TryInto::<[u8; 28]>::try_into(policy_bytes).unwrap().into();

    Ok((policy, name.to_vec().into()))
}

pub enum BlockId {
    Hash([u8; 32]),
    Height(u64),
}

pub fn decode_block_hash_or_height(hash_or_height: &String) -> Result<BlockId, ErrorResponse> {
    if hash_or_height.len() == 64 {
        let decoded_hash = hex::decode(hash_or_height)
            .map_err(|_| bad_request("Malformed block hash, invalid hex"))?
            .try_into()
            .map_err(|_| bad_request("Malformed block hash, invalid length"))?;

        Ok(BlockId::Hash(decoded_hash))
    } else {
        let decoded_height = hash_or_height
            .parse()
            .map_err(|_| bad_request("Malformed block height"))?;

        Ok(BlockId::Height(decoded_height))
    }
}

pub async fn get_last_updated(
    txn: &mut Snapshot,
    encoder: &KeyEncoder,
    chain: &ChainInfo,
) -> Result<LastUpdated, ErrorResponse> {
    let cursor_bytes = txn
        .get(encoder.encode_cursor_key())
        .await
        .map_err(internal_server_error)?
        .ok_or_else(|| internal_server_error_str("cursor key not found in db"))?;

    let (point, _height) = decode_cursor_value(&cursor_bytes);

    let (slot, block_hash) = match point {
        Point::Origin => (0, hex::encode([0; 32])),
        Point::Specific(s, b) => (s, hex::encode(b)),
    };

    Ok(LastUpdated::new(chain, block_hash, slot))
}

pub async fn get_last_updated_with_height(
    txn: &mut Snapshot,
    encoder: &KeyEncoder,
    chain: &ChainInfo,
) -> Result<(LastUpdated, u64), ErrorResponse> {
    let cursor_bytes = txn
        .get(encoder.encode_cursor_key())
        .await
        .map_err(internal_server_error)?
        .ok_or_else(|| internal_server_error_str("cursor key not found in db"))?;

    let (point, height) = decode_cursor_value(&cursor_bytes);

    let (slot, block_hash) = match point {
        Point::Origin => (0, hex::encode([0; 32])),
        Point::Specific(s, b) => (s, hex::encode(b)),
    };

    Ok((LastUpdated::new(chain, block_hash, slot), height))
}

/// Scan all the entries in a range within a tx, which halves scan size if error
/// encountered
pub async fn batch_scan_entire_range(
    txn: &mut Snapshot,
    range: Range<Vec<u8>>,
) -> Result<Vec<KvPair>, ErrorResponse> {
    let mut scan_size = 1000;
    let mut range_remaining = range;
    let mut kvs = Vec::new();

    loop {
        match txn.scan(range_remaining.clone(), scan_size).await {
            Ok(kvs_iter) => {
                let kvs_vec: Vec<KvPair> = kvs_iter.collect();

                kvs.extend(kvs_vec.clone());

                if (kvs_vec.len() as u32) < scan_size {
                    break;
                } else {
                    let mut last_key =
                        Into::<Vec<u8>>::into(kvs_vec.last().unwrap().clone().into_key());

                    last_key.push(0);
                    range_remaining = last_key..range_remaining.end;
                }
            }
            Err(tikv_client::Error::Grpc(e))
                if e.to_string().contains("Received message larger than max") =>
            {
                warn!("scan size {scan_size} too large when fetching entire range {range_remaining:?}");
                scan_size /= 2
            }
            Err(e) => {
                return Err(internal_server_error_str(
                    format!("Error when scanning entire range: {:?}", e).as_str(),
                ))
            }
        }
    }

    Ok(kvs)
}

/// Scan a range, optionally filtering results with `filter`, stopping
/// once we have `max_count` results (if provided) or the range is exhausted
pub async fn scan_extended<F>(
    txn: &mut Snapshot,
    range: Range<Vec<u8>>,
    order: OrderParam,
    f: Option<F>,
    max_count: Option<usize>,
    timeout: Option<Duration>,
) -> Result<Vec<KvPair>, ErrorResponse>
where
    F: FnMut(KvPair) -> bool + std::marker::Copy,
{
    // if we have no filter, we only need to scan max count
    let mut scan_size = match (f, max_count) {
        (None, Some(n)) if n < 1000 => n as u32,
        _ => 1000,
    };
    let mut range_remaining = range;
    let mut kvs = Vec::new();

    let start_instant = tokio::time::Instant::now();

    loop {
        if let Some(duration) = timeout {
            if start_instant.elapsed() > duration {
                return Err(utils::timeout());
            }
        }

        let fetched = match order {
            OrderParam::Asc => txn
                .scan(range_remaining.clone(), scan_size)
                .await
                .map(|iter| iter.collect::<Vec<KvPair>>()),
            OrderParam::Desc => txn
                .scan_reverse(range_remaining.clone(), scan_size)
                .await
                .map(|iter| iter.collect::<Vec<KvPair>>()),
        };

        match fetched {
            Ok(kvs_vec) => {
                let num_kvs_fetched = kvs_vec.len();

                // use the last key fetched to update the remaining range, or
                // break if no keys were fetched
                range_remaining = if let Some(kv) = kvs_vec.last() {
                    let mut last_key = Into::<Vec<u8>>::into(kv.clone().into_key());

                    // modify the range according to the order
                    match order {
                        OrderParam::Asc => {
                            last_key.push(0);
                            last_key..range_remaining.end
                        }
                        OrderParam::Desc => range_remaining.start..last_key,
                    }
                } else {
                    break;
                };

                // if a filter was provided filter the fetched kvs and add the
                // filtered results to the output vec, otherwise add all the kvs
                if let Some(mut filter) = f {
                    let filtered = kvs_vec
                        .into_iter()
                        .filter(|x| filter(x.clone()))
                        .collect::<Vec<KvPair>>();
                    kvs.extend(filtered)
                } else {
                    kvs.extend(kvs_vec)
                };

                // if max_count was provided and we have enough kvs to satisfy
                // that count then stop scanning
                if let Some(max) = max_count {
                    if kvs.len() >= max {
                        break;
                    }
                }

                // if the number of total fetched (unfiltered) kvs is less than
                // the scan size then we have exhausted the scan range, so stop
                if (num_kvs_fetched as u32) < scan_size {
                    break;
                }
            }
            Err(tikv_client::Error::Grpc(e))
                if e.to_string().contains("Received message larger than max") =>
            {
                warn!("scan size {scan_size} too large when fetching entire range {range_remaining:?}");
                scan_size /= 2
            }
            Err(e) => {
                return Err(internal_server_error_str(
                    format!("Error when scanning entire range: {:?}", e).as_str(),
                ))
            }
        }
    }

    if let Some(max) = max_count {
        kvs.truncate(max)
    }

    Ok(kvs)
}

// TODO: Different approach
pub async fn fetch_pool_metadata(url: String, hash: String) -> Option<PoolMetaJson> {
    let hash: [u8; 32] = match hex::decode(hash) {
        Ok(d) => match d.try_into() {
            Ok(x) => x,
            Err(e) => {
                warn!("unable to decode metadata hash: {e:?}");
                return None;
            }
        },
        Err(e) => {
            warn!("unable to decode metadata hex hash: {e:?}");
            return None;
        }
    };

    // handle URLs with no base specified (try http)
    let out = if url.starts_with("http") {
        url
    } else {
        format!("http://{}", url)
    };

    // The metadata URL is pool-operator-controlled: bound both the request time
    // and the response size so a hostile or broken server cannot stall or bloat us.
    const MAX_POOL_METADATA_BYTES: usize = 1024 * 1024; // 1 MiB

    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            warn!("unable to build metadata http client: {e:?}");
            return None;
        }
    };

    let response = client.get(out).send().await;

    let text = match response {
        Ok(mut r) => {
            if r.content_length()
                .is_some_and(|len| len > MAX_POOL_METADATA_BYTES as u64)
            {
                warn!("metadata response exceeds size limit");
                return None;
            }

            let mut buf: Vec<u8> = Vec::new();
            loop {
                match r.chunk().await {
                    Ok(Some(chunk)) => {
                        if buf.len() + chunk.len() > MAX_POOL_METADATA_BYTES {
                            warn!("metadata response exceeds size limit");
                            return None;
                        }
                        buf.extend_from_slice(&chunk);
                    }
                    Ok(None) => break,
                    Err(e) => {
                        warn!("unable to parse metadata text: {e:?}");
                        return None;
                    }
                }
            }

            match String::from_utf8(buf) {
                Ok(t) => t,
                Err(e) => {
                    warn!("unable to parse metadata text: {e:?}");
                    return None;
                }
            }
        }
        Err(e) => {
            warn!("unable to parse metadata url: {e:?}");
            return None;
        }
    };

    let computed_hash = {
        let mut digest = [0u8; 32];
        let mut context = Blake2b::new(32);
        context.input(text.as_bytes());
        context.result(&mut digest);

        digest
    };

    if hash != computed_hash {
        warn!("metadata computed hash did not match");
        return None;
    }

    match serde_json::from_str(&text) {
        Ok(md) => Some(md),
        Err(e) => {
            warn!("unable to parse md json: {e:?}");
            None
        }
    }
}

fn decode_bech32(bech32: &str) -> Result<(String, Vec<u8>), bech32::Error> {
    let (hrp, addr, _) = bech32::decode(bech32)?;
    let base10 = bech32::FromBase32::from_base32(&addr)?;
    Ok((hrp, base10))
}

pub fn generic_iterator<E, F>(pred: F)
where
    E: IntoEnumIterator,
    F: Fn(E),
{
    for e in E::iter() {
        pred(e)
    }
}

pub async fn get_last_updated_dbsync(
    chain: &ChainInfo,
    dbsync: &PgPool,
) -> Result<LastUpdated, ErrorResponse> {
    let lu_row: (String, i64) = sqlx::query_as(
        "SELECT ENCODE(hash, 'hex'), slot_no
        FROM block
        WHERE block_no IS NOT NULL
        ORDER BY block_no DESC
        LIMIT 1",
    )
    .fetch_one(dbsync)
    .await
    .map_err(internal_server_error)?;

    Ok(LastUpdated::new(chain, lu_row.0, lu_row.1 as u64))
}

pub fn slot_to_timestamp_str(genesis: &GenesisValues, abs_slot: u64) -> String {
    let unix_ts = genesis.slot_to_wallclock(abs_slot);
    let datetime: DateTime<Utc> = Utc.timestamp_opt(unix_ts.try_into().unwrap(), 0).unwrap();
    datetime.format("%Y-%m-%d %H:%M:%S").to_string()
}

#[cfg(test)]
mod tests {
    use crate::ogmios_v6::OgmiosError;
    use crate::utils::{map_ogmios_error, service_unavailable, PaginateParams};
    use axum::http::StatusCode;

    use super::paginate;

    #[test]
    fn paginate_pages_correct() {
        let items: Vec<u8> = (0..5).collect();

        assert_eq!(
            paginate(
                items.clone(),
                PaginateParams::new(Some(1), Some(2)).unwrap()
            ),
            vec![0, 1]
        );
        assert_eq!(
            paginate(
                items.clone(),
                PaginateParams::new(Some(2), Some(2)).unwrap()
            ),
            vec![2, 3]
        );
        assert_eq!(
            paginate(
                items.clone(),
                PaginateParams::new(Some(3), Some(2)).unwrap()
            ),
            vec![4]
        );
        assert_eq!(
            paginate(items, PaginateParams::new(Some(4), Some(2)).unwrap()),
            Vec::<u8>::new()
        );
    }

    #[test]
    fn service_unavailable_is_503() {
        let (status, _body) = service_unavailable();
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn maps_transient_to_503() {
        let (status, _) = map_ogmios_error(OgmiosError::Transient("x".into()));
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn maps_jsonrpc_to_503() {
        let (status, _) = map_ogmios_error(OgmiosError::JsonRpc {
            code: -1,
            message: "m".into(),
        });
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn maps_timeout_to_504() {
        let (status, _) = map_ogmios_error(OgmiosError::Timeout);
        assert_eq!(status, StatusCode::GATEWAY_TIMEOUT);
    }

    #[test]
    fn maps_decode_to_500() {
        let (status, _) = map_ogmios_error(OgmiosError::Decode("d".into()));
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn paginate_exactly_all_items() {
        let items: Vec<u8> = (0..10).collect();
        let mut found = Vec::new();

        let mut i = 1;

        loop {
            match paginate(
                items.clone(),
                PaginateParams::new(Some(i), Some(2)).unwrap(),
            ) {
                xs if xs.is_empty() => break,
                xs => {
                    found.extend(xs);
                    i += 1
                }
            }
        }

        assert_eq!(items, found)
    }

    #[test]
    fn paginate_less_items_than_max() {
        let items: Vec<u8> = (0..10).collect();

        assert_eq!(
            paginate(
                items.clone(),
                PaginateParams::new(Some(1), Some(20)).unwrap()
            ),
            items
        )
    }
}
