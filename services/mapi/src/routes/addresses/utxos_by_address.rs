use crate::{
    responses::{ErrorResponse, PaginatedResponse, UtxoWithSlot},
    utils::{
        self, assets, datum_option, internal_server_error, reference_script, scan_extended,
        ParsedSlotPageParams,
    },
    CountParam, MapiExtension, OrderParam, SlotPagination,
};
use axum::{
    extract::{Path, Query},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Extension, Json,
};
use pallas::ledger::primitives::babbage::DatumOption;
use pallas::ledger::traverse::{Era, MultiEraOutput};
use serde::Deserialize;
use tikv_client::KvPair;
use timbre::encoding::{
    decode::{decode_utxos_by_address_cursor, decode_utxos_by_address_key},
    encode::encode_address_utxos_cursor,
};

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
    path = "/addresses/{address}/utxos",
    params(
        ("address" = String, Path, description = "Address in bech32 format"),

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
            description = "Get all unspent transaction outputs at an address",
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
#[tracing::instrument(name = "UTXOS_BY_ADDRESS", level = "info", skip(config, headers))]
/// UTxOs at an address
///
/// Return detailed information on UTxOs controlled by an address
pub async fn utxos_by_address(
    headers: HeaderMap,
    page_params: Query<SlotPagination>,
    params: Query<Params>,
    Path(address): Path<String>,
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

    let address = utils::decode_payment_address(&address)?;

    let filter_asset = params.asset.as_ref().map(utils::decode_asset).transpose()?;

    let ParsedSlotPageParams {
        count,
        order,
        lower,
        upper,
    } = utils::parse_slot_page_params(page_params.0, decode_utxos_by_address_cursor)?;

    // --- start db snapshot at most recent timestamp

    let mut txn = polyphony.begin_snapshot_latest().await?;

    // --- fetch cursor for last updated ---

    let last_updated = utils::get_last_updated(&mut txn, &encoder, &chain).await?;

    // --- initialise `next_cursor`

    let mut next_cursor = None;

    // --- scan keys for this page (max count plus one to see if there is another page)

    let range = encoder.encode_address_utxos_range(&address, lower, upper);

    let kvs = if let Some((policy, name)) = filter_asset {
        scan_extended(
            &mut txn,
            range,
            order,
            Some(|KvPair(_, v)| address_utxo_contains_asset(v, &policy, &name)),
            Some(count + 1),
            Some(timeout),
        )
        .await?
    } else {
        scan_extended(
            &mut txn,
            range,
            order,
            None::<fn(_) -> bool>,
            Some(count + 1),
            None,
        )
        .await?
    };

    // --- process fetched kvs

    let mut kvs = kvs.iter().enumerate();

    let mut datum_hashes = Vec::new();
    let mut txouts = Vec::new();

    while let Some((i, KvPair(k, txo_bytes))) = kvs.next() {
        let key = decode_utxos_by_address_key(k.into());

        let (slot, tx_hash, index) = (key.slot, key.utxo_hash, key.utxo_index);

        let txo = MultiEraOutput::decode(Era::Conway, txo_bytes)
            .or_else(|_| MultiEraOutput::decode(Era::Babbage, txo_bytes))
            .or_else(|_| MultiEraOutput::decode(Era::Alonzo, txo_bytes))
            .or_else(|_| MultiEraOutput::decode(Era::Byron, txo_bytes))
            .map_err(internal_server_error)?;

        // note the dh so we can batch resolve it if needed
        if let Some(DatumOption::Hash(dh)) = txo.datum() {
            datum_hashes.push(*dh)
        }

        txouts.push((tx_hash, index, slot, txo));

        // if this is the last result of the page, check if there is a subsequent
        // result (and therefore we need to return a cursor for next page)
        if i == (count - 1) && kvs.next().is_some() {
            next_cursor = Some(encode_address_utxos_cursor(slot, tx_hash, index));
        }
    }

    // --- create datum hash to datum map if resolving hashes

    let resolved_dhs = resolve_datums.then_some(
        utils::fetch_datums(&mut txn, &polyphony.datum_by_hash_encoder()?, datum_hashes).await?,
    );

    // --- craft response data

    let utxos = txouts
        .into_iter()
        .map(|(tx_hash, index, slot, txo)| {
            let assets = assets(&txo, &headers);
            let datum = datum_option(&txo, resolved_dhs.as_ref());
            let reference_script = reference_script(&txo);

            let txout_cbor = with_cbor.then_some(hex::encode(txo.encode()));

            UtxoWithSlot {
                tx_hash: hex::encode(tx_hash),
                index: index as usize,
                slot,
                address: address.to_string(),
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
