use axum::{
    extract::{Path, Query},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Extension, Json,
};
use pallas::ledger::primitives::babbage::DatumOption;
use pallas::ledger::traverse::MultiEraTx;
use serde::Deserialize;

use crate::{
    responses::{ErrorResponse, TimestampedResponse, UtxoWithBytes},
    utils::{self, assets, datum_option, internal_server_error, not_found, reference_script},
    MapiExtension,
};

#[derive(Debug, Deserialize)]
pub struct Params {
    pub with_cbor: Option<bool>,
}

#[utoipa::path(
    tag = "Transactions",
    get,
    path = "/transactions/{tx_hash}/outputs/{index}/txo",
    params(
        ("with_cbor" = Option<bool>, Query, description = "Include the CBOR encoding of the transaction output in the response"),
        ("tx_hash" = String, Path, description = "Transaction Hash"),
        ("index" = usize, Path, description = "Output Index"),

        ("amounts-as-strings" = Option<String>, Header, description = "Large numbers returned as strings if set to `true`")
    ),
    responses(
        (
            status = 200,
            description = "Get a transaction output via it's output reference",
            body = TimestampedUtxo,
            example = json!({
                "data": {
                    "tx_hash": "291b983ac75275a933415475d6417413b34eaccc5f0794fd7b9e8d04e7fe077e",
                    "index": 0,
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
                        "hash": "5fc2400956b1d1828bea0be9c94e95f2b3cebda27c21b07e8a6bada2b2588163",
                        "bytes": "d8799f584073ab91123886242e4d02de6ed05490d684e99a99c49b957a5f943a80655ac0744372adeacdc7dedb95a818ce676886f1306d439152b82d48de71688008840d00d8799fd8799f1a00030d401a000116e5ffd8799fd8799fd87a9f1b000001863d492040ffd87a80ffd8799fd87a9f1b000001863d649780ffd87a80ffff43555344ff581c0c64d6d0371d11185aae649cf3a169040e94214137137b531ebb16c2ff",
                        "json": {
                            "constructor": 0,
                            "fields": [
                                {
                                    "bytes": "73ab91123886242e4d02de6ed05490d684e99a99c49b957a5f943a80655ac0744372adeacdc7dedb95a818ce676886f1306d439152b82d48de71688008840d00"
                                },
                                {
                                    "constructor": 0,
                                    "fields": [
                                        {
                                            "constructor": 0,
                                            "fields": [
                                                {
                                                    "int": 200000
                                                },
                                                {
                                                    "int": 71397
                                                }
                                            ]
                                        },
                                        {
                                            "constructor": 0,
                                            "fields": [
                                                {
                                                    "constructor": 0,
                                                    "fields": [
                                                        {
                                                            "constructor": 1,
                                                            "fields": [
                                                                {
                                                                    "int": 1676065448000i64
                                                                }
                                                            ]
                                                        },
                                                        {
                                                            "constructor": 1,
                                                            "fields": []
                                                        }
                                                    ]
                                                },
                                                {
                                                    "constructor": 0,
                                                    "fields": [
                                                        {
                                                            "constructor": 1,
                                                            "fields": [
                                                                {
                                                                    "int": 1676067248000i64
                                                                }
                                                            ]
                                                        },
                                                        {
                                                            "constructor": 1,
                                                            "fields": []
                                                        }
                                                    ]
                                                }
                                            ]
                                        },
                                        {
                                            "bytes": "555344"
                                        }
                                    ]
                                },
                                {
                                    "bytes": "0c64d6d0371d11185aae649cf3a169040e94214137137b531ebb16c2"
                                }
                            ]
                        }
                    },
                    "reference_script": null,
                    "txout_cbor": null
                },
                "last_updated": {
                    "timestamp": "2022-10-10 20:25:28",
                    "block_hash": "3c504cc4ce12a511db04838b544820c4e2281c0209a906ca17981cf63a084b7b",
                    "block_slot": 32295757
                }
            })
        ),
        (status = 400, description = "Malformed query parameters"),
        (status = 404, description = "No results found"),
        (status = 500, description = "Internal server error"),
    )
)]
#[tracing::instrument(name = "TXO_BY_TXO_REF", level = "info", skip(config, headers))]
/// Transaction output by output reference
///
/// Returns the specified transaction output. Attempts to resolve the datum hash to the corresponding bytes and JSON, should the output contain a datum hash.
pub async fn txo_by_txo_ref(
    headers: HeaderMap,
    params: Query<Params>,
    Path((tx_hash, index)): Path<(String, usize)>,
    Extension(config): MapiExtension,
) -> Result<impl IntoResponse, ErrorResponse> {
    let chain = config.chain_info;
    let polyphony = config.polyphony_wrapper;

    let with_cbor = params.with_cbor.unwrap_or(false);

    let (cbor, mut txn) = polyphony.get_tx_bytes(&tx_hash).await?;

    // --- fetch cursor for last updated ---

    let last_updated =
        utils::get_last_updated(&mut txn, &polyphony.tx_by_hash_encoder()?, &chain).await?;

    // ---

    let tx = MultiEraTx::decode(&cbor).map_err(internal_server_error)?;

    let txo = if index == tx.outputs().len() {
        tx.collateral_return().ok_or_else(not_found)?
    } else {
        tx.output_at(index).ok_or_else(not_found)?
    };

    let datum_hash = if let Some(DatumOption::Hash(dh)) = txo.datum() {
        vec![*dh]
    } else {
        vec![]
    };

    let assets = assets(&txo, &headers);
    let datum = datum_option(
        &txo,
        Some(
            &utils::fetch_datums(&mut txn, &polyphony.datum_by_hash_encoder()?, datum_hash).await?,
        ),
    );
    let reference_script = reference_script(&txo);

    let txout_cbor = with_cbor.then_some(hex::encode(txo.encode()));

    let utxo = UtxoWithBytes {
        tx_hash: tx_hash.to_string(),
        index,
        address: txo.address().map_err(internal_server_error)?.to_string(),
        assets,
        datum,
        reference_script,
        txout_cbor,
    };

    // ---

    let out = TimestampedResponse {
        data: utxo,
        last_updated,
    };

    Ok((StatusCode::OK, Json(out)))
}
