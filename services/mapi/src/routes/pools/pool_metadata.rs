use axum::{extract::Path, http::StatusCode, response::IntoResponse, Extension, Json};

use crate::{
    responses::{ErrorResponse, PoolMetadata, TimestampedResponse},
    utils::{self, internal_server_error, not_found},
    MapiExtension,
};

#[utoipa::path(
    tag = "Pools",
    get,
    path = "/pools/{pool_id}/metadata",
    params(
        ("pool_id" = String, Path, description = "Pool ID in bech32 format"),
    ),
    responses(
        (
            status = 200,
            description = "Various metadata for the specified pool",
            body = TimestampedPoolMetadata,
            example = json!({
                "data": {
                    "pool_id_bech32": "pool10qrz84cvz95zg8saf43ruhs5ulczuuqm2d3jn6d8xkkkzgzfje7",
                    "meta_url": "https://adappt.online/pool_metadata.json",
                    "meta_hash": "be9033a437ba8e8c9ad7e072e71b1afca66ab18225d39d5e9de3d38e1e088f8d",
                    "meta_json": {
                        "name": "Adappt Online",
                        "ticker": "ADAPT",
                        "homepage": "https://adappt.online",
                        "description": "Adappt Online pool supports Cardano and the concept of DApps. On-premise servers with 24/7 monitoring."
                    }
                },
                "last_updated": {
                    "timestamp": "2022-10-10 20:25:28",
                    "block_hash": "d881a61a3573d2fdd0098b9506da74b36f23d67e797f55419dd7eb96a8b327fc",
                    "block_slot": 96405212
                }
            })
        ),
        (status = 404, description = "No results found"),
        (status = 500, description = "Internal server error"),
    )
)]
#[tracing::instrument(name = "POOL_METADATA", level = "info", skip(config))]
/// Stake pool metadata
///
/// Returns the metadata declared on-chain by the specified stake pool
pub async fn pool_metadata(
    Path(pool_id): Path<String>,
    Extension(config): MapiExtension,
) -> Result<impl IntoResponse, ErrorResponse> {
    let dbsync = config.dbsync;
    let chain = config.chain_info;

    // --- get dbsync tip for last updated

    let last_updated = utils::get_last_updated_dbsync(&chain, &dbsync).await?;

    // --- query data from dbsync

    let mut md = sqlx::query_as::<_, PoolMetadata>("SELECT * FROM grest.pool_metadata($1)")
        .bind(vec![pool_id])
        .fetch_optional(&dbsync)
        .await
        .map_err(internal_server_error)?
        .ok_or_else(not_found)?;

    let fetched_md = match (md.meta_url.clone(), md.meta_hash.clone()) {
        (Some(url), Some(hash)) => utils::fetch_pool_metadata(url, hash).await,
        (_, _) => None,
    };

    md.meta_json = fetched_md;

    // ---

    let out = TimestampedResponse {
        data: md,
        last_updated,
    };

    Ok((StatusCode::OK, Json(out)))
}
