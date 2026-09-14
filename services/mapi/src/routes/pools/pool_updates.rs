use std::ops::Deref;

use axum::{
    extract::Path,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgRow, types::Json as SqlxJson, FromRow, Row};

use crate::{
    responses::{self, ErrorResponse, PoolMetaJson, PoolUpdate, Relay, TimestampedResponse},
    utils::{self, get_maybe_lovelace_from_string, internal_server_error},
    MapiExtension,
};

#[utoipa::path(
    tag = "Pools",
    get,
    path = "/pools/{pool_id}/updates",
    params(
        ("pool_id" = String, Path, description = "Pool ID in bech32 format"),

        ("amounts-as-strings" = Option<String>, Header, description = "Large numbers returned as strings if set to `true`")
    ),
    responses(
        (
            status = 200,
            description = "List of pool updates relating to the specified pool",
            body = TimestampedPoolUpdates,
            example = json!({
                "data": [
                    {
                        "tx_hash": "877091a2e789309eecd7d6a15721c6ad8ffdfdbdf8e1fd172f81ce54b7e26f15",
                        "block_time": 1619787462,
                        "pool_id_bech32": "pool10qrz84cvz95zg8saf43ruhs5ulczuuqm2d3jn6d8xkkkzgzfje7",
                        "pool_id_hex": "780623d70c1168241e1d4d623e5e14e7f02e701b536329e9a735ad61",
                        "active_epoch_no": 265,
                        "vrf_key_hash": "66b11dc1264c5758a202ecea777d12329d6ca3ff111e9a161026d4ccd782238f",
                        "margin": 0.015,
                        "fixed_cost": 340000000,
                        "pledge": 44325545672i64,
                        "reward_addr": "stake1u9dh8mls26d6aq68v5nmmkeh5eq563m3w5mnytqppl6l66swz236x",
                        "owners": [
                            "stake1u9dh8mls26d6aq68v5nmmkeh5eq563m3w5mnytqppl6l66swz236x"
                        ],
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
                        ],
                        "meta_url": "https://adappt.online/pool_metadata.json",
                        "meta_hash": "be9033a437ba8e8c9ad7e072e71b1afca66ab18225d39d5e9de3d38e1e088f8d",
                        "meta_json": {
                            "name": "Adappt Online",
                            "ticker": "ADAPT",
                            "homepage": "https://adappt.online",
                            "description": "Adappt Online pool supports Cardano and the concept of DApps. On-premise servers with 24/7 monitoring."
                        },
                        "pool_status": "registered",
                        "retiring_epoch": null
                    },
                    {
                        "tx_hash": "d4c54ce8b1edc60a18914a461c555d9784f8c6599321926302df31008b0c6c02",
                        "block_time": 1596465191,
                        "pool_id_bech32": "pool10qrz84cvz95zg8saf43ruhs5ulczuuqm2d3jn6d8xkkkzgzfje7",
                        "pool_id_hex": "780623d70c1168241e1d4d623e5e14e7f02e701b536329e9a735ad61",
                        "active_epoch_no": 210,
                        "vrf_key_hash": "66b11dc1264c5758a202ecea777d12329d6ca3ff111e9a161026d4ccd782238f",
                        "margin": 0.03,
                        "fixed_cost": 340000000,
                        "pledge": 2061750060728i64,
                        "reward_addr": "stake1u9dh8mls26d6aq68v5nmmkeh5eq563m3w5mnytqppl6l66swz236x",
                        "owners": [
                            "stake1u9dh8mls26d6aq68v5nmmkeh5eq563m3w5mnytqppl6l66swz236x",
                            "stake1u95f0rfdr6phfpym6gn5rcfes66dkht0u49d8uhhq53agwc62e5xj",
                            "stake1u8px8vy3lwk8me2umaf2n06jx6q0ghaa0vkehjtg5rqm3yqtckhmm"
                        ],
                        "relays": [
                            {
                                "dns": "adappt.online",
                                "srv": null,
                                "ipv4": null,
                                "ipv6": null,
                                "port": 3001
                            }
                        ],
                        "meta_url": "https://adappt.online/pool_metadata.json",
                        "meta_hash": "5a040f5e31f4fef8030fb9f996ac2a7f19852c071dcfd1da6fa0a8416d8fea5f",
                        "meta_json": null,
                        "pool_status": "registered",
                        "retiring_epoch": null
                    }
                ],
                "last_updated": {
                    "timestamp": "2022-10-10 20:25:28",
                    "block_hash": "4cede7843465d55e260722845c3694970500def738245d340501dfc02ae11517",
                    "block_slot": 96405387
                }
            })
        ),
        (status = 500, description = "Internal server error"),
    )
)]
#[tracing::instrument(name = "POOL_UPDATES", level = "info", skip(config, headers))]
/// Stake pool updates
///
/// Returns a list of updates relating to the specified pool
pub async fn pool_updates(
    headers: HeaderMap,
    Path(pool_id): Path<String>,
    Extension(config): MapiExtension,
) -> Result<impl IntoResponse, ErrorResponse> {
    let dbsync = config.dbsync;
    let chain = config.chain_info;

    // --- get dbsync tip for last updated

    let last_updated = utils::get_last_updated_dbsync(&chain, &dbsync).await?;

    // --- query data from dbsync

    let dbs_updates = sqlx::query_as::<_, DbsyncPoolUpdate>("SELECT * FROM grest.pool_updates($1)")
        .bind(pool_id)
        .fetch_all(&dbsync)
        .await
        .map_err(internal_server_error)?;

    let mut updates = vec![];

    for update in dbs_updates {
        updates.push(PoolUpdate {
            tx_hash: update.tx_hash,
            block_time: update.block_time,
            pool_id_bech32: update.pool_id_bech32,
            pool_id_hex: update.pool_id_hex,
            active_epoch_no: update.active_epoch_no,
            vrf_key_hash: update.vrf_key_hash,
            margin: utils::f64_str_conv(update.margin.unwrap_or(0.0), &headers),
            fixed_cost: utils::u64_str_conv(update.fixed_cost.unwrap_or(0), &headers),
            pledge: utils::u64_str_conv(update.pledge.unwrap_or(0), &headers),
            reward_addr: update.reward_addr,
            owners: update.owners,
            relays: update.relays,
            meta_url: update.meta_url,
            meta_hash: update.meta_hash,
            meta_json: update.meta_json,
            pool_status: update.pool_status,
            retiring_epoch: update.retiring_epoch,
        })
    }

    // ---

    let out = TimestampedResponse {
        data: responses::PoolUpdates(updates),
        last_updated,
    };

    Ok((StatusCode::OK, Json(out)))
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
/// Update to a stake pool
pub struct DbsyncPoolUpdate {
    /// Transaction hash for the transaction which contained the update
    tx_hash: String,
    /// UNIX timestamp of the block containing the transaction
    block_time: Option<i32>,
    /// Bech32 encoded pool ID
    pool_id_bech32: String,
    /// Hex encoded pool ID
    pool_id_hex: String,
    /// Epoch when the update takes effect
    active_epoch_no: Option<i64>,
    /// VRF key hash
    vrf_key_hash: Option<String>,
    /// Pool margin
    margin: Option<f64>,
    /// Pool fixed cost
    fixed_cost: Option<u64>,
    /// Pool pledge
    pledge: Option<u64>,
    /// Reward address associated with the pool
    reward_addr: Option<String>,
    /// List of stake keys which control the pool
    owners: Option<Vec<String>>,
    /// Relays declared by the pool
    relays: Option<Vec<Relay>>,
    /// URL pointing to the pool metadata
    meta_url: Option<String>,
    /// Hash of the pool metadata
    meta_hash: Option<String>,
    /// JSON representation of the pool metadata
    meta_json: Option<PoolMetaJson>,
    /// Status of the pool
    pool_status: Option<String>,
    /// Epoch at which the pool will be retired
    retiring_epoch: Option<i32>,
}

impl<'r> FromRow<'r, PgRow> for DbsyncPoolUpdate {
    fn from_row(row: &'r PgRow) -> Result<Self, sqlx::Error> {
        let tx_hash = row.try_get("tx_hash")?;
        let block_time = row.try_get("block_time")?;
        let pool_id_bech32 = row.try_get("pool_id_bech32")?;
        let pool_id_hex = row.try_get("pool_id_hex")?;
        let active_epoch_no = row.try_get("active_epoch_no")?;
        let vrf_key_hash = row.try_get("vrf_key_hash")?;
        let margin = row.try_get("margin")?;
        let fixed_cost = get_maybe_lovelace_from_string(row, "fixed_cost")?;
        let pledge = get_maybe_lovelace_from_string(row, "pledge")?;
        let reward_addr = row.try_get("reward_addr")?;

        let owners = row
            .try_get::<Option<SqlxJson<Vec<String>>>, _>("owners")?
            .map(|x| x.deref().clone());

        let relays = row
            .try_get::<Option<SqlxJson<Vec<Relay>>>, _>("relays")?
            .map(|x| x.deref().clone());

        let meta_url = row.try_get("meta_url")?;
        let meta_hash = row.try_get("meta_hash")?;

        let meta_json = row
            .try_get::<Option<SqlxJson<PoolMetaJson>>, _>("meta_json")?
            .map(|x| x.deref().clone());

        let pool_status = row.try_get("update_type")?;
        let retiring_epoch = row.try_get("retiring_epoch")?;

        Ok(DbsyncPoolUpdate {
            tx_hash,
            block_time,
            pool_id_bech32,
            pool_id_hex,
            active_epoch_no,
            vrf_key_hash,
            margin,
            fixed_cost,
            pledge,
            reward_addr,
            owners,
            relays,
            meta_url,
            meta_hash,
            meta_json,
            pool_status,
            retiring_epoch,
        })
    }
}
