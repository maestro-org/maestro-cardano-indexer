use crate::{
    responses::{ErrorResponse, PaginatedResponse, UtxoWithSlot},
    utils::{
        self, assets, datum_option, internal_server_error, internal_server_error_str,
        reference_script, scan_extended, ParsedSlotPageParams,
    },
    CountParam, MapiExtension, OrderParam, SlotPagination,
};
use axum::{
    extract::{Path, Query},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Extension, Json,
};
use pallas::ledger::addresses::Address;
use pallas::ledger::primitives::babbage::DatumOption;
use pallas::ledger::traverse::{Era, MultiEraOutput};
use serde::Deserialize;
use tikv_client::KvPair;
use timbre::encoding::{
    decode::{decode_utxos_by_address_key, DecodedAddress},
    decode_utxos_by_payment_cred_cursor, encode_utxos_by_payment_cred_cursor, RangeBound,
};

use std::cmp::Ordering;

use super::address_utxo_contains_asset;

#[derive(Debug, Deserialize)]
pub struct Params {
    pub resolve_datums: Option<bool>,
    pub with_cbor: Option<bool>,
    pub asset: Option<String>,
}

#[utoipa::path(
    tag = "Addresses",
    get,
    path = "/addresses/cred/{credential}/utxos",
    params(
        ("credential" = String, Path, description = "Payment credential in bech32 format"),

        ("asset" = inline(Option<String>), Query, description = "Return only UTxOs which contain some of a specific asset (asset formatted as concatenation of hex encoded policy and asset name)"),

        ("resolve_datums" = Option<bool>, Query, description = "Try find and include the corresponding datums for datum hashes"),
        ("with_cbor" = Option<bool>, Query, description = "Include the CBOR encodings of the transaction outputs in the response"),

        ("count" = inline(Option<CountParam>), Query, description = "The max number of results per page"),

        ("order" = inline(Option<OrderParam>), Query, description = "The order in which the results are sorted (by slot at which UTxO was produced)"),
        ("from" = inline(Option<u64>), Query, description = "Return only UTxOs created on or after a specific slot"),
        ("to" = inline(Option<u64>), Query, description = "Return only UTxOs created on or before a specific slot"),

        ("cursor" = inline(Option<String>), Query, description = "Pagination cursor string, use the cursor included in a page of results to fetch the next page"),

        ("amounts-as-strings" = Option<String>, Header, description = "Large numbers returned as strings if set to `true`")
    ),
    responses(
        (
            status = 200,
            description = "Get all unspent transaction outputs at addresses with the given payment credential",
            body = PaginatedUtxoWithSlot,
            example = json!({
                "data": [
                    {
                        "tx_hash": "b24743fb4b5381971ee8b3d02a3fd5e783acdb5a21cd2d22d192ec84e7e279fc",
                        "index": 0,
                        "slot": 23100140,
                        "assets": [
                            {
                                "unit": "lovelace",
                                "amount": 1224040
                            },
                            {
                                "unit": "0c64d6d0371d11185aae649cf3a169040e94214137137b531ebb16c2446a65644f7261636c654e4654",
                                "amount": 1
                            }
                        ],
                        "address": "addr_test1wzdtu0djc76qyqak9cj239udezj2544nyk3ksmfqvaksv7c9xanpg",
                        "datum": {
                            "type": "hash",
                            "hash": "ac0017f05bec9c7d1a475283b89fc40630ecdd66544fc3c0506be5f7c790d79e",
                            "bytes": null,
                            "json": null
                        },
                        "reference_script": null,
                        "txout_cbor": null
                    },
                    {
                        "tx_hash": "e2e91c7be8239b9f52fadec8df627fe759d21e5b6559c3ec06d8120f1366e075",
                        "index": 0,
                        "slot": 32264892,
                        "assets": [
                            {
                                "unit": "lovelace",
                                "amount": 1224040
                            },
                            {
                                "unit": "0c64d6d0371d11185aae649cf3a169040e94214137137b531ebb16c2446a65644f7261636c654e4654",
                                "amount": 1
                            }
                        ],
                        "address": "addr_test1wzdtu0djc76qyqak9cj239udezj2544nyk3ksmfqvaksv7c9xanpg",
                        "datum": {
                            "type": "hash",
                            "hash": "bdea76b7527bf513874be137b040b3235033d6d5af0add15d6e0dcfc78859aed",
                            "bytes": null,
                            "json": null
                        },
                        "reference_script": null,
                        "txout_cbor": null
                    }
                ],
                "last_updated": {
                    "timestamp": "2022-10-10 20:25:28",
                    "block_hash": "eb5008ee0990527fb0d27d16b8c6284d4c898112509948d4741a7c41e544b912",
                    "block_slot": 32265653
                },
                "next_cursor": null
            })
        ),
        (status = 400, description = "Malformed query parameters"),
        (status = 500, description = "Internal server error"),
    )
)]
#[tracing::instrument(name = "UTXOS_BY_PAYMENT_CRED", level = "info", skip(config, headers))]
/// UTxOs by payment credential
///
/// Return detailed information on UTxOs controlled by addresses which use the specified payment credential
pub async fn utxos_by_payment_cred(
    headers: HeaderMap,
    page_params: Query<SlotPagination>,
    params: Query<Params>,
    Path(credential): Path<String>,
    Extension(config): MapiExtension,
) -> Result<impl IntoResponse, ErrorResponse> {
    let polyphony = config.polyphony_wrapper;
    let chain = config.chain_info;
    let timeout = config.timeout_duration;

    let encoder = polyphony.utxo_cbor_by_address_encoder()?;

    // -- parse endpoint specific params

    let resolve_datums = params.resolve_datums.unwrap_or(false);
    let with_cbor = params.with_cbor.unwrap_or(false);

    // -- parse and try decode user params

    let payment_cred = utils::decode_payment_credential(&credential)?;

    let filter_asset = params.asset.as_ref().map(utils::decode_asset).transpose()?;

    let ParsedSlotPageParams {
        count,
        order,
        lower,
        upper,
    } = utils::parse_slot_page_params(page_params.0, decode_utxos_by_payment_cred_cursor)?;

    // --- start db at most recent timestamp

    let mut txn = polyphony.begin_snapshot_latest().await?;

    // --- fetch cursor for last updated ---

    let last_updated = utils::get_last_updated(&mut txn, &encoder, &chain).await?;

    // --- scan keys for this page

    let range =
        encoder.encode_utxos_by_payment_cred_range(chain.network_id(), &payment_cred, None, None);

    let kvs = if let Some((policy, name)) = filter_asset {
        scan_extended(
            &mut txn,
            range,
            order,
            Some(|KvPair(_, v)| address_utxo_contains_asset(v, &policy, &name)),
            None,
            Some(timeout),
        )
        .await?
    } else {
        scan_extended(&mut txn, range, order, None::<fn(_) -> bool>, None, None).await?
    };

    // --- process fetched keys into response data

    let mut datum_hashes = Vec::new();
    let mut txouts = Vec::new(); // this will contain all txouts for the cred

    for KvPair(k, txo_bytes) in kvs.iter() {
        let key = decode_utxos_by_address_key(k.into());

        let (addr, slot, tx_hash, index) = (key.address, key.slot, key.utxo_hash, key.utxo_index);

        // TODO
        let addr = match addr {
            DecodedAddress::Address(Address::Shelley(a)) => a,
            DecodedAddress::Address(a) => {
                return Err(internal_server_error_str(
                    format!("decoded unexpected address: {:?}", a).as_str(),
                ))
            }
            DecodedAddress::Hash(h) => {
                return Err(internal_server_error_str(
                    format!("decoded address hash: {}", hex::encode(h)).as_str(),
                ))
            }
        };

        let txo = MultiEraOutput::decode(Era::Conway, txo_bytes)
            .or_else(|_| MultiEraOutput::decode(Era::Babbage, txo_bytes))
            .or_else(|_| MultiEraOutput::decode(Era::Alonzo, txo_bytes))
            .or_else(|_| MultiEraOutput::decode(Era::Byron, txo_bytes))
            .map_err(internal_server_error)?;

        // note the dh so we can batch resolve it if needed
        if let Some(DatumOption::Hash(dh)) = txo.datum() {
            datum_hashes.push(*dh)
        }

        txouts.push((tx_hash, index, slot, txo, addr));
    }

    // first sort ascending by slot and then tx hash then index
    txouts.sort_by(|a, b| {
        if a.2 == b.2 {
            if a.0 == b.0 {
                a.1.cmp(&b.1)
            } else {
                a.0.to_vec().cmp(&b.0.to_vec())
            }
        } else {
            a.2.cmp(&b.2)
        }
    });

    if let Some(bound) = lower {
        match bound {
            RangeBound::Slot(s) => txouts.retain(|x| x.2 >= s),
            RangeBound::Cursor(c) => txouts.retain(|x| match x.2.cmp(&c.slot) {
                Ordering::Less => false,
                Ordering::Equal => {
                    if x.0.to_vec() < c.u_hash.to_vec() {
                        false
                    } else if x.0 == c.u_hash {
                        x.1 > c.u_index
                    } else {
                        true
                    }
                }
                Ordering::Greater => true,
            }),
        }
    }

    if let Some(bound) = upper {
        match bound {
            RangeBound::Slot(s) => txouts.retain(|x| x.2 <= s),
            RangeBound::Cursor(c) => txouts.retain(|x| match x.2.cmp(&c.slot) {
                Ordering::Greater => false,
                Ordering::Equal => {
                    if x.0.to_vec() > c.u_hash.to_vec() {
                        false
                    } else if x.0 == c.u_hash {
                        x.1 < c.u_index
                    } else {
                        true
                    }
                }
                Ordering::Less => true,
            }),
        }
    }

    if let OrderParam::Desc = order {
        txouts = txouts.into_iter().rev().collect()
    }

    let next_cursor = if txouts.len() > count {
        let last_result = &txouts[count - 1];

        Some(encode_utxos_by_payment_cred_cursor(
            &last_result.4,
            last_result.2,
            last_result.0,
            last_result.1,
        ))
    } else {
        None
    };

    // only return only the page size of results
    txouts.truncate(count);

    // --- create datum hash to datum map if resolving hashes

    let resolved_dhs = resolve_datums.then_some(
        utils::fetch_datums(&mut txn, &polyphony.datum_by_hash_encoder()?, datum_hashes).await?,
    );

    // --- craft response data

    let utxos = txouts
        .into_iter()
        .map(|(tx_hash, index, slot, txo, addr)| {
            let assets = assets(&txo, &headers);
            let datum = datum_option(&txo, resolved_dhs.as_ref());
            let reference_script = reference_script(&txo);

            let txout_cbor = with_cbor.then_some(hex::encode(txo.encode()));

            UtxoWithSlot {
                tx_hash: hex::encode(tx_hash),
                index: index as usize,
                slot,
                address: Address::from(addr).to_string(),
                assets,
                datum,
                reference_script,
                txout_cbor,
            }
        })
        .collect();

    // ---

    let out = PaginatedResponse {
        data: utxos,
        last_updated,
        next_cursor,
    };

    Ok((StatusCode::OK, Json(out)))
}
