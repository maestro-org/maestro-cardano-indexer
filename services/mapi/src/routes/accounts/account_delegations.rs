use crate::responses::{AccountDelegation, ErrorResponse, PaginatedResponse};
use crate::utils::{self, bad_request, internal_server_error, ParsedCursorPageParams};
use crate::{CountParam, CursorPagination, MapiExtension, OrderParam};
use axum::{
    extract::{Path, Query},
    http::StatusCode,
    response::IntoResponse,
    Extension, Json,
};
use base64::{engine::general_purpose as b64, Engine};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Params {
    pub order: Option<OrderParam>,
}

#[utoipa::path(
    tag = "Accounts",
    get,
    path = "/accounts/{stake_addr}/delegations",
    params(
        ("stake_addr" = String, Path, description = "Bech32 encoded stake/reward address ('stake1...')"),

        ("count" = inline(Option<CountParam>), Query, description = "The max number of results per page"),
        ("order" = inline(Option<OrderParam>), Query, description = "The order in which the results are sorted (by epoch number)"),
        ("cursor" = inline(Option<String>), Query, description = "Pagination cursor string, use the cursor included in a page of results to fetch the next page"),
    ),
    responses(
        (
            status = 200,
            description = "Stake account delegations",
            body = PaginatedAccountDelegation,
            example = json!({
                "data": [{
                    "active_epoch_no": 397,
                    "pool_id": "pool1jst7rrhucnp93hepezv5yqy6fx982xs2v0udwfc5ea6my3kfak7",
                    "slot": 85518752
                }, {
                    "active_epoch_no": 462,
                    "pool_id": "pool1ywpt43nttzjd7883wafg255mh0hmjypwe65ercw6p2sxg5lt7ez",
                    "slot": 113498793
                }],
                "last_updated": {
                    "timestamp": "2024-01-12 14:06:24",
                    "block_hash": "8295c809a899e67bbf6a1a90408eb5b9f5b563e64931e1db2fa51ac057a9716f",
                    "block_slot": 113502093
                },
                "next_cursor": null
            })
        ),
        (status = 400, description = "Malformed query parameters"),
        (status = 500, description = "Internal server error"),
    )
)]
#[tracing::instrument(name = "ACCOUNT_DELEGATIONS", level = "info", skip(config))]
/// Stake account delegation history
///
/// Returns list of delegation actions relating to a stake account
pub async fn account_delegations(
    page_params: Query<CursorPagination>,
    params: Query<Params>,
    Path(stake_addr): Path<String>,
    Extension(config): MapiExtension,
) -> Result<impl IntoResponse, ErrorResponse> {
    let dbsync = config.dbsync;
    let chain = config.chain_info;

    // -- parse and try decode user params

    let ParsedCursorPageParams { count, cursor } =
        utils::parse_cursor_page_params(page_params.0, decode_cursor)?;

    let order = params.order.unwrap_or(OrderParam::Asc);

    // --- get dbsync tip for last updated

    let last_updated = utils::get_last_updated_dbsync(&chain, &dbsync).await?;

    // --- fetch all data from db

    // ~https://github.com/IntersectMBO/cardano-db-sync/blob/master/doc/interesting-queries.md#get-the-delegation-history-for-a-specified-stake-address
    let mut delegations: Vec<AccountDelegation> =
        sqlx::query_as("
            select
                delegation.active_epoch_no, delegation.tx_id, delegation.cert_index, pool_hash.view AS pool_id, delegation.slot_no
            from
                delegation
                inner join stake_address on delegation.addr_id = stake_address.id
                inner join pool_hash on delegation.pool_hash_id = pool_hash.id
            where
                stake_address.view = $1
            order by
                (active_epoch_no, delegation.tx_id, cert_index) asc
            ")
            .bind(stake_addr)
            .fetch_all(&dbsync)
            .await
            .map_err(internal_server_error)?;

    // --- cursor pagination

    if order == OrderParam::Desc {
        delegations.reverse()
    }

    let page_start_idx = if let Some(a) = cursor {
        if a > delegations.len() {
            return Err(bad_request("Malformed cursor"));
        }

        delegations = delegations.split_off(a);

        a
    } else {
        0
    };

    let next_cursor = if delegations.len() > count {
        let last_item_idx = page_start_idx + count;

        Some(encode_cursor(last_item_idx))
    } else {
        None
    };

    delegations.truncate(count);

    // ---

    let out = PaginatedResponse {
        data: delegations,
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
