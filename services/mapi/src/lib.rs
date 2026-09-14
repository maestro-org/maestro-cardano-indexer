#![recursion_limit = "1024"]
use std::sync::Arc;
use std::time::Duration;

use crate::config::Config;
use axum::{http::StatusCode, response::IntoResponse, routing::get, Extension, Json, Router};
use cache::TtlCache;
use chain::{mainnet_chain_info, preprod_chain_info, preview_chain_info, ChainInfo};
use key_resolver::ReducerType;
use pallas::ledger::primitives::conway::CostModels;
use responses::{Eras, ErrorResponse, ProtocolParametersV6, TimestampedResponse};
use routes::{
    get_accounts_router, get_addresses_router, get_assets_routes, get_blocks_routes,
    get_datums_routes, get_ecosystem_routes, get_epochs_routes, get_policy_routes,
    get_pools_routes, get_scripts_routes, get_transactions_routes, AdditionalUtxo, EvaluateRequest,
};
use serde::Deserialize;
use serde_json::json;
use sqlx::{postgres::PgPoolOptions, PgPool};

use bb8::Pool;
use bb8_tikv::TiKVTransactionalConnectionManager;
use tikv_client::{Snapshot, TransactionOptions};
use timbre::encoding::encode::KeyEncoder;
use url::Url;
use utils::{bad_request, internal_server_error, not_found};
use utoipa::{OpenApi, ToSchema};

pub mod args;
pub mod cache;
pub mod chain;
pub mod config;
pub mod key_resolver;
pub mod ogmios_v6;
pub mod responses;
pub mod routes;
pub mod utils;

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Cardano - Blockchain Indexer API",
        version = "v1.8.0",
        description = "Core indexer endpoints dedicated to Cardano.",
        contact(
            name = "Go Maestro Inc.",
            url = "https://gomaestro.org/"
        ),
        license(
            name = "Apache 2.0",
            url = "https://www.apache.org/licenses/LICENSE-2.0.txt"
        )
    ),
    servers(
        (url = "http://localhost:4000", description = "Local instance")
    ),
    paths(
        healthcheck,

        // accounts
        routes::account_addresses,
        routes::account_assets,
        routes::account_history,
        routes::account_info,
        routes::account_rewards,
        routes::account_updates,
        routes::account_delegations,

        // addresses
        routes::decode_address,
        routes::tx_count_by_address,
        routes::txs_by_address,
        routes::txs_by_payment_cred,
        routes::txs_by_payment_creds,
        routes::utxo_refs_at_address,
        routes::balance_by_payment_cred,
        routes::utxos_by_address,
        routes::utxos_by_addresses,
        routes::utxos_by_payment_cred,
        routes::utxos_by_payment_creds,

        // assets
        routes::asset_accounts,
        routes::asset_addresses,
        routes::asset_info,
        routes::asset_mints,
        routes::asset_txs,
        routes::asset_utxos,

        routes::policy_accounts,
        routes::policy_addresses,
        routes::policy_assets,
        routes::policy_info,
        routes::policy_mints,
        routes::policy_txs,
        routes::policy_utxos,

        // blocks
        routes::block_info,
        routes::latest_block,

        // ecosystem
        routes::adahandle_resolve,

        // epochs
        routes::epoch_info,
        routes::current_epoch,

        // pools
        routes::list_pools,
        routes::pool_blocks,
        routes::pool_delegators,
        routes::pool_historical_delegators,
        routes::pool_history,
        routes::pool_info,
        routes::pool_metadata,
        routes::pool_relays,
        routes::pool_updates,

        // scripts
        routes::script_by_hash,
        routes::datum_by_hash,
        routes::datums_by_hashes,

        // transactions
        routes::address_by_txo,
        routes::evaluate_redeemers,
        routes::tx_cbor_by_tx_hash,
        routes::tx_info,
        routes::txo_by_txo_ref,
        routes::txos_by_txo_refs,

        // general
        routes::chain_tip,
        routes::era_summaries,
        routes::protocol_parameters,
        routes::system_start,
    ),
    components(
        schemas(
            responses::LastUpdated,

            responses::Address,
            responses::PaginatedAddress,
            responses::PaginatedAsset,
            responses::PaginatedAccountHistory,
            responses::TimestampedAccountInfo,
            responses::PaginatedAccountReward,
            responses::PaginatedAccountUpdate,
            responses::PaginatedAccountDelegation,

            responses::TimestampedTxCount,
            responses::PaginatedAddressTransaction,
            responses::PaginatedUtxoRef,
            responses::PaginatedUtxoWithSlot,
            responses::PaginatedUtxoWithBytes,
            responses::TimestampedBalance,
            responses::PaginatedPaymentCredentialTransaction,
            responses::PaginatedPaymentCredentialsTransaction,
            responses::PaginatedAssetHolderAccount,
            responses::PaginatedAssetHolder,
            responses::TimestampedAssetInfo,
            responses::PaginatedAssetTx,
            responses::PaginatedMintingTx,
            responses::PaginatedAssetUtxo,
            responses::PaginatedPolicyHolderAccount,
            responses::PaginatedPolicyHolder,
            responses::PaginatedAssetInfoOld,
            responses::PaginatedAssetTx,

            responses::PaginatedAssetInfoConcise,
            responses::PaginatedPolicyUtxo,
            responses::TimestampedPolicyInfo,

            responses::TimestampedBlockInfo,

            responses::TimestampedAddress,

            responses::TimestampedCurrentEpochInfo,
            responses::TimestampedEpochInfo,

            responses::PaginatedPoolListInfo,
            responses::PaginatedPoolBlock,
            responses::PaginatedDelegatorInfo,
            responses::PaginatedHistoricalDelegatorInfo,
            responses::PaginatedPoolHistory,
            responses::TimestampedPoolInfo,
            responses::TimestampedPoolMetadata,
            responses::TimestampedPoolRelays,
            responses::TimestampedPoolUpdates,
            responses::TimestampedScript,
            responses::TimestampedTxCbor,
            responses::TimestampedTransactionInfo,
            responses::TimestampedUtxo,

            responses::TimestampedChainTip,
            responses::TimestampedEraSummaries,
            responses::TimestampedDatum,
            responses::TimestampedProtocolParameters,
            responses::TimestampedSystemStart,

            responses::AddressTransaction,
            responses::BlockInfo,
            responses::LedgerEra,
            responses::OperationalCert,
            responses::ExUnits,
            responses::DatumOption,
            responses::DatumOptionType,
            responses::Script,
            responses::ScriptType,
            responses::ScriptFirstSeen,
            responses::Utxo,
            responses::UtxoWithBytes,
            responses::UtxoWithSlot,
            responses::Balance,
            // Address
            responses::AddressInfo,
            responses::NetworkId,
            responses::PaymentCredential,
            responses::PaymentCredentialTransaction,
            responses::PaymentCredentialsTransaction,
            responses::PaymentCredKind,
            responses::StakingCredential,
            responses::StakingCredKind,
            responses::Pointer,
            // ...
            responses::UtxoRef,
            responses::TxCbor,
            responses::TxCount,
            responses::Datum,
            responses::Asset,
            responses::ChainTip,
            responses::SystemStart,
            responses::ProtocolParametersV5,

            responses::ExUnit,
            responses::Prices,
            responses::Version,
            responses::EraParameters,
            responses::EraSummary,
            responses::EraSummaries,
            responses::Bound,
            responses::TxStatus,
            responses::TxStatusInfo,

            // Ogmios V6
            responses::ProtocolParametersV6,
            responses::Ada,
            responses::MinFeeConstant,
            responses::MaxBlockBodySize,
            responses::MaxBlockHeaderSize,
            responses::MaxTransactionSize,
            responses::StakeDeposit,
            responses::MinStakePoolCost,
            responses::MinUtxoDepositConstant,
            responses::PlutusCostModels,
            responses::ScriptExecutionPrices,
            responses::MaxExecutionUnits,
            responses::MaxValueSize,
            responses::DelegateRepresentativeDeposit,
            responses::DelegateRepresentativeVotingThresholds,
            responses::GovernanceActionDeposit,
            responses::MaxReferenceScriptsSize,
            responses::MinFeeReferenceScripts,
            responses::StakePoolVotingThresholds,
            responses::ConstitutionalCommittee,
            responses::ProtocolParametersUpdateStakePool,
            responses::ProtocolParametersUpdateDRep,
            responses::Eras,
            responses::Era,
            responses::Start,
            responses::End,
            responses::Parameters,
            responses::Time,
            responses::SlotLength,

            // Assets
            responses::Holders,
            responses::PolicyInfo,
            responses::TimestampedTransaction,
            responses::MintTransaction,
            responses::PolicyMintTransaction,
            responses::PolicyHolderAccount,
            responses::PolicyHolder,
            responses::PolicyTransaction,
            responses::PolicyUtxo,
            responses::AssetInPolicy,
            responses::AssetInPolicyMint,
            responses::AssetHolder,
            responses::AssetHolderAccount,
            responses::AssetUtxo,
            responses::AssetInfo,
            responses::AssetInfoOld,
            responses::AssetInfoConcise,
            responses::AssetStandards,
            responses::Cip68Metadata,
            responses::Cip68AssetType,
            responses::TokenRegistryMetadata,
            responses::AssetTx,
            responses::MintingTx,
            // Accounts
            responses::AccountInfo,
            responses::AccountHistory,
            responses::AccountUpdate,
            responses::AccountAction,
            responses::AccountReward,
            responses::AccountRewardType,
            responses::AccountStakingRewardType,
            responses::AccountDelegation,
            // Pools
            responses::PoolListInfo,
            responses::PoolBlock,
            responses::DelegatorInfo,
            responses::HistoricalDelegatorInfo,
            responses::PoolHistory,
            responses::PoolInfo,
            responses::PoolMetadata,
            responses::PoolRelay,
            responses::PoolRelays,
            responses::PoolUpdate,
            responses::PoolUpdates,
            responses::Relay,
            responses::PoolMetaJson,
            // Epoch
            responses::EpochInfo,
            responses::CurrentEpochInfo,

            responses::EvaluatedRedeemer,
            responses::RedeemerTag,
            EvaluateRequest,
            AdditionalUtxo,
            // tx info
            responses::TransactionInfo,
            responses::Certificates,
                responses::StakeRegCert,
                responses::StakeDelegCert,
                responses::PoolRegCert,
                responses::PoolRetireCert,
                responses::MirCert,
                responses::MirSource,
                responses::MirTarget,
                responses::RegCert,
                responses::UnRegCert,
                responses::DRep,
                responses::DRepKind,
                responses::DRepCredential,
                responses::DRepCredKind,
                responses::VoteDelegCert,
                responses::StakeVoteDelegCert,
                responses::StakeRegDelegCert,
                responses::VoteRegDelegCert,
                responses::StakeVoteRegDelegCert,
                responses::AuthCommitteeHotCert,
                responses::ResignCommitteeColdCert,
                responses::Anchor,
                responses::RegDRepCert,
                responses::UnRegDRepCert,
                responses::UpdateDRepCert,
            responses::Redeemers,
                responses::SpendRedeemer,
                responses::MintRedeemer,
                responses::WdrlRedeemer,
                responses::CertRedeemer,
                responses::VoteRedeemer,
                responses::ProposalRedeemer,
            responses::MintAsset,
            responses::Withdrawal,
            responses::Data,

            responses::NumOrString,
        ),
    ),
    tags(
        (name = "Maestro")
    ),
)]
pub struct ApiDoc;

#[derive(Clone)]
pub struct MapiConfig {
    pub polyphony_wrapper: PolyphonyWrapper,
    pub dbsync: PgPool,
    pub ogmios_v6_url: Url,
    pub chain_info: ChainInfo,
    pub timeout_duration: Duration,
    pub ogmios_query_opts: ogmios_v6::OgmiosQueryOpts,
    pub protocol_parameters_cache:
        Arc<TtlCache<TimestampedResponse<ProtocolParametersV6>, ErrorResponse>>,
    pub era_summaries_cache: Arc<TtlCache<TimestampedResponse<Eras>, ErrorResponse>>,
    // Evaluate-redeemers consumes only cost models, not the `last_updated` chain-tip
    // metadata, so it gets its own single-flight cache fed straight from Ogmios. Sharing
    // `protocol_parameters_cache` would have coupled transaction evaluation to a dbsync
    // query it never reads — making evaluate fail whenever dbsync is unavailable.
    pub cost_models_cache: Arc<TtlCache<CostModels, ErrorResponse>>,
}

pub type MapiExtension = Extension<MapiConfig>;

#[derive(Clone)]
pub struct PolyphonyWrapper {
    pool: Pool<TiKVTransactionalConnectionManager>,
    /// `(dataplane_id, instance_id)` of the token registry keyspace, if configured.
    token_registry_ids: Option<(u8, u8)>,
}

/// Resolves the `(dataplane_id, instance_id)` pair for a reducer from the
/// registry in Redis and wraps it in a `KeyEncoder`. Resolution failures
/// (Redis outage, empty registry, malformed entry) surface as a 500 JSON
/// error response instead of panicking.
fn resolve_encoder(reducer: ReducerType) -> Result<KeyEncoder, ErrorResponse> {
    let (dataplane_id, reducer_id) =
        key_resolver::resolve_key(reducer).map_err(internal_server_error)?;
    Ok(KeyEncoder::new(dataplane_id, reducer_id))
}

impl PolyphonyWrapper {
    pub fn new(
        pool: Pool<TiKVTransactionalConnectionManager>,
        token_registry_ids: Option<(u8, u8)>,
    ) -> Self {
        PolyphonyWrapper {
            pool,
            token_registry_ids,
        }
    }

    pub async fn get_tikv_client(
        &self,
    ) -> Result<bb8::PooledConnection<'_, TiKVTransactionalConnectionManager>, ErrorResponse> {
        self.pool.get().await.map_err(internal_server_error)
    }

    pub async fn begin_snapshot_latest(&self) -> Result<Snapshot, ErrorResponse> {
        let client = self.get_tikv_client().await?;

        let ts = client
            .current_timestamp()
            .await
            .map_err(internal_server_error)?;

        let snap = client.snapshot(ts, TransactionOptions::new_optimistic());

        Ok(snap)
    }

    pub fn block_by_height_encoder(&self) -> Result<KeyEncoder, ErrorResponse> {
        resolve_encoder(ReducerType::BlockByHeight)
    }

    pub fn datum_by_hash_encoder(&self) -> Result<KeyEncoder, ErrorResponse> {
        resolve_encoder(ReducerType::DatumByHash)
    }

    pub fn tx_by_hash_encoder(&self) -> Result<KeyEncoder, ErrorResponse> {
        resolve_encoder(ReducerType::TxByHash)
    }

    pub fn tx_count_by_address_encoder(&self) -> Result<KeyEncoder, ErrorResponse> {
        resolve_encoder(ReducerType::TxCountByAddress)
    }

    pub fn txs_by_address_encoder(&self) -> Result<KeyEncoder, ErrorResponse> {
        resolve_encoder(ReducerType::TxsByAddress)
    }

    pub fn txs_by_pay_cred_encoder(&self) -> Result<KeyEncoder, ErrorResponse> {
        resolve_encoder(ReducerType::TxsByPayCred)
    }

    pub fn utxo_cbor_by_address_encoder(&self) -> Result<KeyEncoder, ErrorResponse> {
        resolve_encoder(ReducerType::UtxoCborByAddress)
    }

    pub fn utxos_by_asset_encoder(&self) -> Result<KeyEncoder, ErrorResponse> {
        resolve_encoder(ReducerType::UtxosByAsset)
    }

    pub fn utxos_by_policy_encoder(&self) -> Result<KeyEncoder, ErrorResponse> {
        resolve_encoder(ReducerType::UtxosByPolicy)
    }

    pub fn script_by_hash_encoder(&self) -> Result<KeyEncoder, ErrorResponse> {
        resolve_encoder(ReducerType::ScriptByHash)
    }

    pub fn supply_by_asset_encoder(&self) -> Result<KeyEncoder, ErrorResponse> {
        resolve_encoder(ReducerType::SupplyByAsset)
    }

    pub fn updates_by_policy_encoder(&self) -> Result<KeyEncoder, ErrorResponse> {
        resolve_encoder(ReducerType::UpdatesByPolicy)
    }

    pub fn mint_metadata_by_asset_encoder(&self) -> Result<KeyEncoder, ErrorResponse> {
        resolve_encoder(ReducerType::MintMetadataByAsset)
    }

    pub fn txs_by_policy_encoder(&self) -> Result<KeyEncoder, ErrorResponse> {
        resolve_encoder(ReducerType::TxsByPolicy)
    }

    pub fn cip25_metadata_by_asset_encoder(&self) -> Result<KeyEncoder, ErrorResponse> {
        resolve_encoder(ReducerType::Cip25MetadataByAsset)
    }

    pub fn token_registry_encoder(&self) -> Option<KeyEncoder> {
        // Special logic for the token registry encoder:
        // - The token registry data is not a real reducer; it is fed by an
        //   external ingestor and is not managed by the dataplane agent.
        // - This dataplane ID and instance ID won't be loaded from redis,
        //   but from the (optional) mapi config. When unconfigured, no
        //   token-registry data is served.
        self.token_registry_ids
            .map(|(dataplane_id, instance_id)| KeyEncoder::new(dataplane_id, instance_id))
    }

    pub fn block_by_tx_encoder(&self) -> Result<KeyEncoder, ErrorResponse> {
        resolve_encoder(ReducerType::BlockByTx)
    }

    pub async fn get_tx_bytes(
        &self,
        hex_tx_hash: &String,
    ) -> Result<(Vec<u8>, Snapshot), ErrorResponse> {
        let tx_hash: [u8; 32] = match hex::decode(hex_tx_hash) {
            Ok(b) => b
                .try_into()
                .map_err(|_| bad_request("Malformed transaction hash"))?,
            Err(_) => return Err(bad_request("Transaction hash must be hex encoded")),
        };

        let key = self.tx_by_hash_encoder()?.encode_tx_by_hash_key(&tx_hash);

        let mut txn = self.begin_snapshot_latest().await?;

        Ok((
            txn.get(key)
                .await
                .map_err(internal_server_error)?
                .ok_or_else(not_found)?,
            txn,
        ))
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CursorPagination {
    pub count: Option<CountParam>,
    pub cursor: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SlotPagination {
    pub count: Option<CountParam>,
    pub order: Option<OrderParam>,

    pub cursor: Option<String>,

    pub from: Option<u64>,
    pub to: Option<u64>,
}

#[derive(Clone, Copy, Debug, Deserialize, ToSchema, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
#[schema(default = "asc")]
pub enum OrderParam {
    Asc,
    Desc,
}

#[derive(Clone, Copy, Debug, Deserialize, ToSchema, PartialEq, PartialOrd)]
#[schema(default = 100)]
pub struct CountParam(usize);

#[derive(ToSchema)]
#[schema(default = 1)]
#[allow(dead_code)]
pub struct PageParam(usize);

pub async fn app(config: Config) -> anyhow::Result<Router> {
    let tikv_conn_manager =
        TiKVTransactionalConnectionManager::new(vec![config.tikv_pd], None).unwrap();

    let tikv_pool = Pool::builder()
        .max_size(config.tikv_max_connections)
        .build(tikv_conn_manager)
        .await
        .unwrap();

    let polyphony = PolyphonyWrapper::new(
        tikv_pool,
        config
            .token_registry_dataplane_id
            .zip(config.token_registry_instance_id),
    );

    let chain = match config.network.to_lowercase().as_str() {
        "mainnet" => mainnet_chain_info(),
        "preprod" => preprod_chain_info(),
        "preview" => preview_chain_info(),
        _ => panic!("please specify the NETWORK envvar ['mainnet', 'preprod', 'preview']"),
    };

    match config.network.to_lowercase().as_str() {
        "mainnet" => key_resolver::initialize_config(key_resolver::Network::Mainnet, config.redis),
        "preprod" => key_resolver::initialize_config(key_resolver::Network::Preprod, config.redis),
        "preview" => key_resolver::initialize_config(key_resolver::Network::Preview, config.redis),
        _ => panic!("please specify the NETWORK envvar ['mainnet', 'preprod', 'preview']"),
    };

    // Connect lazily: the db-sync database is optional at startup (deployments
    // without the db-sync-backed endpoint groups run without one), so defer
    // connection errors to the endpoints that actually query it.
    let dbsync = PgPoolOptions::new()
        .max_connections(config.dbsync_max_connections)
        .connect_lazy(&config.dbsync_db_url)?;

    let ogmios_v6_endpoint = url::Url::parse(&config.ogmios_v6_url)?;

    let timeout = Duration::from_millis(config.scan_timeout_millis);

    let ogmios_query_opts = ogmios_v6::OgmiosQueryOpts {
        timeout: Duration::from_millis(config.ogmios_query_timeout_millis),
        max_retries: config.ogmios_max_retries,
    };
    // `ogmios_cache_ttl_secs` is validated to be > 0 at config load.
    let cache_ttl = Duration::from_secs(config.ogmios_cache_ttl_secs);
    // Briefly serve a failed refresh to concurrent callers (preserving its real status) so a
    // backend outage neither stampedes the backend nor masks the error class behind a 503.
    let cache_failure_ttl = Duration::from_secs(1);
    let protocol_parameters_cache = Arc::new(TtlCache::new(cache_ttl, cache_failure_ttl));
    let era_summaries_cache = Arc::new(TtlCache::new(cache_ttl, cache_failure_ttl));
    let cost_models_cache = Arc::new(TtlCache::new(cache_ttl, cache_failure_ttl));

    #[allow(deprecated)]
    let router = Router::new()
        .route("/", get(root))
        .route("/healthcheck", get(healthcheck))
        .route("/chain-tip", get(routes::chain_tip))
        .route("/era-summaries", get(routes::era_summaries))
        .route("/protocol-parameters", get(routes::protocol_parameters))
        .route("/system-start", get(routes::system_start))
        .nest("/datum", get_datums_routes()) // TODO: DEPRECATED
        .nest("/datums", get_datums_routes())
        .nest("/accounts", get_accounts_router())
        .nest("/addresses", get_addresses_router())
        .nest("/assets", get_assets_routes())
        .nest("/blocks", get_blocks_routes())
        .nest("/ecosystem", get_ecosystem_routes())
        .nest("/epochs", get_epochs_routes())
        .nest("/policy", get_policy_routes())
        .nest("/pools", get_pools_routes())
        .nest("/scripts", get_scripts_routes())
        .nest("/transactions", get_transactions_routes())
        .layer(Extension(MapiConfig {
            polyphony_wrapper: polyphony,
            dbsync,
            ogmios_v6_url: ogmios_v6_endpoint,
            chain_info: chain,
            timeout_duration: timeout,
            ogmios_query_opts,
            protocol_parameters_cache,
            era_summaries_cache,
            cost_models_cache,
        }))
        .layer(
            tower_http::cors::CorsLayer::new()
                .allow_origin(tower_http::cors::Any)
                .allow_methods(tower_http::cors::Any),
        );

    Ok(router)
}

// Healthcheck
async fn root() -> impl IntoResponse {
    (StatusCode::OK, Json(json!({ "status": "ok" })))
}

#[utoipa::path(
    get,
    path = "/healthcheck",
    responses(
        (status = 200, description= "Service is working"),
        (status = 500, description= "Internal Server Error")
    )
)]
async fn healthcheck(Extension(config): MapiExtension) -> Result<impl IntoResponse, ErrorResponse> {
    let chain = config.chain_info;
    let polyphony = config.polyphony_wrapper;

    let encoder = polyphony.block_by_height_encoder()?;

    let mut txn = polyphony.begin_snapshot_latest().await?;

    let _ = utils::get_last_updated(&mut txn, &encoder, &chain).await?;

    Ok((StatusCode::OK, "OK"))
}
