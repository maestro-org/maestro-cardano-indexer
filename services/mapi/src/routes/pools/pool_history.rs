use axum::{
    extract::{Path, Query},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Extension, Json,
};
use base64::{engine::general_purpose as b64, Engine};
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgRow, FromRow, Row};

use crate::{
    responses::{ErrorResponse, PaginatedResponse, PoolHistory},
    utils::{
        self, bad_request, get_maybe_lovelace_from_string, get_maybe_string_from_numeric,
        internal_server_error, ParsedCursorPageParams,
    },
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
    path = "/pools/{pool_id}/history",
    params(
        ("pool_id" = String, Path, description = "Pool ID in bech32 format"),

        ("epoch_no" = Option<u64>, Query, description = "Epoch number to fetch results for"),

        ("count" = inline(Option<CountParam>), Query, description = "The max number of results per page"),
        ("order" = inline(Option<OrderParam>), Query, description = "The order in which the results are sorted (by epoch number)"),
        ("cursor" = inline(Option<String>), Query, description = "Pagination cursor string, use the cursor included in a page of results to fetch the next page"),

        ("amounts-as-strings" = Option<String>, Header, description = "Large numbers returned as strings if set to `true`")
    ),
    responses(
        (
            status = 200,
            description = "An array of pool history information for each epoch (or containing one entry for a given epoch_no)",
            body = PaginatedPoolHistory,
            example = json!({
                "data": [
                    {
                        "epoch_no": 6,
                        "active_stake": 100000000000000i64,
                        "active_stake_pct": "33.33333333333333333300",
                        "saturation_pct": null,
                        "block_cnt": 7243,
                        "delegator_cnt": 1,
                        "margin": 1,
                        "fixed_cost": 500000000,
                        "pool_fees": 0,
                        "deleg_rewards": 0,
                        "epoch_ros": "0"
                    },
                    {
                        "epoch_no": 7,
                        "active_stake": 100000000000000i64,
                        "active_stake_pct": "33.33333333333333333300",
                        "saturation_pct": null,
                        "block_cnt": 7213,
                        "delegator_cnt": 1,
                        "margin": 1,
                        "fixed_cost": 500000000,
                        "pool_fees": 0,
                        "deleg_rewards": 0,
                        "epoch_ros": "0"
                    },
                    {
                        "epoch_no": 8,
                        "active_stake": 100000000000000i64,
                        "active_stake_pct": "33.33333333333333333300",
                        "saturation_pct": null,
                        "block_cnt": 7102,
                        "delegator_cnt": 1,
                        "margin": 1,
                        "fixed_cost": 500000000,
                        "pool_fees": 0,
                        "deleg_rewards": 0,
                        "epoch_ros": "0"
                    }
                ],
                "last_updated": {
                    "timestamp": "2022-10-10 20:25:28",
                    "block_hash": "c8814a8d064e18b46ffbde6add240203df27533c3ca1fb91c8031b88029ec979",
                    "block_slot": 32288058
                },
                "next_cursor": null
            })
        ),
        (status = 400, description = "Malformed query parameters"),
        (status = 500, description = "Internal server error"),
    )
)]
#[tracing::instrument(name = "POOL_HISTORY", level = "info", skip(config, headers))]
/// Stake pool history
///
/// Returns per-epoch information about the specified pool (or just for epoch `epoch_no` if provided)
pub async fn pool_history(
    headers: HeaderMap,
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

    // koios table is already ordered by epoch_no descending
    let unbinded_query = match order {
        OrderParam::Asc => sqlx::query_as::<_, DbsyncPoolHistory>(
            "SELECT * FROM grest.pool_history($1, $2) ORDER BY epoch_no ASC LIMIT ($3) OFFSET ($4)",
        ),
        OrderParam::Desc => sqlx::query_as::<_, DbsyncPoolHistory>(
            "SELECT * FROM grest.pool_history($1, $2) LIMIT ($3) OFFSET ($4)",
        ),
    };

    let mut epochs = unbinded_query
        .bind(pool_id)
        .bind(params.epoch_no)
        .bind((count + 1) as i32)
        .bind((page * count) as i32)
        .fetch_all(&dbsync)
        .await
        .map_err(internal_server_error)?;

    // --- cursor pagination (fake, cursor is just page number)

    let next_cursor = if epochs.len() > count {
        Some(encode_cursor(page))
    } else {
        None
    };

    epochs.truncate(count);

    let mut modified_epochs = vec![];

    for epoch in epochs {
        let active_stake = epoch.active_stake.map(|x| utils::u64_str_conv(x, &headers));
        let margin = epoch.margin.map(|x| utils::f64_str_conv(x, &headers));

        let fixed_cost = utils::u64_str_conv(epoch.fixed_cost, &headers);
        let pool_fees = utils::u64_str_conv(epoch.pool_fees, &headers);
        let deleg_rewards = utils::u64_str_conv(epoch.deleg_rewards, &headers);

        modified_epochs.push(PoolHistory {
            epoch_no: epoch.epoch_no,
            active_stake,
            active_stake_pct: epoch.active_stake_pct,
            saturation_pct: epoch.saturation_pct,
            block_cnt: epoch.block_cnt,
            delegator_cnt: epoch.delegator_cnt,
            margin,
            fixed_cost,
            pool_fees,
            deleg_rewards,
            epoch_ros: epoch.epoch_ros,
        })
    }

    // ---

    let out = PaginatedResponse {
        data: modified_epochs,
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

#[derive(Debug, PartialEq, Serialize, Deserialize)]
/// Per-epoch history of a stake pool
pub struct DbsyncPoolHistory {
    /// Epoch number
    pub epoch_no: i64,
    /// Active stake in the epoch
    active_stake: Option<u64>,
    /// Pool active stake as percentage of total active stake
    active_stake_pct: Option<String>,
    /// Pool saturation percent
    saturation_pct: Option<String>,
    /// Blocks created in the epoch
    pub block_cnt: Option<i64>,
    /// Delegators in the epoch
    delegator_cnt: Option<i64>,
    /// Pool margin
    margin: Option<f64>,
    /// Pool fixed cost
    fixed_cost: u64,
    /// Fees collected for the epoch
    pool_fees: u64,
    /// Total rewards earned by pool delegators for the epoch
    deleg_rewards: u64,
    /// Annual return percentage for delegators for the epoch
    epoch_ros: String,
}

impl<'r> FromRow<'r, PgRow> for DbsyncPoolHistory {
    fn from_row(row: &'r PgRow) -> Result<Self, sqlx::Error> {
        let epoch_no = row.try_get("epoch_no")?;
        let active_stake = get_maybe_lovelace_from_string(row, "active_stake")?;
        let active_stake_pct = get_maybe_string_from_numeric(row, "active_stake_pct")?;
        let saturation_pct = get_maybe_string_from_numeric(row, "saturation_pct")?;
        // block_cnt and delegator_cnt have been updated to type `bigint` in Koios v1.3.0
        // for backward compatibility, also handle parsing them as type `numeric`.
        let block_cnt = match row.try_get::<Option<i64>, _>("block_cnt") {
            Ok(val) => val,
            Err(_) => get_maybe_string_from_numeric(row, "block_cnt")?.map(|x| x.parse().unwrap()),
        };
        let delegator_cnt = match row.try_get::<Option<i64>, _>("delegator_cnt") {
            Ok(val) => val,
            Err(_) => {
                get_maybe_string_from_numeric(row, "delegator_cnt")?.map(|x| x.parse().unwrap())
            }
        };
        let margin = row.try_get("margin")?;
        let fixed_cost = get_maybe_lovelace_from_string(row, "fixed_cost")?.unwrap_or_default();
        let pool_fees = get_maybe_lovelace_from_string(row, "pool_fees")?.unwrap_or_default();
        let deleg_rewards =
            get_maybe_lovelace_from_string(row, "deleg_rewards")?.unwrap_or_default();
        let epoch_ros = get_maybe_string_from_numeric(row, "epoch_ros")?.unwrap_or_default();

        Ok(DbsyncPoolHistory {
            epoch_no,
            active_stake,
            active_stake_pct,
            saturation_pct,
            block_cnt,
            delegator_cnt,
            margin,
            fixed_cost,
            pool_fees,
            deleg_rewards,
            epoch_ros,
        })
    }
}
