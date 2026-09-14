use axum::{extract::Path, http::StatusCode, response::IntoResponse, Extension, Json};

use crate::{
    responses::{EpochInfo, ErrorResponse, TimestampedResponse},
    utils::{self, internal_server_error, not_found},
    MapiExtension,
};

#[utoipa::path(
    tag = "Epochs",
    get,
    path = "/epochs/{epoch_no}",
    params(
        ("epoch_no" = i32, Path, description = "Epoch number to return information about"),
    ),
    responses(
        (
            status = 200,
            description = "Information about the requested epoch",
            body = TimestampedEpochInfo,
            example = json!({
                "data": {
                    "epoch_no": 448,
                    "fees": "102017010232",
                    "tx_count": 293800,
                    "blk_count": 21133,
                    "start_time": 1699739174,
                    "end_time": 1700171024,
                    "active_stake": "22973647279096705",
                    "total_rewards": "10027944051413",
                    "average_reward": "474515878"
                },
                "last_updated": {
                    "timestamp": "2023-11-23 13:42:12",
                    "block_hash": "89e2f22f2441e7545bfd7b6f01a23be3b804c7cad4eea7dd2c989a679fa0138a",
                    "block_slot": 109180641
                }
            })
        ),
        (status = 404, description = "No results found"),
        (status = 500, description = "Internal server error"),
    )
)]
#[tracing::instrument(name = "EPOCH_INFO", level = "info", skip(config))]
/// Specific epoch details
///
/// Returns a summary of information about a specific epoch
pub async fn epoch_info(
    Path(epoch_no): Path<i32>,
    Extension(config): MapiExtension,
) -> Result<impl IntoResponse, ErrorResponse> {
    let dbsync = config.dbsync;
    let chain = config.chain_info;

    // --- get dbsync tip for last updated

    let last_updated = utils::get_last_updated_dbsync(&chain, &dbsync).await?;

    // --- query data from dbsync

    let mut epoch = sqlx::query_as::<_, EpochInfo>("SELECT * FROM epoch WHERE no = ($1)")
        .bind(epoch_no)
        .fetch_optional(&dbsync)
        .await
        .map_err(internal_server_error)?
        .ok_or_else(not_found)?;

    // --- then fetch some info from koios

    let (active_stake, total_rewards, average_reward) = sqlx::query_as(
        "SELECT active_stake, total_rewards, avg_blk_reward
            FROM grest.epoch_info($1)",
    )
    .bind(epoch_no)
    .fetch_one(&dbsync)
    .await
    .map_err(internal_server_error)?;

    epoch.active_stake = active_stake;
    epoch.total_rewards = total_rewards;
    epoch.average_reward = average_reward;

    // ---

    let out = TimestampedResponse {
        data: epoch,
        last_updated,
    };

    Ok((StatusCode::OK, Json(out)))
}
