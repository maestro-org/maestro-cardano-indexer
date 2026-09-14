use axum::{
    extract::{Path, Query},
    http::StatusCode,
    response::IntoResponse,
    Extension, Json,
};
use itertools::Itertools;
use tikv_client::KvPair;
use timbre::encoding::{
    decode_updates_by_policy_cursor, decode_updates_by_policy_key, decode_updates_by_policy_value,
    encode_updates_by_policy_cursor,
};

use crate::{
    responses::{ErrorResponse, MintTransaction, PaginatedResponse},
    utils::{self, internal_server_error_str, scan_extended, ParsedSlotPageParams},
    CountParam, MapiExtension, OrderParam, SlotPagination,
};

#[utoipa::path(
    tag = "Assets",
    get,
    path = "/assets/{asset}/mints",
    params(
        ("asset" = String, Path, description = "Asset, encoded as concatenation of hex of policy ID and asset name"),

        ("count" = inline(Option<CountParam>), Query, description = "The max number of results per page"),

        ("order" = inline(Option<OrderParam>), Query, description = "The order in which the results are sorted (by point in chain)"),
        ("from" = inline(Option<u64>), Query, description = "Return only transactions in blocks minted on or after a specific slot"),
        ("to" = inline(Option<u64>), Query, description = "Return only transactions in blocks minted on or before a specific slot"),

        ("cursor" = inline(Option<String>), Query, description = "Pagination cursor string, use the cursor included in a page of results to fetch the next page"),
    ),
    responses(
        (
            status = 200,
            description = "Array of transactions with amounts of asset minted",
            body = PaginatedMintTransaction,
            example = json!({
                "data": [{
                    "tx_hash": "257916e7ae112cf16f27218e41bfa37c018bab922354201b3b38c9a24c35ab33",
                    "slot": 55718892,
                    "timestamp": "2022-03-14 19:13:03",
                    "amount": "1"
                }],
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
#[tracing::instrument(name = "ASSET_MINTS", level = "info", skip(config))]
/// Native asset mints and burns
///
/// Returns a list of transactions in which some of the specified asset was minted or burned
pub async fn asset_mints(
    page_params: Query<SlotPagination>,
    Path(asset): Path<String>,
    Extension(config): MapiExtension,
) -> Result<impl IntoResponse, ErrorResponse> {
    let chain = config.chain_info;
    let polyphony = config.polyphony_wrapper;
    let timeout = config.timeout_duration;

    let encoder = polyphony.updates_by_policy_encoder()?;

    // -- parse and try decode user params

    let (policy, name) = utils::decode_asset(&asset)?;

    let name_vec = name.to_vec();

    let ParsedSlotPageParams {
        count,
        order,
        lower,
        upper,
    } = utils::parse_slot_page_params(page_params.0, decode_updates_by_policy_cursor)?;

    // --- start db at most recent timestamp

    let mut txn = polyphony.begin_snapshot_latest().await?;

    // --- fetch cursor for last updated ---

    let last_updated = utils::get_last_updated(&mut txn, &encoder, &chain).await?;

    // --- initialise `next_cursor`

    let mut next_cursor = None;

    // --- scan keys for this page (max count plus one to see if there is another page)

    let range = encoder.encode_updates_by_policy_range(&policy, lower, upper);

    let kvs = scan_extended(
        &mut txn,
        range,
        order,
        Some(|KvPair(_, v)| policy_update_contains_asset(v, &name_vec)),
        Some(count + 1),
        Some(timeout),
    )
    .await?;

    // --- process fetched keys into response data

    let mut kvs = kvs.into_iter().enumerate();

    let mut txs = Vec::new();

    while let Some((i, KvPair(k, v))) = kvs.next() {
        let key = decode_updates_by_policy_key((&k).into());

        let (slot, blk_index) = (key.slot, key.blk_index);

        // if this is the last result of the page, check if there is a subsequent
        // result (and therefore we need to return a cursor for next page)
        if i == (count - 1) && kvs.next().is_some() {
            next_cursor = Some(encode_updates_by_policy_cursor(slot, blk_index));
        }

        let (tx_hash, assets) = decode_updates_by_policy_value(&v);

        let amount = assets
            .into_iter()
            .find(|(n, _)| *n == name_vec)
            .map(|(_, x)| x)
            .ok_or_else(|| internal_server_error_str("mint tx contents error"))?;

        txs.push(MintTransaction {
            tx_hash: hex::encode(tx_hash),
            slot,
            timestamp: chain.slot_to_utc(slot),
            amount: amount.to_string(),
        })
    }

    let out = PaginatedResponse {
        data: txs,
        last_updated,
        next_cursor,
    };

    Ok((StatusCode::OK, Json(out)))
}

fn policy_update_contains_asset(v: Vec<u8>, asset_name: &Vec<u8>) -> bool {
    let (_, assets) = decode_updates_by_policy_value(&v);

    assets
        .into_iter()
        .map(|(name, _)| name)
        .contains(asset_name)
}
