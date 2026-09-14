use serde::Deserialize;
use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    pub tikv_pd: String,
    pub tikv_max_connections: u32,
    pub network: String,
    pub redis: String,
    pub mapi_port: u16,
    pub dbsync_db_url: String,
    pub dbsync_max_connections: u32,
    pub ogmios_v6_url: String,
    pub scan_timeout_millis: u64,
    pub ogmios_cache_ttl_secs: u64,
    pub ogmios_query_timeout_millis: u64,
    pub ogmios_max_retries: u32,
    // The token registry keyspace is fed by an external ingestor rather than the
    // dataplane-managed reducers, so its IDs come from configuration. Both are
    // optional: when either is unset, token-registry lookups are disabled and the
    // API serves `null` token-registry metadata.
    pub token_registry_dataplane_id: Option<u8>,
    pub token_registry_instance_id: Option<u8>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            tikv_pd: get_var("TIKV_PD_CLIENT", "127.0.0.1:2379"),
            tikv_max_connections: get_var("TIKV_MAX_CONNECTIONS", "5")
                .parse()
                .expect("TIKV_MAX_CONNECTIONS should be a number"),
            network: get_var("NETWORK", "mainnet"),
            redis: get_required_var("REDIS"),
            mapi_port: get_var("MAPI_PORT", "4000")
                .parse()
                .expect("MAPI_PORT should be a number"),
            dbsync_db_url: get_var("DBSYNC_DB_URL", "postgres://cexplorer@127.0.0.1"),
            dbsync_max_connections: get_var("DBSYNC_MAX_CONNECTIONS", "5")
                .parse()
                .expect("DBSYNC_MAX_CONNECTIONS should be a number"),
            ogmios_v6_url: get_var("OGMIOS_V6_URL", "ws://127.0.0.1:1338"),
            scan_timeout_millis: get_var("SCAN_TIMEOUT_MILLIS", "5000")
                .parse()
                .expect("SCAN_TIMEOUT_MILLIS should be a number"),
            ogmios_cache_ttl_secs: {
                // Must be > 0: a zero TTL stores an immediately-stale value, turning every
                // request into a miss that serializes through the cache's single-flight lock.
                let secs: u64 = get_var("OGMIOS_CACHE_TTL_SECS", "60")
                    .parse()
                    .expect("OGMIOS_CACHE_TTL_SECS should be a number");
                assert!(secs > 0, "OGMIOS_CACHE_TTL_SECS must be greater than 0");
                secs
            },
            ogmios_query_timeout_millis: {
                // Must be > 0: a zero per-attempt timeout elapses before a websocket connect
                // can complete, so every ogmios query would fail as a timeout (504).
                let millis: u64 = get_var("OGMIOS_QUERY_TIMEOUT_MILLIS", "10000")
                    .parse()
                    .expect("OGMIOS_QUERY_TIMEOUT_MILLIS should be a number");
                assert!(
                    millis > 0,
                    "OGMIOS_QUERY_TIMEOUT_MILLIS must be greater than 0"
                );
                millis
            },
            ogmios_max_retries: {
                // Capped: each refresh runs up to `max_retries + 1` attempts while holding the
                // cache's single-flight lock, so an outsized value (e.g. a typo) would block
                // every queued caller for `max_retries * timeout`. 10 is far above any sane
                // retry count while still bounding that worst case.
                let retries: u32 = get_var("OGMIOS_MAX_RETRIES", "2")
                    .parse()
                    .expect("OGMIOS_MAX_RETRIES should be a number");
                assert!(retries <= 10, "OGMIOS_MAX_RETRIES must be <= 10");
                retries
            },
            token_registry_dataplane_id: get_optional_var("TOKEN_REGISTRY_DATAPLANE_ID").map(|v| {
                v.parse()
                    .expect("TOKEN_REGISTRY_DATAPLANE_ID should be a number")
            }),
            token_registry_instance_id: get_optional_var("TOKEN_REGISTRY_INSTANCE_ID").map(|v| {
                v.parse()
                    .expect("TOKEN_REGISTRY_INSTANCE_ID should be a number")
            }),
        }
    }
}

fn get_var(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|e| {
        tracing::debug!("{} {}, using default: {}", e, key, default);

        String::from(default)
    })
}

fn get_required_var(key: &str) -> String {
    match env::var(key) {
        Ok(value) => value,
        Err(e) => {
            tracing::error!("Environment variable {} not found: {}", key, e);
            panic!("Missing required environment variable: {}", key);
        }
    }
}

fn get_optional_var(key: &str) -> Option<String> {
    match env::var(key) {
        Ok(value) => Some(value),
        Err(_) => {
            tracing::debug!("Optional environment variable {} not set", key);
            None
        }
    }
}

#[derive(Deserialize)]
pub struct ReducerInstances {
    pub block_by_height: u8,
    pub datum_by_hash: u8,
    pub tx_by_hash: u8,
    pub tx_count_by_address: u8,
    pub txs_by_address: u8,
    pub txs_by_pay_cred: u8,
    pub utxo_cbor_by_address: u8,
    pub utxos_by_asset: u8,
    pub utxos_by_policy: u8,
    pub script_by_hash: u8,
    pub supply_by_asset: u8,
    pub updates_by_policy: u8,
    pub mint_metadata_by_asset: u8,
    pub txs_by_policy: u8,
    pub cip25_metadata_by_asset: u8,
    pub token_registry: u8,
    pub block_by_tx: u8,
}

impl ReducerInstances {
    pub fn new(file: String) -> Result<Self, config::ConfigError> {
        let mut s = config::Config::builder();

        s = s.add_source(config::File::with_name(&file).required(true));

        s.build()?.try_deserialize()
    }
}
