use axum::{extract::Query, http::StatusCode, response::IntoResponse, Extension, Json};
use base64::{engine::general_purpose as b64, Engine};

use crate::{
    responses::{ErrorResponse, PaginatedResponse, PoolListInfo},
    utils::{self, bad_request, internal_server_error, ParsedCursorPageParams},
    CountParam, CursorPagination, MapiExtension,
};

#[utoipa::path(
    tag = "Pools",
    get,
    path = "/pools",
    params(
        ("count" = inline(Option<CountParam>), Query, description = "The max number of results per page"),
        ("cursor" = inline(Option<String>), Query, description = "Pagination cursor string, use the cursor included in a page of results to fetch the next page"),
    ),
    responses(
        (
            status = 200,
            description = "List of all registered stake pools (ticker can be null)",
            body = PaginatedPoolListInfo,
            example = json!({
                "data": [
                    {
                        "pool_id_bech32": "pool100wj94uzf54vup2hdzk0afng4dhjaqggt7j434mtgm8v2gfvfgp",
                        "ticker": "JFLD"
                    },
                    {
                        "pool_id_bech32": "pool102s2nqtea2hf5q0s4amj0evysmfnhrn4apyyhd4azcmsclzm96m",
                        "ticker": "YULI"
                    },
                    {
                        "pool_id_bech32": "pool102vsulhfx8ua2j9fwl2u7gv57fhhutc3tp6juzaefgrn7ae35wm",
                        "ticker": "BNP"
                    },
                    {
                        "pool_id_bech32": "pool1030as3pp5684ghgf4kzcpv4p2jnmkmme7j363t95690zwxp7wa0",
                        "ticker": "888"
                    },
                    {
                        "pool_id_bech32": "pool1030vz0wxheg0dvarr8hmeavelpszmp52qucs68c7wc9uga6n6e4",
                        "ticker": "ROMER"
                    },
                    {
                        "pool_id_bech32": "pool10ysem79xjxd223plxqws0fheawd78znr3qs4h2h422gmxa40amu",
                        "ticker": "LUCY"
                    }
                ],
                "last_updated": {
                    "timestamp": "2022-10-10 20:25:28",
                    "block_hash": "73087e0231d665d82edcf7215b5381c26e406d111fe0075f987e59861dd6e451",
                    "block_slot": 96404335
                },
                "next_cursor": "AAAAAAAAAAA"
            })
        ),
        (status = 400, description = "Malformed query parameters"),
        (status = 500, description = "Internal server error"),
    )
)]
#[tracing::instrument(name = "LIST_POOLS", level = "info", skip(config))]
/// List registered stake pools
///
/// Returns a list of currently registered stake pools
pub async fn list_pools(
    page_params: Query<CursorPagination>,
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

    let mut pools = sqlx::query_as::<_, PoolListInfo>(
        "SELECT * FROM grest.pool_list()
        LIMIT ($1) OFFSET ($2)",
    )
    .bind((count + 1) as i32)
    .bind((page * count) as i32)
    .fetch_all(&dbsync)
    .await
    .map_err(internal_server_error)?;

    // --- cursor pagination (fake, cursor is just page number)

    let next_cursor = if pools.len() > count {
        Some(encode_cursor(page))
    } else {
        None
    };

    pools.truncate(count);

    // ---

    let out = PaginatedResponse {
        data: pools,
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
