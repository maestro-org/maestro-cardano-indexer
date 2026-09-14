use axum::http::HeaderMap;
use axum::{extract::Query, http::StatusCode, response::IntoResponse, Extension, Json};
use pallas::ledger::addresses::{Address, ShelleyPaymentPart};
use pallas::ledger::primitives::babbage::DatumOption;
use pallas::ledger::traverse::{Era, MultiEraOutput};
use serde::Deserialize;
use tikv_client::KvPair;
use timbre::encoding::decode::{decode_utxos_by_address_key, DecodedAddress};
use timbre::encoding::{
    decode_utxos_by_payment_creds_cursor, encode_utxos_by_payment_creds_cursor,
    UtxosByPaymentCredCursor,
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
    path = "/addresses/cred/utxos",
    params(
        ("resolve_datums" = Option<bool>, Query, description = "Try find and include the corresponding datums for datum hashes"),
        ("with_cbor" = Option<bool>, Query, description = "Include the CBOR encodings of the transaction outputs in the response"),

        ("count" = inline(Option<CountParam>), Query, description = "The max number of results per page"),

        ("cursor" = inline(Option<String>), Query, description = "Pagination cursor string, use the cursor included in a page of results to fetch the next page"),

        ("amounts-as-strings" = Option<String>, Header, description = "Large numbers returned as strings if set to `true`")
    ),
    request_body(content = [String], example = json!(["addr_vkh1wdkle2sprqsuklt34474g6n4ps7k6pv6zwe4644uxmg7xj54y87", "addr_shared_vkh1ewj7sycvy5y234m3uhudn5dggqk3djr0jheacr3utna5gcnmwp2"])),
    responses(
        (
            status = 200,
            description = "Unspent transaction outputs",
            body = PaginatedUtxoWithSlot,
            example = json!({
                "data": [
                    {
                        "tx_hash": "a7e1f137a1d4befa15128286fd08dad67aebe2c11995aa2c04cc48159ccef082",
                        "index": 0,
                        "slot": 55718892,
                        "assets": [
                            {
                                "unit": "lovelace",
                                "amount": 1724100
                            },
                            {
                                "unit": "29760f3fb40b3670144594df635e5bb5d144a1173bff0d96fcad155c43727970746f426173686f2330303037",
                                "amount": 1
                            }
                        ],
                        "address": "addr1w896t6qnpsjs32xhw8jl3kw34pqz69kgd72l8hqw83w0k3qahx2sv",
                        "datum": {
                            "type": "hash",
                            "hash": "950c3e20ab32ba38c75cac5b1a7451800e514326a953c64e2dced7b4772ffcbe",
                            "bytes": null,
                            "json": null
                        },
                        "reference_script": null,
                        "txout_cbor": null
                    },
                    {
                        "tx_hash": "257916e7ae112cf16f27218e41bfa37c018bab922354201b3b38c9a24c35ab33",
                        "index": 1,
                        "slot": 55718892,
                        "assets": [
                            {
                                "unit": "lovelace",
                                "amount": 1444443
                            },
                            {
                                "unit": "f0ff48bbb7bbe9d59a40f1ce90e9e9d0ff5002ec48f232b49ca0fb9a6164616d616e74",
                                "amount": 1
                            }
                        ],
                        "address": "addr1g9ekml92qyvzrjmawxkh64r2w5xr6mg9ngfmxh2khsmdrcudevsft64mf887333adamant",
                        "datum": null,
                        "reference_script": null,
                        "txout_cbor": null
                    }
                ],
                "last_updated": {
                    "timestamp": "2023-10-18 07:30:52",
                    "block_hash": "0e1e924710135acfe200ab13d290bd282a67584fd54456f0dcac0aeaa38bb2c2",
                    "block_slot": 106047961
                },
                "next_cursor": null
            })
        ),
        (status = 400, description = "Malformed query parameters"),
        (status = 500, description = "Internal server error"),
    )
)]
#[tracing::instrument(
    name = "UTXOS_BY_PAYMENT_CREDS",
    level = "info",
    skip(params, config, headers)
)]
/// UTxOs by multiple payment credentials
///
/// Return detailed information on UTxOs which are controlled by some payment credential in a list of payment credentials. No more than 100 payment credentials can be provided.
pub async fn utxos_by_payment_creds(
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

    let mut creds: Vec<String> = payload;

    if creds.len() > 100 {
        return Err(bad_request("Payload size must not exceed 100 items"));
    }

    creds.sort();
    creds.dedup();

    let creds = creds
        .iter()
        .map(|cred: &std::string::String| utils::decode_payment_credential(cred))
        .collect::<Result<Vec<ShelleyPaymentPart>, _>>()?;

    let original_creds = creds.clone();

    let mut creds = creds.into_iter();

    let ParsedCursorPageParams { count, cursor } =
        utils::parse_cursor_page_params(page_params.0, decode_utxos_by_payment_creds_cursor)?;

    // --- start db snapshot at most recent timestamp

    let mut txn = polyphony.begin_snapshot_latest().await?;

    // --- fetch cursor for last updated ---

    let last_updated = utils::get_last_updated(&mut txn, &encoder, &chain).await?;

    // --- initialise `next_cursor`

    // the index of the address that the last result of the page relates to,
    // and the last result, _if_ there is subsequent page
    let mut next_cursor = None;

    // --- parse pagination cursor if provided

    // if we have a cursor, start scanning from the cred, address and utxo in that
    // cursor, else start with the first address and no utxo bound
    let (mut curr_cred, mut lower) = if let Some(c) = cursor {
        let cred = creds
            .nth(c.cred_idx as usize)
            .ok_or_else(|| bad_request("Malformed cursor"))?;

        // we need to know: cred index, address of cursor utxo, cursor utxo
        let l = Some(UtxosByPaymentCredCursor {
            address: c.address,
            slot: c.slot,
            u_hash: c.u_hash,
            u_index: c.u_index,
        });

        (cred, l)
    } else {
        let cred = creds
            .next()
            .ok_or_else(|| bad_request("You must specify at least one payment credential"))?;

        (cred, None)
    };

    // --- first collect all the required kvs

    // one extra to know if we need to return a cursor
    let mut required = count + 1;

    let mut combined_kvs = Vec::new();

    'outer: loop {
        let range = encoder.encode_utxos_by_payment_cred_range(
            chain.network_id(),
            &curr_cred,
            lower.clone(),
            None,
        );

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
            (curr_cred) = match creds.next() {
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
        let (cred, addr) = match addr {
            DecodedAddress::Address(Address::Shelley(a)) => (a.payment().clone(), a),
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

        // if this is the last result of the page, check if there is a subsequent
        // result (and therefore we need to return a cursor for next page)
        if i == (count - 1) && kvs.next().is_some() {
            let cred_idx = original_creds.iter().position(|a| *a == cred).unwrap();
            next_cursor = Some(encode_utxos_by_payment_creds_cursor(
                cred_idx as u16,
                &addr,
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
