use axum::{
    extract::{Path, Query},
    http::StatusCode,
    response::IntoResponse,
    Extension, Json,
};
use tikv_client::KvPair;
use timbre::encoding::{
    decode_txs_by_policy_cursor, decode_txs_by_policy_key, decode_txs_by_policy_value,
    encode_txs_by_policy_cursor,
};

use crate::{
    responses::{ErrorResponse, PaginatedResponse, TimestampedTransaction},
    utils::{self, scan_extended, ParsedSlotPageParams},
    CountParam, MapiExtension, OrderParam, SlotPagination,
};

#[utoipa::path(
    tag = "Assets",
    get,
    path = "/assets/{asset}/transactions",
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
            description = "An array of transactions",
            body = PaginatedTimestampedTransaction,
            example = json!({
                "data": [{
                    "tx_hash": "ad266dfdf3bec26462d326dcfca1bfd359ca72659bd792be19edd0e69708c466",
                    "slot": 49503576,
                    "timestamp": "2022-01-01 20:44:27"
                }, {
                    "tx_hash": "2e36e8f8d600d95ac3851879f0fb930756e4f88917bf697c9a725049ce25f4eb",
                    "slot": 49503576,
                    "timestamp": "2022-01-01 20:44:27"
                }, {
                    "tx_hash": "bc21b95844a8b4b08c20a848c4bc7f5be08df11bb3f75cf9510836c7259bc9fc",
                    "slot": 55718892,
                    "timestamp": "2022-03-14 19:13:03"
                }, {
                    "tx_hash": "904e3cd77c10def76dadd4c1cb87166ed707bae43e5d7da8dcccaf3066b5bfe2",
                    "slot": 106047961,
                    "timestamp": "2023-10-18 07:30:52"
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
#[tracing::instrument(name = "ASSET_TXS", level = "info", skip(config))]
/// Native asset transactions
///
/// Returns a list of transactions in which some of a specific asset is moved or minted
pub async fn asset_txs(
    page_params: Query<SlotPagination>,
    Path(asset): Path<String>,
    Extension(config): MapiExtension,
) -> Result<impl IntoResponse, ErrorResponse> {
    let chain = config.chain_info;
    let polyphony: crate::PolyphonyWrapper = config.polyphony_wrapper;
    let timeout = config.timeout_duration;

    let encoder = polyphony.txs_by_policy_encoder()?;

    // -- parse and try decode user params

    let (policy, name) = utils::decode_asset(&asset)?;

    let name_vec = name.to_vec();

    let ParsedSlotPageParams {
        count,
        order,
        lower,
        upper,
    } = utils::parse_slot_page_params(page_params.0, decode_txs_by_policy_cursor)?;

    // --- start db at most recent timestamp

    let mut txn = polyphony.begin_snapshot_latest().await?;

    // --- fetch cursor for last updated ---

    let last_updated = utils::get_last_updated(&mut txn, &encoder, &chain).await?;

    // --- initialise `next_cursor`

    let mut next_cursor = None;

    // --- scan keys for this page (max count plus one to see if there is another page)

    let range = encoder.encode_txs_by_policy_range(&policy, lower, upper);

    let kvs = scan_extended(
        &mut txn,
        range,
        order,
        Some(|KvPair(_, v)| policy_tx_contains_asset(v, &name_vec)),
        Some(count + 1),
        Some(timeout),
    )
    .await?;

    // --- process fetched keys into response data

    let mut kvs = kvs.into_iter().enumerate();

    let mut txs = Vec::new();

    while let Some((i, KvPair(k, v))) = kvs.next() {
        let key = decode_txs_by_policy_key((&k).into());

        let (slot, blk_index) = (key.slot, key.blk_index);

        // if this is the last result of the page, check if there is a subsequent
        // result (and therefore we need to return a cursor for next page)
        if i == (count - 1) && kvs.next().is_some() {
            next_cursor = Some(encode_txs_by_policy_cursor(slot, blk_index));
        }

        let (tx_hash, _) = decode_txs_by_policy_value(&v);

        txs.push(TimestampedTransaction {
            tx_hash: hex::encode(tx_hash),
            slot,
            timestamp: chain.slot_to_utc(slot),
        })
    }

    let out = PaginatedResponse {
        data: txs,
        last_updated,
        next_cursor,
    };

    Ok((StatusCode::OK, Json(out)))
}

fn policy_tx_contains_asset(v: Vec<u8>, asset_name: &Vec<u8>) -> bool {
    let (_, assets) = decode_txs_by_policy_value(&v);

    assets.contains(asset_name)
}
