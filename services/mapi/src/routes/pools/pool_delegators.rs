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
    responses::{DelegatorInfo, ErrorResponse, PaginatedResponse},
    utils::{
        self, bad_request, get_maybe_lovelace_from_string, internal_server_error,
        ParsedCursorPageParams,
    },
    CountParam, CursorPagination, MapiExtension,
};

#[utoipa::path(
    tag = "Pools",
    get,
    path = "/pools/{pool_id}/delegators",
    params(
        ("pool_id" = String, Path, description = "Pool ID in bech32 format"),
        ("count" = inline(Option<CountParam>), Query, description = "The max number of results per page"),
        ("cursor" = inline(Option<String>), Query, description = "Pagination cursor string, use the cursor included in a page of results to fetch the next page"),

        ("amounts-as-strings" = Option<String>, Header, description = "Large numbers returned as strings if set to `true`")
    ),
    responses(
        (
            status = 200,
            description = "Array of information about current delegators for a given pool",
            body = PaginatedDelegatorInfo,
            example = json!({
                "data": [
                    {
                        "stake_address": "stake1u82hrwv9ucsa38k4qejwenmq5zmleexlsplc4acpvl3g6asxr8aay",
                        "amount": 838095398379i64,
                        "active_epoch_no": 234,
                        "latest_delegation_tx_hash": "94019e466d8c1a89f88df4127980bc6fb3f432b21c3f498ceb2daaedd721f04b"
                    },
                    {
                        "stake_address": "stake1uy0yzgwlv2898qwap6qtg7uwavknxtqfs3c2u8x35rgf5jcx0z2tv",
                        "amount": 19763135363i64,
                        "active_epoch_no": 363,
                        "latest_delegation_tx_hash": "659911f1d712961c6795c1c082b216704351f368ef52388b9c0564e7d947218c"
                    },
                    {
                        "stake_address": "stake1uy7cha5x6k67xq0c5adpsknf8982pdr4vwrn9yaxapzz9esmllnll",
                        "amount": 22896634615i64,
                        "active_epoch_no": 234,
                        "latest_delegation_tx_hash": "100124ac08d9beaa68c9592a35bd4479ccdf20f74b6ff60bc87311fa4ca6b248"
                    },
                    {
                        "stake_address": "stake1uylvs9r8f98cjfehvdy8ypa9feslckdy48m4vn8ruvaupas47scvy",
                        "amount": 1474,
                        "active_epoch_no": 263,
                        "latest_delegation_tx_hash": "c317381b0d9e243dea2eacefe1a85d9e2186b6a80e70ca9c9680a757353a7a3e"
                    }
                ],
                "last_updated": {
                    "timestamp": "2022-10-10 20:25:28",
                    "block_hash": "525024696e2a2fa3312bc726d443aebae17763387207299c3e8b394dddf592e4",
                    "block_slot": 96404843
                },
                "next_cursor": null
            })
        ),
        (status = 400, description = "Malformed query parameters"),
        (status = 500, description = "Internal server error"),
    )
)]
#[tracing::instrument(name = "POOL_DELEGATORS", level = "info", skip(config, headers))]
/// Stake pool delegators
///
/// Returns a list of delegators of the specified pool
pub async fn pool_delegators(
    headers: HeaderMap,
    page_params: Query<CursorPagination>,
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

    // --- get dbsync tip for last updated

    let last_updated = utils::get_last_updated_dbsync(&chain, &dbsync).await?;

    // --- fetch data from db

    let mut delegators = sqlx::query_as::<_, DbsyncDelegatorInfo>(
        "SELECT * FROM grest.pool_delegators($1)
        LIMIT ($2) OFFSET ($3)",
    )
    .bind(pool_id)
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

    let delegators = delegators
        .into_iter()
        .map(|x| DelegatorInfo {
            stake_address: x.stake_address,
            amount: utils::u64_str_conv(x.amount.unwrap_or(0), &headers),
            active_epoch_no: x.active_epoch_no,
            latest_delegation_tx_hash: x.latest_delegation_tx_hash,
        })
        .collect();

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

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Information summary of a delegator
pub struct DbsyncDelegatorInfo {
    /// Bech32 encoded stake address (reward address)
    stake_address: Option<String>,
    /// Delegator live stake
    amount: Option<u64>,
    /// Epoch at which the delegation becomes active
    active_epoch_no: Option<i64>,
    /// Transaction hash relating to the most recent delegation
    latest_delegation_tx_hash: Option<String>,
}

impl<'r> FromRow<'r, PgRow> for DbsyncDelegatorInfo {
    fn from_row(row: &'r PgRow) -> Result<Self, sqlx::Error> {
        let stake_address = row.try_get("stake_address")?;
        let amount = get_maybe_lovelace_from_string(row, "amount")?;
        let active_epoch_no = row.try_get("active_epoch_no")?;
        let latest_delegation_tx_hash = row.try_get("latest_delegation_tx_hash")?;

        Ok(DbsyncDelegatorInfo {
            stake_address,
            amount,
            active_epoch_no,
            latest_delegation_tx_hash,
        })
    }
}
