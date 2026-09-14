use axum::{
    extract::{Path, Query},
    http::StatusCode,
    response::IntoResponse,
    Extension, Json,
};
use base64::{engine::general_purpose as b64, Engine};
use serde::Deserialize;

use crate::{
    responses::{ErrorResponse, PaginatedResponse, PoolBlock},
    utils::{self, bad_request, internal_server_error, ParsedCursorPageParams},
    CountParam, CursorPagination, MapiExtension, OrderParam,
};

#[derive(Debug, Deserialize)]
pub struct Params {
    pub epoch_no: Option<i32>,
    pub order: Option<OrderParam>,
}

#[utoipa::path(
    tag = "Pools",
    get,
    path = "/pools/{pool_id}/blocks",
    params(
        ("pool_id" = String, Path, description = "Pool ID in bech32 format"),

        ("epoch_no" = Option<u64>, Query, description = "Epoch number to fetch results for"),

        ("count" = inline(Option<CountParam>), Query, description = "The max number of results per page"),
        ("order" = inline(Option<OrderParam>), Query, description = "The order in which the results are sorted (by block absolute slot)"),
        ("cursor" = inline(Option<String>), Query, description = "Pagination cursor string, use the cursor included in a page of results to fetch the next page"),
    ),
    responses(
        (
            status = 200,
            description = "Return information about blocks minted by a given pool for all epochs (or epoch_no if provided)",
            body = PaginatedPoolBlock,
            example = json!({
                "data": [
                    {
                        "epoch_no": 241,
                        "epoch_slot": 416235,
                        "abs_slot": 19165035,
                        "block_height": 5213445,
                        "block_hash": "6ecd67bb8515cf7ab82c5c0c4a3f6e99c390fdfaaffca4bd7ab0a3365c5327ce",
                        "block_time": 1610731326
                    },
                    {
                        "epoch_no": 243,
                        "epoch_slot": 166752,
                        "abs_slot": 19779552,
                        "block_height": 5243962,
                        "block_hash": "3ad8d2bafc5ad3e7fed82318f03f7b4ae221b5c9ca0b20b47b72a96bdf82a71b",
                        "block_time": 1611345843
                    },
                    {
                        "epoch_no": 280,
                        "epoch_slot": 401137,
                        "abs_slot": 35997937,
                        "block_height": 6042598,
                        "block_hash": "4a0572125268fd327258a6fea5c35bdc1e619bd81d542c36c8cf0a8a92214e21",
                        "block_time": 1627564228
                    },
                    {
                        "epoch_no": 281,
                        "epoch_slot": 105848,
                        "abs_slot": 36134648,
                        "block_height": 6049318,
                        "block_hash": "0cdef0adaf13b7db4a469637439b7847ba822bc05c94710ca4f517b57ac0f3a8",
                        "block_time": 1627700939
                    },
                    {
                        "epoch_no": 281,
                        "epoch_slot": 246962,
                        "abs_slot": 36275762,
                        "block_height": 6056368,
                        "block_hash": "8590d0e47ed7d50a39fed48e9f556c99f644626a08a90d04198810bff8c41bac",
                        "block_time": 1627842053
                    }
                ],
                "last_updated": {
                    "timestamp": "2022-10-10 20:25:28",
                    "block_hash": "6d0c6e1c0966c5ba824fedd4dfa963ee2cab1091e33e3c76551d1dbae50ccd25",
                    "block_slot": 96404672
                },
                "next_cursor": "AAAAAAAAAAA"
            })
        ),
        (status = 400, description = "Malformed query parameters"),
        (status = 500, description = "Internal server error"),
    )
)]
#[tracing::instrument(name = "POOL_BLOCKS", level = "info", skip(config))]
/// Stake pool blocks
///
/// Return information about blocks minted by a given pool for all epochs (or just for epoch `epoch_no` if provided)
pub async fn pool_blocks(
    page_params: Query<CursorPagination>,
    params: Query<Params>,
    Path(pool_id): Path<String>,
    Extension(config): MapiExtension,
) -> Result<impl IntoResponse, ErrorResponse> {
    let dbsync = config.dbsync;
    let chain = config.chain_info;

    // -- parse and try decode user params

    let ParsedCursorPageParams { count, cursor } =
        utils::parse_cursor_page_params(page_params.0, decode_cursor)?;

    // cursor is just page number of the results which were returned (1 index)
    let page = cursor.map(|prev| prev + 1).unwrap_or(0);

    let order = params.order.unwrap_or(OrderParam::Asc);

    // --- get dbsync tip for last updated

    let last_updated = utils::get_last_updated_dbsync(&chain, &dbsync).await?;

    // --- fetch data from db

    let unbinded_query = match order {
        OrderParam::Asc => sqlx::query_as::<_, PoolBlock>(
            "SELECT * FROM grest.pool_blocks($1, $2) ORDER BY abs_slot ASC LIMIT ($3) OFFSET ($4)",
        ),
        OrderParam::Desc => sqlx::query_as::<_, PoolBlock>(
            "SELECT * FROM grest.pool_blocks($1, $2) ORDER BY abs_slot DESC LIMIT ($3) OFFSET ($4)",
        ),
    };

    let mut blocks = unbinded_query
        .bind(pool_id)
        .bind(params.epoch_no)
        .bind((count + 1) as i32)
        .bind((page * count) as i32)
        .fetch_all(&dbsync)
        .await
        .map_err(internal_server_error)?;

    // --- cursor pagination (fake, cursor is just page number)

    let next_cursor = if blocks.len() > count {
        Some(encode_cursor(page))
    } else {
        None
    };

    blocks.truncate(count);

    // ---

    let out = PaginatedResponse {
        data: blocks,
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
