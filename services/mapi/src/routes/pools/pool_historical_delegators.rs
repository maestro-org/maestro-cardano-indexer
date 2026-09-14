use axum::{
    extract::{Path, Query},
    http::StatusCode,
    response::IntoResponse,
    Extension, Json,
};
use base64::{engine::general_purpose as b64, Engine};

use crate::{
    responses::{ErrorResponse, HistoricalDelegatorInfo, PaginatedResponse},
    utils::{self, bad_request, internal_server_error, ParsedCursorPageParams},
    CountParam, CursorPagination, MapiExtension,
};

#[utoipa::path(
    tag = "Pools",
    get,
    path = "/pools/{pool_id}/delegators/{epoch_no}",
    params(
        ("pool_id" = String, Path, description = "Pool ID in bech32 format"),
        ("epoch_no" = i32, Path, description = "Epoch number to fetch results for"),

        ("count" = inline(Option<CountParam>), Query, description = "The max number of results per page"),
        ("cursor" = inline(Option<String>), Query, description = "Pagination cursor string, use the cursor included in a page of results to fetch the next page"),
    ),
    responses(
        (
            status = 200,
            description = "Array of information about delegators for a given pool in a specific epoch",
            body = PaginatedHistoricalDelegatorInfo,
            example = json!({
                "data": [{
                    "stake_address": "stake1u83feq9ks8lc5gyre0c3fqr7e643604jgec9jp0tau220mgyj0a8n",
                    "amount": "18272086108353"
                }, {
                    "stake_address": "stake1uxz2du6vka7emfclsgd2we8znyu2edy9exkxnd7n5xg63qga7u2jf",
                    "amount": "4002283786898"
                }],
                "last_updated": {
                    "timestamp": "2023-12-18 10:13:03",
                    "block_hash": "fb42be66ef3ca55b0538c9c69c4f9b55f16b286b45f1817f89e481feb8d5b7bf",
                    "block_slot": 111328092
                },
                "next_cursor": "AAAAAAAAAAA"
            })
        ),
        (status = 400, description = "Malformed query parameters"),
        (status = 500, description = "Internal server error"),
    )
)]
#[tracing::instrument(name = "POOL_HISTORICAL_DELEGATORS", level = "info", skip(config))]
/// Stake pool delegator history
///
/// Returns a list delegators of a pool as of a certain epoch
pub async fn pool_historical_delegators(
    page_params: Query<CursorPagination>,
    Path((pool_id, epoch_no)): Path<(String, i32)>,
    Extension(config): MapiExtension,
) -> Result<impl IntoResponse, ErrorResponse> {
    let dbsync = config.dbsync;
    let chain = config.chain_info;

    // -- parse and try decode user params

    let ParsedCursorPageParams { count, cursor } =
        utils::parse_cursor_page_params(page_params.0, decode_cursor)?;

    // cursor is just page number of the results which were returned (1 index)
    let page = cursor.map(|prev| prev + 1).unwrap_or(0);

    // --- get dbsync tip for last updated

    let last_updated = utils::get_last_updated_dbsync(&chain, &dbsync).await?;

    // --- fetch data from db

    let mut delegators = sqlx::query_as::<_, HistoricalDelegatorInfo>(
        "SELECT * FROM grest.pool_delegators_history($1, $2)
        LIMIT ($3) OFFSET ($4)",
    )
    .bind(pool_id)
    .bind(epoch_no)
    .bind((count + 1) as i32)
    .bind((page * count) as i32)
    .fetch_all(&dbsync)
    .await
    .map_err(internal_server_error)?;

    // --- cursor pagination (fake, cursor is just page number)

    let next_cursor = if delegators.len() > count {
        Some(encode_cursor(page))
    } else {
        None
    };

    delegators.truncate(count);

    // ---

    let out = PaginatedResponse {
        data: delegators,
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
