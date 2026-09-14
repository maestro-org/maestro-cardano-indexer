use axum::{http::StatusCode, response::IntoResponse, Extension, Json};

use crate::{
    responses::{CurrentEpochInfo, ErrorResponse, TimestampedResponse},
    utils::{self, internal_server_error},
    MapiExtension,
};

#[utoipa::path(
    tag = "Epochs",
    get,
    path = "/epochs/current",
    responses(
        (
            status = 200,
            description = "Information about the current epoch",
            body = TimestampedCurrentEpochInfo,
            example = json!({
                "data": {
                    "epoch_no": 78,
                    "fees": "8689785871",
                    "tx_count": 15413,
                    "blk_count": 9571,
                    "start_time": 1687737655
                },
                "last_updated": {
                    "timestamp": "2022-10-10 20:25:28",
                    "block_hash": "bb35643eb0da04a3965b15db86edfaa8b4b9eba0f2dad77edf8e0250f41f2de1",
                    "block_slot": 32284313
                }
            })
        ),
        (status = 500, description = "Internal server error"),
    )
)]
#[tracing::instrument(name = "CURRENT_EPOCH", level = "info", skip(config))]
/// Current epoch details
///
/// Returns a summary of information about the current epoch
pub async fn current_epoch(
    Extension(config): MapiExtension,
) -> Result<impl IntoResponse, ErrorResponse> {
    let dbsync = config.dbsync;
    let chain = config.chain_info;

    // --- get dbsync tip for last updated

    let last_updated = utils::get_last_updated_dbsync(&chain, &dbsync).await?;

    // --- query data from dbsync

    let epoch =
        sqlx::query_as::<_, CurrentEpochInfo>("SELECT * FROM epoch ORDER BY no DESC LIMIT 1")
            .fetch_one(&dbsync)
            .await
            .map_err(internal_server_error)?;

    // ---

    let out = TimestampedResponse {
        data: epoch,
        last_updated,
    };

    Ok((StatusCode::OK, Json(out)))
}
