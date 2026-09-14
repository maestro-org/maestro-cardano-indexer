use axum::{
    extract::{Path, Query},
    http::StatusCode,
    response::IntoResponse,
    Extension, Json,
};
use std::str;
use timbre::{
    encoding::{
        decode::{decode_key, decode_utxos_by_address_cursor},
        encode::encode_address_utxos_cursor,
    },
    Key, ReducerKey, UtxosByAddressKey,
};

use crate::{
    responses::{ErrorResponse, PaginatedResponse, UtxoRef},
    utils::{self, internal_server_error, internal_server_error_str, ParsedSlotPageParams},
    CountParam, MapiExtension, OrderParam, SlotPagination,
};

#[utoipa::path(
    tag = "Addresses",
    get,
    path = "/addresses/{address}/utxo_refs",
    params(
        ("address" = String, Path, description = "Address in bech32 format"),

        ("count" = inline(Option<CountParam>), Query, description = "The max number of results per page"),

        ("order" = inline(Option<OrderParam>), Query, description = "The order in which the results are sorted (by slot at which UTxO was produced)"),
        ("from" = inline(Option<u64>), Query, description = "Return only UTxOs created on or after a specific slot"),
        ("to" = inline(Option<u64>), Query, description = "Return only UTxOs created on or before a specific slot"),

        ("cursor" = inline(Option<String>), Query, description = "Pagination cursor string, use the cursor included in a page of results to fetch the next page"),
    ),
    responses(
        (
            status = 200,
            description = "UTxO references for all the unspent transaction outputs at an address",
            body = PaginatedUtxoRef,
            example = json!({
                "data": [
                    {
                        "tx_hash": "b24743fb4b5381971ee8b3d02a3fd5e783acdb5a21cd2d22d192ec84e7e279fc",
                        "index": 0
                    },
                    {
                        "tx_hash": "e2e91c7be8239b9f52fadec8df627fe759d21e5b6559c3ec06d8120f1366e075",
                        "index": 0
                    }
                ],
                "last_updated": {
                    "timestamp": "2022-10-10 20:25:28",
                    "block_hash": "8b78a8e4f3f097a6a6fcb20901326b660e4c8b2d4a45f77dea3f2c6e223adf96",
                    "block_slot": 32266099
                },
                "next_cursor": null
            })
        ),
        (status = 400, description = "Malformed query parameters"),
        (status = 500, description = "Internal server error"),
    )
)]
#[tracing::instrument(
    name = "UTXO_REFS_AT_ADDRESS",
    level = "info",
    skip(page_params, config)
)]
/// UTxO references at an address
///
/// Returns references (pair of transaction hash and output index in transaction) for UTxOs controlled by the specified address
pub async fn utxo_refs_at_address(
    page_params: Query<SlotPagination>,
    Path(address): Path<String>,
    Extension(config): MapiExtension,
) -> Result<impl IntoResponse, ErrorResponse> {
    let polyphony = config.polyphony_wrapper;
    let chain = config.chain_info;

    let encoder = polyphony.utxo_cbor_by_address_encoder()?;

    // TODO share code between utxo refs, utxos, utxos at multiple addr

    // -- parse and try decode user params

    let address = utils::decode_payment_address(&address)?;

    let ParsedSlotPageParams {
        count,
        order,
        lower,
        upper,
    } = utils::parse_slot_page_params(page_params.0, decode_utxos_by_address_cursor)?;

    // --- start db at most recent timestamp

    let mut txn = polyphony.begin_snapshot_latest().await?;

    // --- fetch cursor for last updated ---

    let last_updated = utils::get_last_updated(&mut txn, &encoder, &chain).await?;

    // --- initialise `next_cursor`

    let mut next_cursor = None;

    // --- scan keys for this page (max count plus one to see if there is another page)

    let range = encoder.encode_address_utxos_range(&address, lower, upper);

    // TODO global tikv wrapper with helpers
    let kvs: Vec<tikv_client::Key> = match order {
        OrderParam::Asc => txn
            .scan_keys(range, (count + 1) as u32)
            .await
            .map_err(internal_server_error)?
            .collect(),
        OrderParam::Desc => txn
            .scan_keys_reverse(range, (count + 1) as u32)
            .await
            .map_err(internal_server_error)?
            .collect(),
    };

    // --- process fetched keys into response data

    let mut kvs = kvs.into_iter().enumerate();

    let mut utxos = Vec::new();

    while let Some((i, k)) = kvs.next() {
        // TODO try_decode_X_keyxw or something
        let decoded_key = decode_key(&Into::<Vec<u8>>::into(k));

        let (tx_hash, index, slot) =
            if let Key::Reducer((_, _, ReducerKey::AddressUtxos(key @ UtxosByAddressKey { .. }))) =
                decoded_key
            {
                (key.utxo_hash, key.utxo_index, key.slot)
            } else {
                return Err(internal_server_error_str(
                    "unexpected key scanned: {decoded:?}",
                ));
            };

        // this is the last result of the page, check if there is a subsequent
        // result (and therefore we need to return a cursor for next page)
        if i == (count - 1) && kvs.next().is_some() {
            next_cursor = Some(encode_address_utxos_cursor(slot, tx_hash, index));
        }

        utxos.push(UtxoRef {
            tx_hash: hex::encode(tx_hash),
            index,
        })
    }

    let out = PaginatedResponse {
        data: utxos,
        last_updated,
        next_cursor,
    };

    Ok((StatusCode::OK, Json(out)))
}
