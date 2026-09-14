use axum::{http::StatusCode, response::IntoResponse, Extension, Json};
use sqlx::types::chrono;

use crate::{
    responses::{self, ErrorResponse, TimestampedResponse},
    utils::{self, internal_server_error},
    MapiExtension,
};

#[utoipa::path(
    tag = "General",
    get,
    path = "/system-start",
    responses(
        (
            status = 200,
            description = "Get system start time",
            body = TimestampedSystemStart,
            example = json!({
                "data": "2017-09-23 21:44:51",
                "last_updated": {
                    "timestamp": "2022-10-10 20:25:28",
                    "block_hash": "2e220d7adfe5b9e6444894ae237679c29d71fc4b44cc36aa9156262b4bb4c159",
                    "block_slot": 96402974
                }
            })
        ),
        (status = 500, description = "Internal server error"),
    )
)]
#[tracing::instrument(name = "SYSTEM_START", level = "info", skip(config))]
/// Blockchain system start
///
/// Returns the blockchain system start time
pub async fn system_start(
    Extension(config): MapiExtension,
) -> Result<impl IntoResponse, ErrorResponse> {
    let dbsync = config.dbsync;
    let chain = config.chain_info;

    // --- get dbsync tip for last updated

    let last_updated = utils::get_last_updated_dbsync(&chain, &dbsync).await?;

    // --- query data from dbsync

    let time: chrono::NaiveDateTime = sqlx::query_scalar("SELECT start_time FROM meta LIMIT 1")
        .fetch_one(&dbsync)
        .await
        .map_err(internal_server_error)?;

    // ---

    let out = TimestampedResponse {
        data: responses::SystemStart(time.to_string()),
        last_updated,
    };

    Ok((StatusCode::OK, Json(out)))
}
