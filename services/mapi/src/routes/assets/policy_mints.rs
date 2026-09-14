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
    responses::{AssetInPolicyMint, ErrorResponse, PaginatedResponse, PolicyMintTransaction},
    utils::{self, scan_extended, ParsedSlotPageParams},
    CountParam, MapiExtension, OrderParam, SlotPagination,
};

#[utoipa::path(
    tag = "Asset Policy",
    get,
    path = "/policy/{policy}/mints",
    params(
        ("policy" = String, Path, description = "Hex encoded policy ID"),

        ("count" = inline(Option<CountParam>), Query, description = "The max number of results per page"),

        ("order" = inline(Option<OrderParam>), Query, description = "The order in which the results are sorted (by point in chain)"),
        ("from" = inline(Option<u64>), Query, description = "Return only transactions in blocks minted on or after a specific slot"),
        ("to" = inline(Option<u64>), Query, description = "Return only transactions in blocks minted before a specific slot"),

        ("cursor" = inline(Option<String>), Query, description = "Pagination cursor string, use the cursor included in a page of results to fetch the next page"),
    ),
    responses(
        (
            status = 200,
            description = "Array of transactions with assets and amounts minted",
            body = PaginatedPolicyMintTransaction,
            example = json!({
                "data": [{
                    "tx_hash": "257916e7ae112cf16f27218e41bfa37c018bab922354201b3b38c9a24c35ab33",
                    "slot": 55718892,
                    "timestamp": "2022-03-14 19:13:03",
                    "assets": [{
                        "name": "6164616d616e74",
                        "amount": "1"
                    }, {
                        "name": "6264736d",
                        "amount": "1"
                    }, {
                        "name": "63617264616e6f737765657473",
                        "amount": "1"
                    }]
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
#[tracing::instrument(name = "POLICY_MINTS", level = "info", skip(config))]
/// Transactions minting or burning assets of policy
///
/// Returns a list of transactions in which some of the specified asset was minted or burned
pub async fn policy_mints(
    page_params: Query<SlotPagination>,
    Path(policy): Path<String>,
    Extension(config): MapiExtension,
) -> Result<impl IntoResponse, ErrorResponse> {
    let chain = config.chain_info;
    let polyphony = config.polyphony_wrapper;
    let encoder = polyphony.updates_by_policy_encoder()?;

    // -- parse and try decode user params

    let policy = utils::decode_policy_id(&policy)?;

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
        None::<fn(_) -> bool>,
        Some(count + 1),
        None,
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

        let assets = assets
            .into_iter()
            .map(|(n, x)| AssetInPolicyMint {
                name: hex::encode(n),
                amount: x.to_string(),
            })
            .sorted_by_key(|x| x.name.clone())
            .collect();

        txs.push(PolicyMintTransaction {
            tx_hash: hex::encode(tx_hash),
            slot,
            timestamp: chain.slot_to_utc(slot),
            assets,
        })
    }

    let out = PaginatedResponse {
        data: txs,
        last_updated,
        next_cursor,
    };

    Ok((StatusCode::OK, Json(out)))
}
