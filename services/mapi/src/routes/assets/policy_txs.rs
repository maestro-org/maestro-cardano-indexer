use crate::{
    responses::{ErrorResponse, PaginatedResponse, PolicyTransaction},
    utils::{self, scan_extended, ParsedSlotPageParams},
    CountParam, MapiExtension, OrderParam, SlotPagination,
};
use axum::{
    extract::{Path, Query},
    http::StatusCode,
    response::IntoResponse,
    Extension, Json,
};
use itertools::Itertools;
use tikv_client::KvPair;
use timbre::encoding::*;

#[utoipa::path(
    tag = "Asset Policy",
    get,
    path = "/policy/{policy}/transactions",
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
            description = "Transactions involving assets of policy",
            body = PaginatedPolicyTransaction,
            example = json!({
                "data": [{
                    "tx_hash": "09b2c948cf12de9eba590eb6af9ddc1e98b9342a79568b63fea06541cb3fecc1",
                    "slot": 49503576,
                    "assets": ["61736d72", "67657268617274", "647572616e676f", "67756e6e657273"]
                }, {
                    "tx_hash": "1c0375e0aa718c9a203d6fc35522f0de953fcccfe7a74b544a5a246f4d5103d4",
                    "slot": 49503576,
                    "assets": ["6368696c6c696e67", "6d6f6e6579706f6f6c"]
                }, {
                    "tx_hash": "23243cf14d26e87a4ac6e70cc001d81587475759049c8dc8954629f665fe87c7",
                    "slot": 55718892,
                    "assets": ["7468657265616c746f656b6e656573", "646566692e646567656e", "636f70796361742e626f7373636174", "6a7562", "632e6e2e662e742e", "6e6f6f702e646f6767", "77696e672e64616f", "627261696e6c657373", "6e66742e6c6f766572", "6869742e6d792e70616e7473", "626c7565746f756768677579", "686974686f6c6573", "6a61636f6269", "736f6d656461792e73776170", "6261642e636f6f6b", "63617264616e6f2e646567656e"]
                }, {
                    "tx_hash": "6029360e8236d06d81a31e9d8c6748ada97817f227204be092c93ad04fd04e4c",
                    "slot": 55718892,
                    "assets": ["6972732e63727970746f", "2e323233", "6972732e616461"]
                }, {
                    "tx_hash": "6383a77a538f598cd07d7a79dfef7589527833a433d3b8475665f4ec1ed4c19d",
                    "slot": 106047961,
                    "assets": ["666169726661782d726f6265727473", "6861646c65792e636f78", "62616e6b6f66686177616969", "63617264616e6f2e63686164", "74686169726f79616c66616d696c79"]
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
#[tracing::instrument(name = "POLICY_TXS", level = "info", skip(page_params, config))]
/// Transactions involving assets of policy
///
/// Returns transactions which involved (moved, minted) at least one asset of the specified policy, along with a list of all the assets of the policy that were involved.
pub async fn policy_txs(
    page_params: Query<SlotPagination>,
    Path(policy): Path<String>,
    Extension(config): MapiExtension,
) -> Result<impl IntoResponse, ErrorResponse> {
    let chain = config.chain_info;
    let polyphony = config.polyphony_wrapper;
    let encoder = polyphony.txs_by_policy_encoder()?;

    // -- parse and try decode user params

    let policy = utils::decode_policy_id(&policy)?;

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
        None::<fn(_) -> bool>,
        Some(count + 1),
        None,
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

        let (tx_hash, assets_list) = decode_txs_by_policy_value(&v);

        let assets = assets_list.iter().map(hex::encode).sorted().collect();

        txs.push(PolicyTransaction {
            tx_hash: hex::encode(tx_hash),
            slot,
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
