/*
   Results sorted as:
       addresses sorted lexicographically on bech32 string representation
       all the utxos for the first address are returned, then all the utxos for
       the second address are returned, ...
       all utxos for an specific address are sorted ascending by slot

   So the results for query ['addr1bbb', 'addr1aaa'] will be ordered like:
       [address: addr1aaa, slot: 1, utxo: ...]
       [address: addr1aaa, slot: 2, utxo: ...]
       [address: addr1aaa, slot: 3, utxo: ...]
       [address: addr1bbb, slot: 1, utxo: ...]
       [address: addr1bbb, slot: 4, utxo: ...]
       [address: addr1bbb, slot: 5, utxo: ...]
*/

use axum::http::HeaderMap;
use axum::{extract::Query, http::StatusCode, response::IntoResponse, Extension, Json};
use pallas::ledger::addresses::Address;
use pallas::ledger::primitives::babbage::DatumOption;
use pallas::ledger::traverse::{Era, MultiEraOutput};
use serde::Deserialize;
use tikv_client::KvPair;
use timbre::encoding::{
    decode::{decode_utxos_by_address_key, decode_utxos_by_addresses_cursor, DecodedAddress},
    encode::encode_addresses_utxos_cursor,
    RangeBound, UtxosByAddressCursor,
};

use crate::{
    responses::{ErrorResponse, PaginatedResponse, UtxoWithSlot},
    utils::{
        self, assets, bad_request, datum_option, internal_server_error, internal_server_error_str,
        reference_script, ParsedCursorPageParams,
    },
    CountParam, CursorPagination, MapiExtension,
};

#[derive(Deserialize)]
pub struct Params {
    pub resolve_datums: Option<bool>,
    pub with_cbor: Option<bool>,
}

#[utoipa::path(
    tag = "Addresses",
    post,
    path = "/addresses/utxos",
    params(
        ("resolve_datums" = Option<bool>, Query, description = "Try find and include the corresponding datums for datum hashes"),
        ("with_cbor" = Option<bool>, Query, description = "Include the CBOR encodings of the transaction outputs in the response"),

        ("count" = inline(Option<CountParam>), Query, description = "The max number of results per page"),

        ("cursor" = inline(Option<String>), Query, description = "Pagination cursor string, use the cursor included in a page of results to fetch the next page"),

        ("amounts-as-strings" = Option<String>, Header, description = "Large numbers returned as strings if set to `true`")
    ),
    request_body(content = [String], example = json!(["addr_test1wr2x24tlcpr37sjrscaqsh6z4tue3k7zx8qt8n0kscen2jct0wkz7", "addr_test1wplyjq2gqaufjt6uux6g9ax9s7mcc50rm3f8zgsmquknggce4mde8"])),
    responses(
        (
            status = 200,
            description = "Get all unspent transaction outputs residing at any address in a list",
            body = PaginatedUtxoWithSlot,
            example = json!({
                "data": [
                    {
                        "tx_hash": "21d0ff7e2b5d8988ff999f404668aff3f72f9e7a7de5eaa3740cb306f3024ca9",
                        "index": 0,
                        "slot": 20648797,
                        "assets": [
                            {
                                "unit": "lovelace",
                                "amount": 20000000
                            }
                        ],
                        "address": "addr_test1wr2x24tlcpr37sjrscaqsh6z4tue3k7zx8qt8n0kscen2jct0wkz7",
                        "datum": null,
                        "reference_script": {
                            "hash": "d465557fc0471f4243863a085f42aaf998dbc231c0b3cdf68633354b",
                            "type": "plutusv2",
                            "bytes": "5910630100003....10030021120011",
                            "json": null
                        },
                        "txout_cbor": null
                    },
                    {
                        "tx_hash": "b2054dc932454372808972d92e46e6725d57438d9095cb77010a12aa843b5e01",
                        "index": 1,
                        "slot": 21610369,
                        "assets": [
                            {
                                "unit": "lovelace",
                                "amount": 2000000
                            },
                            {
                                "unit": "c00c2ee78495d1cc21010b354c56975ca3f0202e2ea9e16ebab0acfa4f7261636c6546656564",
                                "amount": 1
                            }
                        ],
                        "address": "addr_test1wr2x24tlcpr37sjrscaqsh6z4tue3k7zx8qt8n0kscen2jct0wkz7",
                        "datum": {
                            "type": "inline",
                            "hash": "b908508e84bd37060d4515eb25a43396610a4d6ce363a89ff802f40a7f0a1729",
                            "bytes": "d8799fd87b9fa3001a000593e9011b00000186867c6928021b00000186868a24c8ffff",
                            "json": {
                                "constructor": 0,
                                "fields": [
                                    {
                                        "constructor": 2,
                                        "fields": [
                                            {
                                                "map": [
                                                    {
                                                        "k": {
                                                            "int": 0
                                                        },
                                                        "v": {
                                                            "int": 365545
                                                        }
                                                    },
                                                    {
                                                        "k": {
                                                            "int": 1
                                                        },
                                                        "v": {
                                                            "int": 1677293545768i64
                                                        }
                                                    },
                                                    {
                                                        "k": {
                                                            "int": 2
                                                        },
                                                        "v": {
                                                            "int": 1677294445768i64
                                                        }
                                                    }
                                                ]
                                            }
                                        ]
                                    }
                                ]
                            }
                        },
                        "reference_script": null,
                        "txout_cbor": null
                    },
                    {
                        "tx_hash": "45c205a85aa13b2bb8677675937deac2d19ac6fccf84905d845e1ca9df62bac3",
                        "index": 0,
                        "slot": 21610637,
                        "assets": [
                            {
                                "unit": "lovelace",
                                "amount": 3000000
                            },
                            {
                                "unit": "436941ead56c61dbf9b92b5f566f7d5b9cac08f8c957f28f0bd60d4b5041594d454e54544f4b454e",
                                "amount": 850860
                            },
                            {
                                "unit": "c00c2ee78495d1cc21010b354c56975ca3f0202e2ea9e16ebab0acfa4167675374617465",
                                "amount": 1
                            }
                        ],
                        "address": "addr_test1wr2x24tlcpr37sjrscaqsh6z4tue3k7zx8qt8n0kscen2jct0wkz7",
                        "datum": {
                            "type": "inline",
                            "hash": "83115d854808011060dff35ab238823b693250e7a089386cfbd422ce55f50e76",
                            "bytes": "d87b9fd8799fd879....6dffffffff",
                            "json": {
                                "constructor": 2,
                                "fields": [
                                    {
                                        "constructor": 0,
                                        "fields": [
                                            {
                                                "constructor": 0,
                                                "fields": [
                                                    {
                                                        "list": [
                                                            {
                                                                "bytes": "007df380aef26e44739db3f4fe67d8137446e630dab3df16d9fbddc5"
                                                            },
                                                            {
                                                                "bytes": "cef7fb5f89a9c76a65acdd746d9e84104d6f824d7dc44f427fcaa1dd"
                                                            },
                                                            {
                                                                "bytes": "4ad1571e7df63d4d6c49240c8372eb639f57c0ef669338c0d752f29b"
                                                            },
                                                            {
                                                                "bytes": "f6f69e5af37c2978cb2124c12202f2185fd5c14ee93bb832911daf8e"
                                                            },
                                                            {
                                                                "bytes": "2d7103fdaf4beecbbef37edc6d24d311230f2836d0af791e3a6364d2"
                                                            }
                                                        ]
                                                    },
                                                    {
                                                        "int": 7500
                                                    },
                                                    {
                                                        "int": 900000
                                                    },
                                                    {
                                                        "int": 900000
                                                    },
                                                    {
                                                        "int": 200
                                                    },
                                                    {
                                                        "constructor": 0,
                                                        "fields": [
                                                            {
                                                                "int": 20
                                                            }
                                                        ]
                                                    },
                                                    {
                                                        "int": 20000
                                                    },
                                                    {
                                                        "int": 1500
                                                    },
                                                    {
                                                        "list": [
                                                            {
                                                                "bytes": "496e6469676f4f7261636c65446174756d"
                                                            }
                                                        ]
                                                    }
                                                ]
                                            }
                                        ]
                                    }
                                ]
                            }
                        },
                        "reference_script": null,
                        "txout_cbor": null
                    }
                ],
                "last_updated": {
                    "timestamp": "2022-10-10 20:25:28",
                    "block_hash": "2d6b15048692aca81cc6dbd9234360ed6163e838fcf9d9f1a9e8dc67e0779f82",
                    "block_slot": 32266164
                },
                "next_cursor": null
            })
        ),
        (status = 400, description = "Malformed query parameters"),
        (status = 500, description = "Internal server error"),
    )
)]
#[tracing::instrument(
    name = "UTXOS_BY_ADDRESSES",
    level = "info",
    skip(params, config, headers)
)]
/// UTxOs by multiple addresses
///
/// Return detailed information on UTxOs which are controlled by some address in the specified list of addresses
pub async fn utxos_by_addresses(
    headers: HeaderMap,
    page_params: Query<CursorPagination>,
    params: Query<Params>,
    Extension(config): MapiExtension,
    Json(payload): Json<Vec<String>>,
) -> Result<impl IntoResponse, ErrorResponse> {
    let polyphony = config.polyphony_wrapper;
    let chain = config.chain_info;

    let encoder = polyphony.utxo_cbor_by_address_encoder()?;

    // -- parse endpoint specific params

    let resolve_datums = params.resolve_datums.unwrap_or(false);
    let with_cbor = params.with_cbor.unwrap_or(false);

    // -- parse and try decode user params

    let mut addresses: Vec<String> = payload;

    addresses.sort();
    addresses.dedup();

    let addresses = addresses
        .iter()
        .map(|addr: &std::string::String| utils::decode_payment_address(addr))
        .collect::<Result<Vec<Address>, _>>()?;

    let original_addresses = addresses.clone();

    let mut addresses = addresses.into_iter();

    let ParsedCursorPageParams { count, cursor } =
        utils::parse_cursor_page_params(page_params.0, decode_utxos_by_addresses_cursor)?;

    // --- start db snapshot at most recent timestamp

    let mut txn = polyphony.begin_snapshot_latest().await?;

    // --- fetch cursor for last updated ---

    let last_updated = utils::get_last_updated(&mut txn, &encoder, &chain).await?;

    // --- initialise `next_cursor`

    // the index of the address that the last result of the page relates to,
    // and the last result, _if_ there is subsequent page
    let mut next_cursor = None;

    // --- parse pagination cursor if provided

    // if we have a cursor, start scanning from the address and utxo in that
    // cursor, else start with the first address and no utxo bound
    let (mut curr_address, mut lower) = if let Some(c) = cursor {
        let addr = addresses
            .nth(c.address_idx as usize)
            .ok_or_else(|| bad_request("Malformed cursor"))?;

        let l = Some(RangeBound::Cursor(UtxosByAddressCursor {
            slot: c.slot,
            u_hash: c.u_hash,
            u_index: c.u_index,
        }));

        (addr, l)
    } else {
        let addr = addresses
            .next()
            .ok_or_else(|| bad_request("You must specify at least one address"))?;

        (addr, None)
    };

    // --- first collect all the required kvs

    // one extra to know if we need to return a cursor
    let mut required = count + 1;

    let mut combined_kvs = Vec::new();

    'outer: loop {
        let range = encoder.encode_address_utxos_range(&curr_address, lower.clone(), None);

        let kvs: Vec<KvPair> = txn
            .scan(range, (required) as u32)
            .await
            .map_err(internal_server_error)?
            .collect();

        let kvs_len = kvs.len();

        for kv in kvs {
            combined_kvs.push(kv);
        }

        if kvs_len == required {
            break;
        } else {
            // we have not found all the required kvs, move onto the next address
            required -= kvs_len;

            // if we have ran out of addresses then return the kvs we found
            (curr_address) = match addresses.next() {
                Some(a) => a,
                None => break 'outer,
            };

            lower = None;
        }
    }

    // --- process fetched kvs

    let mut datum_hashes = Vec::new();
    let mut txouts = Vec::new();

    let mut kvs = combined_kvs.iter().enumerate();

    while let Some((i, KvPair(k, txo_bytes))) = kvs.next() {
        let key = decode_utxos_by_address_key(k.into());

        let (addr, slot, tx_hash, index) = (key.address, key.slot, key.utxo_hash, key.utxo_index);

        // TODO
        let addr = match addr {
            DecodedAddress::Address(a) => a,
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

        // if this is the last result of the page, check if there is a subsequent
        // result (and therefore we need to return a cursor for next page)
        if i == (count - 1) && kvs.next().is_some() {
            let addr_idx = original_addresses.iter().position(|a| *a == addr).unwrap();
            next_cursor = Some(encode_addresses_utxos_cursor(
                addr_idx as u16,
                slot,
                tx_hash,
                index,
            ));
        }

        txouts.push((tx_hash, index, slot, txo, addr));
    }

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
                address: addr.to_string(),
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
