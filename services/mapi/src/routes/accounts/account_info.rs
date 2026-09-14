use axum::{
    extract::Path,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgRow, FromRow, Row};
use utoipa::ToSchema;

use crate::{
    responses::{AccountInfo, ErrorResponse, TimestampedResponse},
    utils::{self, get_lovelace_from_string, internal_server_error, not_found},
    MapiExtension,
};

#[utoipa::path(
    tag = "Accounts",
    get,
    path = "/accounts/{stake_addr}",
    params(
        ("stake_addr" = String, Path, description = "Bech32 encoded reward/stake address ('stake1...')"),

        ("amounts-as-strings" = Option<String>, Header, description = "Large numbers returned as strings if set to `true`")
    ),
    responses(
        (
            status = 200,
            description = "Information about the account",
            body = TimestampedAccountInfo,
            example = json!({
                "data": {
                    "stake_address": "stake_test1uzq7mcr5mmj2jeqqf5eaxxd4sdzvszqnd26vnz5nvn0mcaqte3ycr",
                    "registered": true,
                    "delegated_pool": "pool1egfg26w0syqly9qc65hz33gqv2qrzyka8tfue3ccsk3c73a56jp",
                    "delegated_drep": "drep1ydvwp8x9j8u4flwjqwxf8jdjwp9p4zf2x4rk7vv7fgzk6j2k7c6",
                    "rewards_available": 24648659341i64,
                    "utxo_balance": 249497357989i64,
                    "total_balance": 274146017330i64,
                    "total_rewarded": 24648659341i64,
                    "total_withdrawn": 0
                },
                "last_updated": {
                    "timestamp": "2022-10-10 20:25:28",
                    "block_hash": "d99c239b6fae8036b3057b6837a563d045ce2a393e17d789d1e85c51bee898fc",
                    "block_slot": 32265099
                }
            })
        ),
        (status = 404, description = "No results found"),
        (status = 500, description = "Internal server error"),
    )
)]
#[tracing::instrument(name = "ACCOUNT_INFO", level = "info", skip(config, headers))]
/// Stake account information
///
/// Returns various information regarding a stake account
pub async fn account_info(
    headers: HeaderMap,
    Path(stake_addr): Path<String>,
    Extension(config): MapiExtension,
) -> Result<impl IntoResponse, ErrorResponse> {
    let dbsync = config.dbsync;
    let chain = config.chain_info;

    // --- get dbsync tip for last updated

    let last_updated = utils::get_last_updated_dbsync(&chain, &dbsync).await?;

    // --- fetch data from db

    let row = sqlx::query_as::<_, DbsyncAccountInfo>("SELECT * FROM grest.account_info_v2($1)")
        .bind(vec![stake_addr])
        .fetch_optional(&dbsync)
        .await
        .map_err(internal_server_error)?
        .ok_or_else(not_found)?;

    let info = AccountInfo {
        stake_address: row.stake_address,
        registered: row.registered,
        delegated_pool: row.delegated_pool,
        delegated_drep: row.delegated_drep,
        rewards_available: utils::u64_str_conv(row.rewards_available, &headers),
        utxo_balance: utils::u64_str_conv(row.utxo_balance, &headers),
        total_balance: utils::u64_str_conv(row.total_balance, &headers),
        total_rewarded: utils::u64_str_conv(row.total_rewarded, &headers),
        total_withdrawn: utils::u64_str_conv(row.total_withdrawn, &headers),
    };

    // ---

    let out = TimestampedResponse {
        data: info,
        last_updated,
    };

    Ok((StatusCode::OK, Json(out)))
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
/// Summary of information regarding a stake account
pub struct DbsyncAccountInfo {
    /// Bech32 encoded stake address
    pub stake_address: String,
    /// True if the stake key is registered
    pub registered: bool,
    /// Bech32 pool ID that the stake key is delegated to
    pub delegated_pool: Option<String>,
    /// Bech32 DRep ID that the stake key is delegated to
    pub delegated_drep: Option<String>,
    /// The amount of rewards that are available to be withdrawn
    pub rewards_available: u64,
    /// Amount locked in UTxOs controlled by addresses with the stake key
    pub utxo_balance: u64,
    /// Total balance controlled by the stake key (sum of UTxO and rewards)
    pub total_balance: u64,
    /// Total rewards earned
    pub total_rewarded: u64, // rewards
    /// Total rewards withdrawn
    pub total_withdrawn: u64,
}

impl<'r> FromRow<'r, PgRow> for DbsyncAccountInfo {
    fn from_row(row: &'r PgRow) -> Result<Self, sqlx::Error> {
        let stake_address = row.try_get("stake_address")?;
        let registered = row.try_get::<String, _>("status")? == "registered";
        let delegated_pool = row.try_get("delegated_pool")?;
        let delegated_drep = row.try_get("delegated_drep")?;
        let rewards_available = get_lovelace_from_string(row, "rewards_available")?;
        let utxo_balance = get_lovelace_from_string(row, "utxo")?;
        let total_balance = get_lovelace_from_string(row, "total_balance")?;
        let total_rewarded = get_lovelace_from_string(row, "rewards")?;
        let total_withdrawn = get_lovelace_from_string(row, "withdrawals")?;

        Ok(DbsyncAccountInfo {
            stake_address,
            registered,
            delegated_pool,
            delegated_drep,
            rewards_available,
            utxo_balance,
            total_balance,
            total_rewarded,
            total_withdrawn,
        })
    }
}
