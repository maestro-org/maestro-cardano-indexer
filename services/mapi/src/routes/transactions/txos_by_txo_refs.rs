use std::collections::HashMap;

use axum::http::HeaderMap;
use axum::{extract::Query, http::StatusCode, response::IntoResponse, Extension, Json};
use base64::{engine::general_purpose as b64, Engine};
use pallas::ledger::primitives::babbage::DatumOption;
use pallas::ledger::traverse::MultiEraTx;
use serde::Deserialize;
use tikv_client::KvPair;
use timbre::encoding::decode::decode_tx_by_hash_key;

use crate::{
    responses::{ErrorResponse, PaginatedResponse, UtxoWithBytes},
    utils::{
        self, assets, bad_request, datum_option, internal_server_error, not_found,
        reference_script, ParsedCursorPageParams,
    },
    CountParam, CursorPagination, MapiExtension,
};

#[derive(Debug, Deserialize)]
pub struct Params {
    pub resolve_datums: Option<bool>,
    pub with_cbor: Option<bool>,
    pub allow_missing: Option<bool>,
}

#[utoipa::path(
    tag = "Transactions",
    post,
    path = "/transactions/outputs",
    params(
        ("resolve_datums" = Option<bool>, Query, description = "Try find and include the corresponding datums for datum hashes"),
        ("with_cbor" = Option<bool>, Query, description = "Include the CBOR encoding of the transaction output in the response"),
        ("allow_missing" = Option<bool>, Query, description = "Do not return 404 if any transactions are not found (404 will still be returned if you specify an index higher than the number of outputs in a transaction)"),

        ("count" = inline(Option<CountParam>), Query, description = "The max number of results per page"),
        ("cursor" = inline(Option<String>), Query, description = "Pagination cursor string, use the cursor included in a page of results to fetch the next page"),
    ),
    request_body(content = [String], example = json!(["a90e31b3de59452659617c351e5f746b819cb8b026bf945dd41b4cc199bcc8c9#1", "31a84c3c6200bec2498b18c42f882fa690cd0d32a9c84a2019eb5cc42f5971d0#0"])),
    responses(
        (
            status = 200,
            description = "Get transaction outputs via output references",
            body = PaginatedUtxoWithBytes,
            example = json!({
                "data": [
                    {
                        "tx_hash": "31a84c3c6200bec2498b18c42f882fa690cd0d32a9c84a2019eb5cc42f5971d0",
                        "index": 0,
                        "assets": [
                            {
                                "unit": "lovelace",
                                "amount": 27660180
                            }
                        ],
                        "address": "addr1wxn9efv2f6w82hagxqtn62ju4m293tqvw0uhmdl64ch8uwc0h43gt",
                        "datum": {
                            "type": "hash",
                            "hash": "f03819a4039003a8c6b65153351adbc61f983bd22787ed238a5b4f24a01aa5d6",
                            "bytes": "d8799fd8799fd8799f581cfc6e1b47816bc4a4165a39f3c6dd65868e6b74a743dc2dcf54eb2d4cffd8799fd8799fd8799f581cd756fb7ceee90ed5af3f193d73f9970ca640ab540eedac272cb28c29ffffffffd8799fd8799f581cfc6e1b47816bc4a4165a39f3c6dd65868e6b74a743dc2dcf54eb2d4cffd8799fd8799fd8799f581cd756fb7ceee90ed5af3f193d73f9970ca640ab540eedac272cb28c29ffffffffd87a80d87a9fd8799f581c25f0fc240e91bd95dcdaebd2ba7713fc5168ac77234a3d79449fc20c47534f4349455459ff1a0bebc200ff1a001e84801a001e8480ff",
                            "json": {
                                "constructor": 0,
                                "fields": [
                                    {
                                        "constructor": 0,
                                        "fields": [
                                            {
                                                "constructor": 0,
                                                "fields": [
                                                    {
                                                        "bytes": "fc6e1b47816bc4a4165a39f3c6dd65868e6b74a743dc2dcf54eb2d4c"
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
                                                                "constructor": 0,
                                                                "fields": [
                                                                    {
                                                                        "bytes": "d756fb7ceee90ed5af3f193d73f9970ca640ab540eedac272cb28c29"
                                                                    }
                                                                ]
                                                            }
                                                        ]
                                                    }
                                                ]
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
                                                        "bytes": "fc6e1b47816bc4a4165a39f3c6dd65868e6b74a743dc2dcf54eb2d4c"
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
                                                                "constructor": 0,
                                                                "fields": [
                                                                    {
                                                                        "bytes": "d756fb7ceee90ed5af3f193d73f9970ca640ab540eedac272cb28c29"
                                                                    }
                                                                ]
                                                            }
                                                        ]
                                                    }
                                                ]
                                            }
                                        ]
                                    },
                                    {
                                        "constructor": 1,
                                        "fields": []
                                    },
                                    {
                                        "constructor": 1,
                                        "fields": [
                                            {
                                                "constructor": 0,
                                                "fields": [
                                                    {
                                                        "bytes": "25f0fc240e91bd95dcdaebd2ba7713fc5168ac77234a3d79449fc20c"
                                                    },
                                                    {
                                                        "bytes": "534f4349455459"
                                                    }
                                                ]
                                            },
                                            {
                                                "int": 200000000
                                            }
                                        ]
                                    },
                                    {
                                        "int": 2000000
                                    },
                                    {
                                        "int": 2000000
                                    }
                                ]
                            }
                        },
                        "reference_script": null,
                        "txout_cbor": null
                    },
                    {
                        "tx_hash": "a90e31b3de59452659617c351e5f746b819cb8b026bf945dd41b4cc199bcc8c9",
                        "index": 1,
                        "assets": [
                            {
                                "unit": "lovelace",
                                "amount": 118671749
                            }
                        ],
                        "address": "addr1qy27nwry749alh8ajmkghml67j9egdx9asgcnsjk8wldfs40m5d3qle5j85vfd335zzxfdz9eshkjh58svvjwtgxt0tskx7f5l",
                        "datum": null,
                        "reference_script": null,
                        "txout_cbor": null
                    }
                ],
                "last_updated": {
                    "timestamp": "2022-10-10 20:25:28",
                    "block_hash": "bdf4630098ac6923e0ef1ca0b0d6e00dca96790a8c3dd670948fdfe1456798d2",
                    "block_slot": 73867237
                },
                "next_cursor": null
            })
        ),
        (status = 400, description = "Malformed query parameters"),
        (status = 404, description = "No results found"),
        (status = 500, description = "Internal server error"),
    )
)]
#[tracing::instrument(name = "TXOS_BY_TXO_REFS", level = "info", skip(config, headers))]
/// Transaction outputs by output references
///
/// Returns the specified transaction outputs. Returns 404 if any of the outputs specified do not exist unless the `allow_missing` query parameter is set to `true`. Results are sorted lexicographically by output reference and duplicates are omitted. Do not change the output references parameter while paginating.
pub async fn txos_by_txo_refs(
    headers: HeaderMap,
    page_params: Query<CursorPagination>,
    params: Query<Params>,
    Extension(config): MapiExtension,
    Json(payload): Json<Vec<String>>,
) -> Result<impl IntoResponse, ErrorResponse> {
    let chain = config.chain_info;
    let polyphony = config.polyphony_wrapper;
    let encoder = polyphony.tx_by_hash_encoder()?;

    // -- parse and try decode user params

    let resolve_datums = params.resolve_datums.unwrap_or(false);
    let with_cbor = params.with_cbor.unwrap_or(false);
    let allow_missing = params.allow_missing.unwrap_or(false);

    let ParsedCursorPageParams { count, cursor } =
        utils::parse_cursor_page_params(page_params.0, decode_cursor)?;

    let mut out_refs = payload.clone();

    out_refs.sort();
    out_refs.dedup();

    let asked = out_refs.len();

    let page_start_idx = if let Some(a) = cursor {
        if a > out_refs.len() {
            return Err(bad_request("Malformed cursor"));
        }

        out_refs = out_refs.split_off(a);

        a
    } else {
        0
    };

    let mut next_cursor = None;

    // --- parse the output reference strings

    let mut parsed_refs = Vec::with_capacity(out_refs.len());

    for out_ref in out_refs {
        let parts: Vec<&str> = out_ref.split('#').collect();

        if parts.len() != 2 {
            return Err(bad_request(
                "Output references must be of form '<tx_hash>#<index>'",
            ));
        }

        let tx_hash: [u8; 32] = match hex::decode(parts[0]) {
            Ok(b) => b
                .try_into()
                .map_err(|_| bad_request("Malformed transaction hash"))?,
            Err(_) => return Err(bad_request("Transaction hash must be hex encoded")),
        };

        let index: usize = parts[1]
            .parse()
            .map_err(|_| bad_request("Malformed output reference index"))?;

        parsed_refs.push((tx_hash, index))
    }

    // --- start db at most recent timestamp

    let mut txn = polyphony.begin_snapshot_latest().await?;

    // --- fetch cursor for last updated ---

    let last_updated = utils::get_last_updated(&mut txn, &encoder, &chain).await?;

    // --- batch fetch all required transactions

    let required_keys: Vec<Vec<u8>> = parsed_refs
        .iter()
        .map(|(h, _)| encoder.encode_tx_by_hash_key(h))
        .collect();

    let kvs: HashMap<[u8; 32], Vec<u8>> = txn
        .batch_get(required_keys.to_vec())
        .await
        .map_err(internal_server_error)?
        .map(|KvPair(k, v)| (decode_tx_by_hash_key((&k).into()).tx_hash, v))
        .collect();

    // --- process fetched kvs

    let txs_map: HashMap<[u8; 32], MultiEraTx> = kvs
        .iter()
        .map(|(k, v)| {
            MultiEraTx::decode(v)
                .map(|tx| (*k, tx))
                .map_err(internal_server_error)
        })
        .collect::<Result<_, _>>()?;

    let mut datum_hashes = Vec::new();
    let mut txouts = Vec::new();

    for (idx, (tx_hash, index)) in parsed_refs.into_iter().enumerate() {
        let tx = if let Some(x) = txs_map.get(&tx_hash) {
            x
        } else if allow_missing {
            continue;
        } else {
            return Err(not_found());
        };

        let txo = if index == tx.outputs().len() {
            tx.collateral_return().ok_or_else(not_found)?
        } else {
            tx.output_at(index).ok_or_else(not_found)?
        };

        // note the dh so we can batch resolve it if needed
        if let Some(DatumOption::Hash(dh)) = txo.datum() {
            datum_hashes.push(*dh)
        }

        let address = txo.address().map_err(internal_server_error)?.to_string();

        txouts.push((tx_hash, index, txo, address));

        if txouts.len() == count {
            let last_item_idx = page_start_idx + idx + 1;

            if last_item_idx < asked {
                next_cursor = Some(encode_cursor(last_item_idx));
            }

            break;
        };
    }

    // --- create datum hash to datum map if resolving hashes

    let resolved_dhs = resolve_datums.then_some(
        utils::fetch_datums(&mut txn, &polyphony.datum_by_hash_encoder()?, datum_hashes).await?,
    );

    // --- craft response data

    let utxos = txouts
        .into_iter()
        .map(|(tx_hash, index, txo, address)| {
            let assets = assets(&txo, &headers);
            let datum = datum_option(&txo, resolved_dhs.as_ref());
            let reference_script = reference_script(&txo);

            let txout_cbor = with_cbor.then_some(hex::encode(txo.encode()));

            UtxoWithBytes {
                tx_hash: hex::encode(tx_hash),
                index,
                address,
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

fn decode_cursor(b64_cursor: &String) -> Result<usize, ErrorResponse> {
    let bytes: [u8; 8] = b64::URL_SAFE_NO_PAD
        .decode(b64_cursor)
        .map_err(|_| bad_request("Malformed cursor"))?
        .try_into()
        .map_err(|_| bad_request("Malformed cursor"))?;

    Ok(u64::from_be_bytes(bytes) as usize)
}

fn encode_cursor(item: usize) -> String {
    b64::URL_SAFE_NO_PAD.encode(u64::to_be_bytes(item as u64))
}
