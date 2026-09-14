use std::ops::Deref;

use axum::{
    extract::Path,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgRow, types::BigDecimal, types::Json as SqlxJson, FromRow, Row};

use crate::{
    responses::{
        ErrorResponse, MapiDecodeError, PoolInfo, PoolMetaJson, Relay, TimestampedResponse,
    },
    utils::{
        self, get_lovelace_from_string, get_maybe_lovelace_from_string,
        get_maybe_string_from_numeric, internal_server_error, not_found,
    },
    MapiExtension,
};

#[utoipa::path(
    tag = "Pools",
    get,
    path = "/pools/{pool_id}/info",
    params(
        ("pool_id" = String, Path, description = "Pool ID in bech32 format"),

        ("amounts-as-strings" = Option<String>, Header, description = "Large numbers returned as strings if set to `true`")
    ),
    responses(
        (
            status = 200,
            description = "Current information regarding the specified pool",
            body = TimestampedPoolInfo,
            example = json!({
                "data": {
                    "pool_id_bech32": "pool174mw7e20768e8vj4fn8y6p536n8rkzswsapwtwn354dckpjqzr8",
                    "pool_id_hex": "f576ef654ff68f93b2554cce4d0691d4ce3b0a0e8742e5ba71a55b8b",
                    "active_epoch_no": 6,
                    "vrf_key_hash": "352196224497a0fd7bad52d113767660bf70f8b11a8c40c265f7bfb359ebe9ee",
                    "margin": 1,
                    "fixed_cost": 500000000,
                    "pledge": 100000000000000i64,
                    "reward_addr": "stake_test1uzkdwx64sjkt6xxtzye00y3k2m9wn5zultsguadaf4ggmssadyunp",
                    "owners": [
                        "stake_test1urcnqgzt2x8hpsvej4zfudehahknm8lux894pmqwg5qshgcrn346q"
                    ],
                    "relays": [
                        {
                            "dns": "preprod-node.world.dev.cardano.org",
                            "srv": null,
                            "ipv4": null,
                            "ipv6": null,
                            "port": 30000
                        }
                    ],
                    "meta_url": null,
                    "meta_hash": null,
                    "meta_json": null,
                    "pool_status": "registered",
                    "retiring_epoch": null,
                    "op_cert": "4838a036e2540d83eea0d28626f41566dbaf6d263bc9ad95e2ff89740a2a1b65",
                    "op_cert_counter": 5,
                    "active_stake": 1037439267850i64,
                    "sigma": "0.00432904657047742993",
                    "block_count": 145101,
                    "live_pledge": 1000053629698i64,
                    "live_stake": 1037439267850i64,
                    "live_delegators": 13,
                    "live_saturation": "1.7000"
                },
                "last_updated": {
                    "timestamp": "2022-10-10 20:25:28",
                    "block_hash": "5daa79fe4bf6581b59e5ebebb7a0ac6b2edd3ade5016e9db09ac67a7661010e5",
                    "block_slot": 32288194
                }
            })
        ),
        (status = 404, description = "No results found"),
        (status = 500, description = "Internal server error"),
    )
)]
#[tracing::instrument(name = "POOL_INFO", level = "info", skip(config, headers))]
/// Stake pool information
///
/// Returns current information about the specified pool
pub async fn pool_info(
    headers: HeaderMap,
    Path(pool_id): Path<String>,
    Extension(config): MapiExtension,
) -> Result<impl IntoResponse, ErrorResponse> {
    let dbsync = config.dbsync;
    let chain = config.chain_info;

    // --- get dbsync tip for last updated

    let last_updated = utils::get_last_updated_dbsync(&chain, &dbsync).await?;

    // --- query data from dbsync

    let info = sqlx::query_as::<_, DbsyncPoolInfo>("SELECT * FROM grest.pool_info($1)")
        .bind(vec![pool_id])
        .fetch_optional(&dbsync)
        .await
        .map_err(internal_server_error)?
        .ok_or_else(not_found)?;

    let fetched_md = match (info.meta_url.clone(), info.meta_hash.clone()) {
        (Some(url), Some(hash)) => utils::fetch_pool_metadata(url, hash).await,
        (_, _) => None,
    };

    let info = PoolInfo {
        pool_id_bech32: info.pool_id_bech32,
        pool_id_hex: info.pool_id_hex,
        active_epoch_no: info.active_epoch_no,
        vrf_key_hash: info.vrf_key_hash,
        margin: utils::f64_str_conv(info.margin, &headers),
        fixed_cost: utils::u64_str_conv(info.fixed_cost, &headers),
        pledge: utils::u64_str_conv(info.pledge, &headers),
        reward_addr: info.reward_addr,
        owners: info.owners,
        relays: info.relays,
        meta_url: info.meta_url,
        meta_hash: info.meta_hash,
        meta_json: fetched_md,
        pool_status: info.pool_status,
        retiring_epoch: info.retiring_epoch,
        op_cert: info.op_cert,
        op_cert_counter: info.op_cert_counter,
        active_stake: utils::u64_str_conv(info.active_stake.unwrap_or(0), &headers),
        sigma: info.sigma,
        block_count: info.block_count,
        live_pledge: utils::u64_str_conv(info.live_pledge.unwrap_or(0), &headers),
        live_stake: utils::u64_str_conv(info.live_stake.unwrap_or(0), &headers),
        live_delegators: info.live_delegators,
        live_saturation: info.live_saturation,
    };

    // ---

    let out = TimestampedResponse {
        data: info,
        last_updated,
    };

    Ok((StatusCode::OK, Json(out)))
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
/// Information summary of a stake pool
pub struct DbsyncPoolInfo {
    /// Bech32 encoded pool ID
    pool_id_bech32: String,
    /// Hex encoded pool ID
    pool_id_hex: String,
    /// Epoch when the update takes effect
    active_epoch_no: i64,
    /// VRF key hash
    pub vrf_key_hash: Option<String>,
    /// Pool margin
    margin: f64,
    /// Pool fixed cost
    fixed_cost: u64,
    /// Pool pledge
    pledge: u64,
    /// Reward address associated with the pool
    reward_addr: Option<String>,
    /// List of stake keys which control the pool
    owners: Vec<String>,
    /// Relays declared by the pool
    relays: Vec<Relay>,
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
    /// Pool operational certificate
    op_cert: Option<String>,
    /// Operational certificate counter
    op_cert_counter: Option<i64>,
    /// Active stake
    active_stake: Option<u64>,
    /// Pool stake share
    sigma: Option<String>,
    /// Number of blocks created
    block_count: Option<u64>,
    /// Account balance of pool owners
    live_pledge: Option<u64>,
    /// Live stake
    live_stake: Option<u64>,
    /// Number of current delegators
    live_delegators: i64,
    /// Live saturation
    live_saturation: Option<String>,
}

impl<'r> FromRow<'r, PgRow> for DbsyncPoolInfo {
    fn from_row(row: &'r PgRow) -> Result<Self, sqlx::Error> {
        let pool_id_bech32 = row.try_get("pool_id_bech32")?;
        let pool_id_hex = row.try_get("pool_id_hex")?;
        let active_epoch_no = row.try_get("active_epoch_no")?;
        let vrf_key_hash = row.try_get("vrf_key_hash")?;
        let margin = row.try_get("margin")?;
        let fixed_cost = get_lovelace_from_string(row, "fixed_cost")?;
        let pledge = get_lovelace_from_string(row, "pledge")?;
        let reward_addr = row.try_get("reward_addr")?;
        let owners = row.try_get("owners")?;

        let relays = row
            .try_get::<Vec<SqlxJson<Relay>>, _>("relays")?
            .iter()
            .map(|x| x.deref().clone())
            .collect();

        let meta_url = row.try_get("meta_url")?;
        let meta_hash = row.try_get("meta_hash")?;

        let meta_json = row
            .try_get::<Option<SqlxJson<PoolMetaJson>>, _>("meta_json")?
            .map(|x| x.deref().clone());

        let pool_status = row.try_get("pool_status")?;
        let retiring_epoch = row.try_get("retiring_epoch")?;
        let op_cert = row.try_get("op_cert")?;
        let op_cert_counter = row.try_get("op_cert_counter")?;
        let active_stake = get_maybe_lovelace_from_string(row, "active_stake")?;
        let sigma = get_maybe_string_from_numeric(row, "sigma")?;

        // parse postgres numeric into u64 (block_count is numeric but will always be int?)
        let block_count: Option<u64> = row
            .try_get::<Option<BigDecimal>, _>("block_count")?
            .map(|x| x.into_bigint_and_exponent().0)
            .map(|b| b.try_into())
            .transpose()
            .map_err(|_| sqlx::Error::Decode(anyhow::Error::new(MapiDecodeError).into()))?;

        let live_pledge = get_maybe_lovelace_from_string(row, "live_pledge")?;
        let live_stake = get_maybe_lovelace_from_string(row, "live_stake")?;
        let live_delegators = row.try_get("live_delegators")?;
        let live_saturation = get_maybe_string_from_numeric(row, "live_saturation")?;

        Ok(DbsyncPoolInfo {
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
            op_cert,
            op_cert_counter,
            active_stake,
            sigma,
            block_count,
            live_pledge,
            live_stake,
            live_delegators,
            live_saturation,
        })
    }
}
