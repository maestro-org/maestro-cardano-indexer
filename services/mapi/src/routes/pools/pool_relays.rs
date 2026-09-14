use axum::{extract::Path, http::StatusCode, response::IntoResponse, Extension, Json};

use crate::{
    responses::{self, ErrorResponse, PoolRelay, TimestampedResponse},
    utils::{self, internal_server_error},
    MapiExtension,
};

#[utoipa::path(
    tag = "Pools",
    get,
    path = "/pools/{pool_id}/relays",
    params(
        ("pool_id" = String, Path, description = "Pool ID in bech32 format"),
    ),
    responses(
        (
            status = 200,
            description = "List of relays declared on-chain by the specified pool",
            body = TimestampedPoolRelays,
            example = json!({
                "data": [
                    {
                        "pool_id_bech32": "pool10qrz84cvz95zg8saf43ruhs5ulczuuqm2d3jn6d8xkkkzgzfje7",
                        "relays": [
                            {
                                "dns": "relay1.adappt.online",
                                "srv": null,
                                "ipv4": null,
                                "ipv6": null,
                                "port": 3001
                            },
                            {
                                "dns": "relay2.adappt.online",
                                "srv": null,
                                "ipv4": null,
                                "ipv6": null,
                                "port": 3001
                            }
                        ]
                    }
                ],
                "last_updated": {
                    "timestamp": "2022-10-10 20:25:28",
                    "block_hash": "44d7fa145be596adba7e1a122cee3204bc1177c21598a3ef199925a13c042082",
                    "block_slot": 96405276
                }
            })
        ),
        (status = 500, description = "Internal server error"),
    )
)]
#[tracing::instrument(name = "POOL_RELAYS", level = "info", skip(config))]
/// Stake pool relays
///
/// Returns a list of relays declared on-chain by the specified stake pool
pub async fn pool_relays(
    Path(pool_id): Path<String>,
    Extension(config): MapiExtension,
) -> Result<impl IntoResponse, ErrorResponse> {
    let dbsync = config.dbsync;
    let chain = config.chain_info;

    // --- get dbsync tip for last updated

    let last_updated = utils::get_last_updated_dbsync(&chain, &dbsync).await?;

    // --- query data from dbsync

    let rows = sqlx::query_as::<_, PoolRelay>("SELECT * FROM grest.pool_relays()")
        .fetch_all(&dbsync)
        .await
        .map_err(internal_server_error)?;

    let filtered: Vec<PoolRelay> = rows
        .into_iter()
        .filter(|r| r.pool_id_bech32.eq(&pool_id))
        .collect();

    // ---

    let out = TimestampedResponse {
        data: responses::PoolRelays(filtered),
        last_updated,
    };

    Ok((StatusCode::OK, Json(out)))
}
