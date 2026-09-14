use lazy_static::lazy_static; // For global singleton
use r2d2::{Pool, PooledConnection};
use r2d2_redis2::redis::Commands;
use r2d2_redis2::RedisConnectionManager;
use redis::RedisResult;
use std::error::Error;
use std::fmt;
use std::sync::{Mutex, Once};
use strum_macros::EnumIter;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub network: Network,
    pub redis_address: String,
}

// Global configuration using lazy_static and Mutex for thread safety
lazy_static! {
    static ref CONFIG: Mutex<Option<AppConfig>> = Mutex::new(None);
}

// Helper function to initialize the configuration. This should only be called once.
static INIT: Once = Once::new();

pub fn initialize_config(network: Network, redis_address: String) {
    INIT.call_once(|| {
        let config_result = CONFIG.lock();
        match config_result {
            Ok(mut config) => {
                *config = Some(AppConfig {
                    network,
                    redis_address,
                });
            }
            Err(poisoned) => {
                // Handle the poisoned lock by continuing with the locked value
                let mut config = poisoned.into_inner();
                *config = Some(AppConfig {
                    network,
                    redis_address,
                });
            }
        }
    });
}

pub fn get_config() -> AppConfig {
    let config = CONFIG.lock().unwrap();
    config
        .clone()
        .expect("Configuration has not been initialized")
}

#[derive(Debug, Clone, Copy)]
pub enum Network {
    Mainnet,
    Preprod,
    Preview,
}

#[derive(Debug, Clone, Copy)]
pub enum InstanceType {
    AssetsPolicies,
    BalanceByAddress,
    BlockByHeight,
    BlockByTx,
    DatumByHash,
    TxByHash,
    TxCountByAddress,
    TxsByAddress,
    TxsByPaymentCred,
    TxsByPolicy,
    UtxoCborByAddress,
    UtxosByAsset,
    UtxosByPolicy,
}

impl fmt::Display for InstanceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InstanceType::AssetsPolicies => write!(f, "assets-policies"),
            InstanceType::BalanceByAddress => write!(f, "balancebyaddress"),
            InstanceType::BlockByHeight => write!(f, "blockbyheight"),
            InstanceType::BlockByTx => write!(f, "blockbytx"),
            InstanceType::DatumByHash => write!(f, "datumbyhash"),
            InstanceType::TxByHash => write!(f, "txbyhash"),
            InstanceType::TxCountByAddress => write!(f, "txcountbyaddress"),
            InstanceType::TxsByAddress => write!(f, "txsbyaddress"),
            InstanceType::TxsByPaymentCred => write!(f, "txsbypaymentcred"),
            InstanceType::TxsByPolicy => write!(f, "txsbypolicy"),
            InstanceType::UtxoCborByAddress => write!(f, "utxocborbyaddress"),
            InstanceType::UtxosByAsset => write!(f, "utxosbyasset"),
            InstanceType::UtxosByPolicy => write!(f, "utxosbypolicy"),
        }
    }
}

// Enum for the Cardano reducer types served by this API
#[derive(Debug, Clone, Copy, EnumIter, PartialEq)]
pub enum ReducerType {
    Cip25MetadataByAsset,
    MintMetadataByAsset,
    ScriptByHash,
    SupplyByAsset,
    UpdatesByPolicy,
    BalanceByAddress,
    BlockByHeight,
    BlockByTx,
    DatumByHash,
    TxByHash,
    TxCountByAddress,
    TxsByAddress,
    TxsByPayCred,
    TxsByPolicy,
    UtxoCborByAddress,
    UtxosByAsset,
    UtxosByPolicy,
}

impl fmt::Display for Network {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Network::Mainnet => write!(f, "mainnet"),
            Network::Preprod => write!(f, "preprod"),
            Network::Preview => write!(f, "preview"),
        }
    }
}

impl fmt::Display for ReducerType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReducerType::Cip25MetadataByAsset => write!(f, "Cip25MetadataByAsset"),
            ReducerType::MintMetadataByAsset => write!(f, "MintMetadataByAsset"),
            ReducerType::ScriptByHash => write!(f, "ScriptByHash"),
            ReducerType::SupplyByAsset => write!(f, "SupplyByAsset"),
            ReducerType::UpdatesByPolicy => write!(f, "UpdatesByPolicy"),
            ReducerType::BalanceByAddress => write!(f, "BalanceByAddress"),
            ReducerType::BlockByHeight => write!(f, "BlockByHeight"),
            ReducerType::BlockByTx => write!(f, "BlockByTx"),
            ReducerType::DatumByHash => write!(f, "DatumByHash"),
            ReducerType::TxByHash => write!(f, "TxByHash"),
            ReducerType::TxCountByAddress => write!(f, "TxCountByAddress"),
            ReducerType::TxsByAddress => write!(f, "TxsByAddress"),
            ReducerType::TxsByPayCred => write!(f, "TxsByPayCred"),
            ReducerType::TxsByPolicy => write!(f, "TxsByPolicy"),
            ReducerType::UtxoCborByAddress => write!(f, "UtxoCborByAddress"),
            ReducerType::UtxosByAsset => write!(f, "UtxosByAsset"),
            ReducerType::UtxosByPolicy => write!(f, "UtxosByPolicy"),
        }
    }
}

// Create a global Redis connection pool using lazy_static
lazy_static! {
    static ref REDIS_POOL: Pool<RedisConnectionManager> = {
        let manager = RedisConnectionManager::new(get_config().redis_address).expect("Failed to create Redis manager");
        Pool::builder()
            .max_size(15) // Set maximum connections in the pool
            .build(manager)
            .expect("Failed to create Redis connection pool")
    };
}

// Helper function to get a Redis connection from the pool
fn get_redis_connection() -> RedisResult<PooledConnection<RedisConnectionManager>> {
    REDIS_POOL.get().map_err(|e| {
        redis::RedisError::from((
            redis::ErrorKind::IoError,
            "Failed to get Redis connection from pool",
            e.to_string(),
        ))
    })
}

// Define a custom error type
#[derive(Debug)]
pub enum ResolveKeyError {
    RedisConnectionError(String),
    NoEntriesFound,
    EntryTooShort,
}

impl Error for ResolveKeyError {}

impl fmt::Display for ResolveKeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            ResolveKeyError::RedisConnectionError(ref e) => {
                write!(f, "Redis connection error: {}", e)
            }
            ResolveKeyError::NoEntriesFound => write!(f, "No entries found in the sorted set"),
            ResolveKeyError::EntryTooShort => write!(f, "Entry is too short"),
        }
    }
}

pub fn get_instance_for_reducer(reducer: ReducerType) -> InstanceType {
    match reducer {
        ReducerType::BalanceByAddress => InstanceType::BalanceByAddress,
        ReducerType::BlockByHeight => InstanceType::BlockByHeight,
        ReducerType::BlockByTx => InstanceType::BlockByTx,
        ReducerType::DatumByHash => InstanceType::DatumByHash,
        ReducerType::TxByHash => InstanceType::TxByHash,
        ReducerType::TxCountByAddress => InstanceType::TxCountByAddress,
        ReducerType::TxsByAddress => InstanceType::TxsByAddress,
        ReducerType::TxsByPayCred => InstanceType::TxsByPaymentCred,
        ReducerType::TxsByPolicy => InstanceType::TxsByPolicy,
        ReducerType::UtxoCborByAddress => InstanceType::UtxoCborByAddress,
        ReducerType::UtxosByAsset => InstanceType::UtxosByAsset,
        ReducerType::UtxosByPolicy => InstanceType::UtxosByPolicy,
        ReducerType::Cip25MetadataByAsset => InstanceType::AssetsPolicies,
        ReducerType::MintMetadataByAsset => InstanceType::AssetsPolicies,
        ReducerType::ScriptByHash => InstanceType::AssetsPolicies,
        ReducerType::SupplyByAsset => InstanceType::AssetsPolicies,
        ReducerType::UpdatesByPolicy => InstanceType::AssetsPolicies,
    }
}

// Function to resolve the key, now returning a generic `Result`
pub fn resolve_key(reducer: ReducerType) -> Result<(u8, u8), ResolveKeyError> {
    let config = get_config();

    let instance = get_instance_for_reducer(reducer);

    // Construct the Redis key
    let key = format!("cardano:{}:{}:scores", config.network, instance);

    tracing::debug!("Retrieving key: {} from redis...", key);

    // Get a Redis connection from the pool
    let mut con =
        get_redis_connection().map_err(|e| ResolveKeyError::RedisConnectionError(e.to_string()))?;

    // Fetch the first entry from the sorted set in Redis
    let first_entry: Result<Vec<Vec<u8>>, _> = con.zrange(key, 0, 0);
    match first_entry {
        Ok(entries) => {
            if let Some(entry) = entries.first() {
                if entry.len() >= 2 {
                    let dataplane_id = entry[0]; // First byte is the Dataplane ID
                    let instance_id = entry[1]; // Second byte is the Instance ID
                    tracing::debug!(
                        "{} => dataplane_id: {} instance_id: {}",
                        reducer,
                        dataplane_id,
                        instance_id
                    );
                    Ok((dataplane_id, instance_id))
                } else {
                    Err(ResolveKeyError::EntryTooShort)
                }
            } else {
                Err(ResolveKeyError::NoEntriesFound)
            }
        }
        Err(e) => Err(ResolveKeyError::RedisConnectionError(e.to_string())),
    }
}
