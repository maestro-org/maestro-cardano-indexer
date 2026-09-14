use std::error::Error;
use std::ops::Deref;
use std::{collections::HashMap, fmt};

use axum::http::StatusCode;
use pallas::ledger::traverse::wellknown::{
    GenesisValues, MAINNET_MAGIC, PREVIEW_MAGIC, PRE_PRODUCTION_MAGIC,
};
use serde::{Deserialize, Serialize};
use sqlx::types::chrono::NaiveDateTime;
use sqlx::{postgres::PgRow, types::Json, FromRow, Row};
use utoipa::ToSchema;

use crate::chain::ChainInfo;
use crate::utils::{de_i64_from_str, get_string_from_numeric, slot_to_timestamp_str};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorBody {
    pub code: u16,
    pub error: String,
    pub message: String,
}

pub type ErrorResponse = (StatusCode, axum::Json<ErrorBody>);

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, ToSchema)]
#[serde(untagged)]
/// Integer or number by default, or a string representation if the `amounts-as-strings` header is set to `true`
pub enum NumOrString {
    /// Unsigned 64-bit integer
    U64(u64),
    /// Unsigned 128-bit integer
    U128(u128),
    /// Signed 64-bit integer
    I64(i64),
    /// 64-bit floating point number
    F64(f64),
    /// String representation of an integer or number
    String(String),
}

#[derive(Debug)]
pub struct MapiDecodeError;

impl fmt::Display for MapiDecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Mapi decode error")
    }
}

impl Error for MapiDecodeError {}

#[derive(Serialize, Deserialize, ToSchema)]
/// Bech32-encoded Cardano Address
pub struct Address(pub String);

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
/// Transaction which involved a specific address
pub struct AddressTransaction {
    /// Transaction hash
    pub tx_hash: String,
    /// Absolute slot of the block which contains the transaction
    pub slot: u64,
    /// Address controlled at least one of the consumed UTxOs
    pub input: bool,
    /// Address controlled at least one of the produced UTxOs
    pub output: bool,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
/// Transaction which involved a specific payment credential
pub struct PaymentCredentialTransaction {
    /// Transaction hash
    pub tx_hash: String,
    /// Absolute slot of the block which contains the transaction
    pub slot: u64,
    /// Payment credential controlled at least one of the consumed UTxOs
    pub input: bool,
    /// Payment credential controlled at least one of the produced UTxOs
    pub output: bool,
    /// Payment credential was an additional required signer
    pub required_signer: bool,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
/// Transaction which involved one or more specific payment credentials
pub struct PaymentCredentialsTransaction {
    /// Transaction hash
    pub tx_hash: String,
    /// Absolute slot of the block which contains the transaction
    pub slot: u64,
}

#[derive(Serialize, Deserialize, ToSchema)]
/// Number of transactions
pub struct TxCount(pub u64);

#[derive(Debug, PartialEq, Serialize, Deserialize, ToSchema)]
/// Holder of assets of a specific policy
pub struct PolicyHolder {
    /// Address of the holder
    pub address: String,
    /// List of assets owned by the holder belonging to the policy
    pub assets: Vec<AssetInPolicy>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, ToSchema)]
/// Account which controls some assets of a specific policy
pub struct PolicyHolderAccount {
    /// Address of the holder
    pub account: String,
    /// List of assets owned by the holder belonging to the policy
    pub assets: Vec<AssetInPolicy>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, ToSchema)]
/// UTxO which contains assets of a specific policy
pub struct PolicyUtxo {
    /// UTxO transaction hash
    pub tx_hash: String,
    /// UTxO transaction index
    pub index: usize,
    /// Absolute slot of block which produced the UTxO
    pub slot: u64,
    /// Address which controls the UTxO
    pub address: String,
    /// List of assets contained in the UTxO belonging to the policy
    pub assets: Vec<AssetInPolicy>,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
/// A transaction which moves assets of a specific policy
pub struct PolicyTransaction {
    /// Transaction hash
    pub tx_hash: String,
    /// Absolute slot of block which contained the transaction
    pub slot: u64,
    /// List of assets of the policy which were involved in the transaction
    pub assets: Vec<String>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, ToSchema)]
/// Holder of a specific asset
pub struct AssetHolder {
    /// Address of the holder
    pub address: String,
    /// Amount of the asset owned by the holder
    pub amount: NumOrString,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, ToSchema)]
/// Account which controls some of a specific asset
pub struct AssetHolderAccount {
    /// Stake/reward address for stake credential
    pub account: String,
    /// Amount of the asset held by addresses which use the stake credential
    pub amount: NumOrString,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, ToSchema)]
/// UTxO which contains a specific asset
pub struct AssetUtxo {
    /// UTxO transaction hash
    pub tx_hash: String,
    /// UTxO transaction index
    pub index: u64,
    /// Absolute slot of block which produced the UTxO
    pub slot: u64,
    /// Address which controls the UTxO
    pub address: String,
    /// Amount of the asset contained in the UTxO
    pub amount: NumOrString,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
/// Asset information corresponding to popular standards
pub struct AssetStandards {
    #[schema(value_type = Option<Object>)]
    /// CIP-25 metadata for a specific asset
    pub cip25_metadata: Option<serde_json::Value>,
    /// CIP-68 metadata for a specific asset
    pub cip68_metadata: Option<Cip68Metadata>,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
/// Policy ID information corresponding to popular standards
pub struct PolicyStandards {
    #[schema(value_type = Option<Object>)]
    /// CIP-27 (community royalties) metadata for a policy ID
    pub cip27_metadata: Option<serde_json::Value>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, ToSchema)]
/// CIP-27 metadata for a native-asset policy ID
pub struct Cip27Metadata {
    /// Address to receive the royalties
    pub address: String,
    /// Percentage of sales requested as royalty
    pub rate: NumOrString,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
/// Information about a specific Cardano native-asset
pub struct AssetInfo {
    /// Hex encoding of the asset name
    pub asset_name: String,
    /// ASCII representation of the asset name
    pub asset_name_ascii: String,
    /// CIP-14 fingerprint of the asset
    pub fingerprint: String,
    /// Total amount of the asset in existence currently
    pub total_supply: String,
    pub unique_holders: Holders,
    /// First transaction which minted some of a specific asset
    pub first_mint_tx: MintTransaction,
    /// The latest transaction which minted or burned some of a specific asset
    pub latest_mint_tx: MintTransaction,
    /// Number of transactions which minted some of the asset
    pub mint_tx_count: u64,
    /// Number of transactions which burned some of the asset
    pub burn_tx_count: u64,
    pub asset_standards: AssetStandards,
    /// Metadata of the most recent transaction which minted or burned the asset
    #[schema(value_type = Option<Object>)]
    pub latest_mint_tx_metadata: Option<serde_json::Value>,
    /// Token registry metadata for the asset
    pub token_registry_metadata: Option<TokenRegistryMetadata>,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
/// Information about a specific Cardano native-asset
pub struct AssetInfoOld {
    /// Hex encoding of the asset name
    pub asset_name: String,
    /// ASCII representation of the asset name
    pub asset_name_ascii: Option<String>,
    /// CIP-14 fingerprint of the asset
    pub fingerprint: String,
    /// Total amount of the asset in existence currently
    pub total_supply: u64,
    /// First transaction which minted some of a specific asset
    pub first_mint_tx: String,
    /// Time of the first transaction which minted some of a specific asset
    pub first_mint_time: i32,
    /// Number of transactions which minted some of the asset
    pub mint_tx_count: i64,
    /// Number of transactions which burned some of the asset
    pub burn_tx_count: i64,
    pub asset_standards: AssetStandards,
    #[schema(value_type = Object)]
    /// Metadata of the most recent transaction which minted the asset
    pub latest_mint_tx_metadata: Option<serde_json::Value>,
    /// Token registry metadata for the asset
    pub token_registry_metadata: Option<TokenRegistryMetadata>,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
/// Information about a specific Cardano native-asset
pub struct AssetInfoConcise {
    /// Hex encoding of the asset name
    pub asset_name: String,
    /// ASCII representation of the asset name
    pub asset_name_ascii: String,
    /// CIP-14 fingerprint of the asset
    pub fingerprint: String,
    /// Current amount of the asset minted
    pub total_supply: String,
    pub asset_standards: AssetStandards,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
/// Information about a Cardano native-asset policy ID
pub struct PolicyInfo {
    /// Hex encoding of the policy ID
    pub policy_id: String,
    /// Details of the script which governs the minting of assets under the policy ID
    pub script: Script,
    /// Number of unique asset names of the policy ID which have existed
    pub assets_of_policy: u64,
    /// Sum of the total supplies of all different assets under the policy ID
    pub total_supply: String,
    /// Number of unique holders, by either full address or staking key
    pub unique_holders: Holders,
    // TODO: CIP27
    // pub asset_standards: PolicyStandards,
    /// First transaction which minted some of an asset of the policy ID
    pub first_mint_tx: TimestampedTransaction,
    /// The latest transaction which minted or burned some of an asset of the policy ID
    pub latest_mint_tx: TimestampedTransaction,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
/// Number of holders of at least one of a specific asset or assets of a specific policy, by address or address staking part
pub struct Holders {
    /// Number of unique addresses which control at least one of an asset of the policy ID
    pub by_address: u64,
    /// Number of unique staking keys used in addresses which control at least one of an asset of the policy ID
    pub by_account: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
/// Transaction hash with details of when that transaction appeared on-chain
pub struct TimestampedTransaction {
    /// Transaction hash
    pub tx_hash: String,
    /// Absolute slot of the block which includes the transaction
    pub slot: u64,
    /// UTC timestamp of the block which includes the transaction
    pub timestamp: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
/// Transaction hash with details of when that transaction appeared on-chain
pub struct MintTransaction {
    /// Transaction hash
    pub tx_hash: String,
    /// Absolute slot of the block which includes the transaction
    pub slot: u64,
    /// UTC timestamp of the block which includes the transaction
    pub timestamp: String,
    /// Amount of asset minted or burned (negative if burned)
    pub amount: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
/// Transaction hash with details of when that transaction appeared on-chain
pub struct PolicyMintTransaction {
    /// Transaction hash
    pub tx_hash: String,
    /// Absolute slot of the block which includes the transaction
    pub slot: u64,
    /// UTC timestamp of the block which includes the transaction
    pub timestamp: String,
    /// Assets of specified policy which were minted or burned in the transaction
    pub assets: Vec<AssetInPolicyMint>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
/// Name of asset of a specific policy and amount minted or burned
pub struct AssetInPolicyMint {
    /// Asset name
    pub name: String,
    /// Amount of asset minted (negative if burned)
    pub amount: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
/// Token registry metadata
pub struct TokenRegistryMetadata {
    /// Asset name
    pub name: String,
    /// Asset description
    pub description: String,
    /// Asset ticker
    pub ticker: Option<String>,
    /// URL associated with the asset
    pub url: Option<String>,
    /// Base64 encoded logo PNG associated with the asset
    pub logo: Option<String>,
    /// Recommended value for decimal places
    pub decimals: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema, FromRow)]
/// Transaction which moved or minted a specific asset
pub struct AssetTx {
    /// Transaction hash
    pub tx_hash: String,
    /// Epoch in which the transaction occurred
    pub epoch_no: i32,
    /// The height of the block which included the transaction
    pub block_height: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
/// Transaction which minted or burned a specific asset
pub struct MintingTx {
    /// Transaction hash
    pub tx_hash: String,
    #[serde(alias = "quantity")]
    #[serde(deserialize_with = "de_i64_from_str")]
    /// Amount of the asset minted or burned (negative if burned)
    pub mint_amount: i64,
    #[serde(alias = "block_time")]
    /// UNIX timestamp of the block which included transaction
    pub block_timestamp: i32, // TODO
    #[schema(value_type = Object)]
    /// Transaction metadata
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, ToSchema)]
/// Summary of information regarding a stake account
pub struct AccountInfo {
    /// Bech32 encoded stake address
    pub stake_address: String,
    /// True if the stake key is registered
    pub registered: bool,
    /// Bech32 pool ID that the stake key is delegated to
    pub delegated_pool: Option<String>,
    /// Bech32 DRep ID that the stake key is delegated to
    pub delegated_drep: Option<String>,
    /// The amount of rewards that are available to be withdrawn
    pub rewards_available: NumOrString,
    /// Amount locked in UTxOs controlled by addresses with the stake key
    pub utxo_balance: NumOrString,
    /// Total balance controlled by the stake key (sum of UTxO and rewards)
    pub total_balance: NumOrString,
    /// Total rewards earned
    pub total_rewarded: NumOrString,
    /// Total rewards withdrawn
    pub total_withdrawn: NumOrString,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
/// Per-epoch information about a stake account
pub struct AccountHistory {
    /// Epoch number
    pub epoch_no: i32,
    /// Bech32 encoded pool ID the account was delegated to
    pub pool_id: Option<String>,
    /// Active stake of the account in the epoch
    pub active_stake: NumOrString,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema, sqlx::Type)]
#[serde(rename_all = "lowercase")]
/// Type of staking-related action
pub enum AccountAction {
    Registration,
    Deregistration,
    Delegation,
    Withdrawal,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
/// Stake account related update
pub struct AccountUpdate {
    #[serde(alias = "action_type")]
    pub action: AccountAction,
    /// Transaction hash of the transaction which performed the action
    pub tx_hash: String,
    /// Epoch number in which the transaction occured
    pub epoch_no: i32,
    #[serde(alias = "absolute_slot")]
    /// Absolute slot of the block which contained the transaction
    pub abs_slot: i32,
    /// Deposit in lovelace if action is Registration
    pub deposit: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema, sqlx::FromRow)]
/// Stake account related update
pub struct AccountDelegation {
    /// Epoch number in which the delegation becomes active
    pub active_epoch_no: i64,
    /// Bech32 encoded pool ID the account is delegating to
    #[sqlx(rename = "pool_id")]
    pub pool_id: String,
    /// Absolute slot of the block which contained the transaction
    #[sqlx(rename = "slot_no")]
    pub slot: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema, sqlx::Type)]
#[serde(rename_all = "lowercase")]
/// Type of stake account reward
pub enum AccountRewardType {
    Member,
    Leader,
    Treasury,
    Reserves,
    Refund,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema, sqlx::Type)]
#[serde(rename_all = "lowercase")]
/// Staking-related reward type
pub enum AccountStakingRewardType {
    Member,
    Leader,
    Refund,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
/// Stake account related reward
pub struct AccountReward {
    #[schema(value_type = AccountStakingRewardType)]
    #[serde(rename = "type")]
    pub reward_type: AccountRewardType,
    /// Epoch in which the reward was earned
    pub earned_epoch: i32,
    /// Epoch at which the reward is spendable
    pub spendable_epoch: i32,
    /// Reward amount
    pub amount: NumOrString,
    /// Bech32 encoded pool ID (if relevant to reward type)
    pub pool_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
/// Datum type (inline datum or datum hash)
pub enum DatumOptionType {
    Hash,
    Inline,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
/// Datum (inline or hash)
pub struct DatumOption {
    #[serde(rename = "type")]
    pub datum_type: DatumOptionType,
    /// Datum hash
    pub hash: String,
    /// Hex encoded datum CBOR bytes (`null` if datum type is `hash` and corresponding datum bytes have not been seen on-chain)
    pub bytes: Option<String>,
    #[schema(value_type = Option<Object>)]
    /// JSON representation of the datum (`null` if datum type is `hash` and corresponding datum bytes have not been seen on-chain)
    pub json: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema, sqlx::Type)]
#[serde(rename_all = "lowercase")]
/// Script type and version
pub enum ScriptType {
    #[sqlx(rename = "timelock")]
    Native,
    #[sqlx(rename = "plutusV1")]
    PlutusV1,
    #[sqlx(rename = "plutusV2")]
    PlutusV2,
    #[sqlx(rename = "plutusV3")]
    PlutusV3,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
/// Details of a Native or Plutus script
pub struct Script {
    /// Script hash
    pub hash: String,
    #[serde(rename = "type")]
    pub script_type: ScriptType,
    /// Script bytes
    pub bytes: String,
    #[schema(value_type = Option<Object>)]
    /// JSON representation of script (`null` if script not of `native` type)
    pub json: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
/// Details of a Native or Plutus script
pub struct ScriptFirstSeen {
    /// Script hash
    pub hash: String,
    #[serde(rename = "type")]
    pub script_type: ScriptType,
    /// Script bytes
    pub bytes: String,
    #[schema(value_type = Option<Object>)]
    /// JSON representation of script (`null` if script not of `native` type)
    pub json: Option<serde_json::Value>,
    pub first_seen: TimestampedTransaction,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
/// Transaction output
pub struct Utxo {
    /// UTxO transaction hash
    pub tx_hash: String,
    /// UTxO transaction index
    pub index: u64,
    /// List of assets contained in the UTxO
    pub assets: Vec<Asset>,
    /// Address which controls the UTxO
    pub address: String,
    /// Datum contained in the UTxO
    pub datum: Option<DatumOption>,
    /// Inlined script contained in the UTxO
    pub reference_script: Option<Script>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
/// Transaction output
pub struct UtxoWithSlot {
    /// UTxO transaction hash
    pub tx_hash: String,
    /// UTxO transaction index
    pub index: usize,
    /// Absolute slot of block which produced the UTxO
    pub slot: u64,
    /// List of assets contained in the UTxO
    pub assets: Vec<Asset>,
    /// Address which controls the UTxO
    pub address: String,
    /// Datum contained in the UTxO
    pub datum: Option<DatumOption>,
    /// Inlined script contained in the UTxO
    pub reference_script: Option<Script>,
    /// Hex encoded transaction output CBOR bytes
    pub txout_cbor: Option<String>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, ToSchema)]
/// Transaction output (with field for optionally-returned CBOR bytes)
pub struct UtxoWithBytes {
    /// UTxO transaction hash
    pub tx_hash: String,
    /// UTxO transaction index
    pub index: usize,
    /// List of assets contained in the UTxO
    pub assets: Vec<Asset>,
    /// Address which controls the UTxO
    pub address: String,
    /// Datum contained in the UTxO
    pub datum: Option<DatumOption>,
    /// Inlined script contained in the UTxO
    pub reference_script: Option<Script>,
    /// Hex encoded transaction output CBOR bytes
    pub txout_cbor: Option<String>,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
/// UTxO reference (transaction hash and output index)
pub struct UtxoRef {
    /// UTxO transaction hash
    pub tx_hash: String,
    /// UTxO transaction index
    pub index: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
/// Lovelace or native asset
pub struct Asset {
    /// Asset (either `lovelace` or concatenation of hex encoded policy ID and asset name for native asset)
    pub unit: String,
    /// Amount of the asset
    pub amount: NumOrString,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, ToSchema)]
/// Lovelace or native asset
pub struct MintAsset {
    /// Asset (represented as concatenation of hex encoded policy ID and asset name)
    pub unit: String,
    /// Amount of the asset minted or burned (negative is burn)
    pub amount: NumOrString,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, ToSchema)]
/// Asset of a specific policy
pub struct AssetInPolicy {
    /// Hex encoded asset name
    pub name: String,
    /// Amount of the asset
    pub amount: NumOrString,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
/// Hex encoded transaction CBOR bytes
pub struct TxCbor(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Datum {
    /// JSON representation of the datum
    #[serde(alias = "value")]
    #[schema(value_type = Option<Object>)]
    pub json: serde_json::Value,
    /// Hex encoded datum CBOR bytes
    pub bytes: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SystemStart(pub String);

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct Bound {
    time: i64,
    slot: i64,
    epoch: i64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct EraSummaries(pub Vec<EraSummary>);

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct EraSummary {
    start: Bound,
    pub end: Option<Bound>,
    parameters: EraParameters,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all(deserialize = "camelCase"))]
pub struct EraParameters {
    epoch_length: i64,
    slot_length: i64,
    safe_zone: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Version {
    pub major: i32,
    pub minor: i32,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct Prices {
    pub memory: String,
    pub steps: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
/// Execution units for Plutus scripts
pub struct ExUnit {
    /// Memory execution units
    pub memory: i64,
    /// CPU execution units
    pub steps: i64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all(deserialize = "camelCase"))]
pub struct ProtocolParametersV5 {
    pub min_fee_coefficient: i64,
    pub min_fee_constant: i64,
    pub max_block_body_size: i64,
    pub max_block_header_size: i64,
    pub max_tx_size: i64,
    pub stake_key_deposit: i64,
    pub pool_deposit: i64,
    pub pool_retirement_epoch_bound: i64,
    pub desired_number_of_pools: i64,
    pub pool_influence: String,
    pub monetary_expansion: String,
    pub treasury_expansion: String,
    pub protocol_version: Version,
    pub min_pool_cost: i64,
    pub coins_per_utxo_byte: i64,
    pub cost_models: HashMap<String, HashMap<String, i64>>,
    pub prices: Prices,
    pub max_execution_units_per_transaction: ExUnit,
    pub max_execution_units_per_block: ExUnit,
    pub max_value_size: i64,
    pub collateral_percentage: i64,
    pub max_collateral_inputs: i64,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub enum TxStatus {
    Failed,
    Onchain,
    Pending,
    Rejected,
    Rolledback,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct TxStatusInfo {
    pub tx_hash: String,
    pub tx_status: TxStatus,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct StakePool {
    pool_id: String,
    ticker: String,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, ToSchema)]
/// List of updates to a stake pool
pub struct PoolUpdates(pub Vec<PoolUpdate>);

#[derive(Debug, PartialEq, Serialize, Deserialize, ToSchema)]
/// Update to a stake pool
pub struct PoolUpdate {
    /// Transaction hash for the transaction which contained the update
    pub tx_hash: String,
    /// UNIX timestamp of the block containing the transaction
    pub block_time: Option<i32>,
    /// Bech32 encoded pool ID
    pub pool_id_bech32: String,
    /// Hex encoded pool ID
    pub pool_id_hex: String,
    /// Epoch when the update takes effect
    pub active_epoch_no: Option<i64>,
    /// VRF key hash
    pub vrf_key_hash: Option<String>,
    /// Pool margin
    pub margin: NumOrString,
    /// Pool fixed cost
    pub fixed_cost: NumOrString,
    /// Pool pledge
    pub pledge: NumOrString,
    /// Reward address associated with the pool
    pub reward_addr: Option<String>,
    /// List of stake keys which control the pool
    pub owners: Option<Vec<String>>,
    /// Relays declared by the pool
    pub relays: Option<Vec<Relay>>,
    /// URL pointing to the pool metadata
    pub meta_url: Option<String>,
    /// Hash of the pool metadata
    pub meta_hash: Option<String>,
    /// JSON representation of the pool metadata
    pub meta_json: Option<PoolMetaJson>,
    /// Status of the pool
    pub pool_status: Option<String>,
    /// Epoch at which the pool will be retired
    pub retiring_epoch: Option<i32>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, ToSchema)]
/// Information summary of a stake pool
pub struct PoolInfo {
    /// Bech32 encoded pool ID
    pub pool_id_bech32: String,
    /// Hex encoded pool ID
    pub pool_id_hex: String,
    /// Epoch when the update takes effect
    pub active_epoch_no: i64,
    /// VRF key hash
    pub vrf_key_hash: Option<String>,
    /// Pool margin
    pub margin: NumOrString,
    /// Pool fixed cost
    pub fixed_cost: NumOrString,
    /// Pool pledge
    pub pledge: NumOrString,
    /// Reward address associated with the pool
    pub reward_addr: Option<String>,
    /// List of stake keys which control the pool
    pub owners: Vec<String>,
    /// Relays declared by the pool
    pub relays: Vec<Relay>,
    /// URL pointing to the pool metadata
    pub meta_url: Option<String>,
    /// Hash of the pool metadata
    pub meta_hash: Option<String>,
    /// JSON representation of the pool metadata
    pub meta_json: Option<PoolMetaJson>,
    /// Status of the pool
    pub pool_status: Option<String>,
    /// Epoch at which the pool will be retired
    pub retiring_epoch: Option<i32>,
    /// Pool operational certificate
    pub op_cert: Option<String>,
    /// Operational certificate counter
    pub op_cert_counter: Option<i64>,
    /// Active stake
    pub active_stake: NumOrString,
    /// Pool stake share
    pub sigma: Option<String>,
    /// Number of blocks created
    pub block_count: Option<u64>,
    /// Account balance of pool owners
    pub live_pledge: NumOrString,
    /// Live stake
    pub live_stake: NumOrString,
    /// Number of current delegators
    pub live_delegators: i64,
    /// Live saturation
    pub live_saturation: Option<String>,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema, FromRow)]
/// Stake pool identifier
pub struct PoolListInfo {
    /// Bech32 encoded pool ID
    pool_id_bech32: String,
    /// Pool ticker symbol
    ticker: Option<String>,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, FromRow, ToSchema)]
/// Information summary of a delegator
pub struct HistoricalDelegatorInfo {
    /// Bech32 encoded stake address (reward address)
    stake_address: Option<String>,
    /// Delegator live stake
    amount: Option<String>,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
/// A list of stake pool relays declared on-chain
pub struct PoolRelays(pub Vec<PoolRelay>);

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
/// Relay declared by a stake pool
pub struct PoolRelay {
    /// Bech32 encoded pool ID
    pub pool_id_bech32: String,
    /// Relays declared by the pool
    pub relays: Vec<Relay>,
}

impl<'r> FromRow<'r, PgRow> for PoolRelay {
    fn from_row(row: &'r PgRow) -> Result<Self, sqlx::Error> {
        let pool_id_bech32 = row.try_get("pool_id_bech32")?;

        let relays = row
            .try_get::<Json<Vec<Relay>>, _>("relays")?
            .iter()
            .cloned()
            .collect();

        Ok(PoolRelay {
            pool_id_bech32,
            relays,
        })
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
/// Metadata associated with a stake pool
pub struct PoolMetadata {
    /// Bech32 encoded pool ID
    pub pool_id_bech32: String,
    /// URL pointing to the pool metadata
    pub meta_url: Option<String>,
    /// Hash of the pool metadata
    pub meta_hash: Option<String>,
    /// JSON representation of the pool metadata
    pub meta_json: Option<PoolMetaJson>,
}

impl<'r> FromRow<'r, PgRow> for PoolMetadata {
    fn from_row(row: &'r PgRow) -> Result<Self, sqlx::Error> {
        let pool_id_bech32 = row.try_get("pool_id_bech32")?;
        let meta_url = row.try_get("meta_url")?;
        let meta_hash = row.try_get("meta_hash")?;

        let meta_json = row
            .try_get::<Option<Json<PoolMetaJson>>, _>("meta_json")?
            .map(|x| x.deref().clone());

        Ok(PoolMetadata {
            pool_id_bech32,
            meta_url,
            meta_hash,
            meta_json,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
/// Stake pool relay
pub struct Relay {
    pub dns: Option<String>,
    pub srv: Option<String>,
    pub ipv4: Option<String>,
    pub ipv6: Option<String>,
    pub port: Option<i32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema, FromRow)]
/// JSON metadata associated with a stake pool
pub struct PoolMetaJson {
    /// Pool name
    name: String,
    /// Pool ticker symbol
    ticker: Option<String>,
    /// Pool home page URL
    homepage: Option<String>,
    /// Pool description
    description: Option<String>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, ToSchema)]
/// Per-epoch history of a stake pool
pub struct PoolHistory {
    /// Epoch number
    pub epoch_no: i64,
    /// Active stake in the epoch
    pub active_stake: Option<NumOrString>,
    /// Pool active stake as percentage of total active stake
    pub active_stake_pct: Option<String>,
    /// Pool saturation percent
    pub saturation_pct: Option<String>,
    /// Blocks created in the epoch
    pub block_cnt: Option<i64>,
    /// Delegators in the epoch
    pub delegator_cnt: Option<i64>,
    /// Pool margin
    pub margin: Option<NumOrString>,
    /// Pool fixed cost
    pub fixed_cost: NumOrString,
    /// Fees collected for the epoch
    pub pool_fees: NumOrString,
    /// Total rewards earned by pool delegators for the epoch
    pub deleg_rewards: NumOrString,
    /// Annual return percentage for delegators for the epoch
    pub epoch_ros: String,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema, FromRow)]
/// Block created by a stake pool
pub struct PoolBlock {
    /// Epoch number
    epoch_no: Option<i32>,
    /// Epoch slot
    epoch_slot: Option<i32>,
    /// Absolute slot of the block
    pub abs_slot: Option<i64>,
    /// Block height (block number)
    block_height: i32,
    /// Block hash
    block_hash: String,
    /// UNIX timestamp when the block was mined
    block_time: i32,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, ToSchema)]
/// Information summary of a delegator
pub struct DelegatorInfo {
    /// Bech32 encoded stake address (reward address)
    pub stake_address: Option<String>,
    /// Delegator live stake
    pub amount: NumOrString,
    /// Epoch at which the delegation becomes active
    pub active_epoch_no: Option<i64>,
    /// Transaction hash relating to the most recent delegation
    pub latest_delegation_tx_hash: Option<String>,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
/// Blockchain chain-tip (most recently adopted block)
pub struct ChainTip {
    /// Block hash of the most recent block
    pub block_hash: String,
    /// Absolute slot of the most recent block
    pub slot: u64,
    /// Height (number) of the most recent block
    pub height: u64,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
/// Information summary of an epoch
pub struct EpochInfo {
    /// Epoch number
    pub epoch_no: i64,
    /// Total fees collected in the epoch
    pub fees: String,
    /// Total transactions in the epoch
    pub tx_count: i64,
    /// Total blocks in the epoch
    pub blk_count: i64,
    /// UNIX timestamp when the epoch began
    pub start_time: i64,
    /// UNIX timestamp when the epoch ended
    pub end_time: i64,
    /// Total active stake for the epoch
    pub active_stake: Option<String>,
    /// Total rewards earned by block producers during the epoch
    pub total_rewards: Option<String>,
    /// Average reward for block producers during the epoch
    pub average_reward: Option<String>,
}

impl<'r> FromRow<'r, PgRow> for EpochInfo {
    fn from_row(row: &'r PgRow) -> Result<Self, sqlx::Error> {
        // `no`, `tx_count` and `blk_count` are word31type (int4) columns
        let epoch_no = row.try_get::<i32, _>("no")?.into();
        let fees = get_string_from_numeric(row, "fees")?;
        let tx_count = row.try_get::<i32, _>("tx_count")?.into();
        let blk_count = row.try_get::<i32, _>("blk_count")?.into();
        let start_time = row
            .try_get::<NaiveDateTime, _>("start_time")?
            .and_utc()
            .timestamp();
        let end_time = row
            .try_get::<NaiveDateTime, _>("end_time")?
            .and_utc()
            .timestamp();

        Ok(EpochInfo {
            epoch_no,
            fees,
            tx_count,
            blk_count,
            start_time,
            end_time,
            active_stake: Default::default(),
            total_rewards: Default::default(),
            average_reward: Default::default(),
        })
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
/// Information summary of the current epoch
pub struct CurrentEpochInfo {
    /// Epoch number
    pub epoch_no: i64,
    /// Total fees collected in the epoch so far
    pub fees: String,
    /// Total transactions in the epoch so far
    pub tx_count: i64,
    /// Total blocks in the epoch so far
    pub blk_count: i64,
    /// UNIX timestamp when the epoch began
    pub start_time: i64,
}

impl<'r> FromRow<'r, PgRow> for CurrentEpochInfo {
    fn from_row(row: &'r PgRow) -> Result<Self, sqlx::Error> {
        // `no`, `tx_count` and `blk_count` are word31type (int4) columns
        let epoch_no = row.try_get::<i32, _>("no")?.into();
        let fees = get_string_from_numeric(row, "fees")?;
        let tx_count = row.try_get::<i32, _>("tx_count")?.into();
        let blk_count = row.try_get::<i32, _>("blk_count")?.into();
        let start_time = row
            .try_get::<NaiveDateTime, _>("start_time")?
            .and_utc()
            .timestamp();

        Ok(CurrentEpochInfo {
            epoch_no,
            fees,
            tx_count,
            blk_count,
            start_time,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Withdrawal {
    pub stake_address: String,
    pub amount: NumOrString,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
/// Execution unit budget for executing a redeemer
pub struct ExUnits {
    pub mem: u64,
    pub steps: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
/// Redeemer tag
#[serde(rename_all = "lowercase")]
pub enum RedeemerTag {
    Spend,
    Mint,
    Cert,
    Wdrl,
    Vote,
    Propose,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
/// Identifier of an evaluated redeemer and the execution units required to execute it
pub struct EvaluatedRedeemer {
    pub redeemer_tag: RedeemerTag,
    /// Index for the redeemer tag (which input, policy, etc)
    pub redeemer_index: usize,
    pub ex_units: ExUnits,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Data {
    pub hash: String,
    #[schema(value_type = Object)]
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Redeemer {
    pub purpose: String,
    pub ex_units: ExUnits,
    pub data: Data,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
/// Certificates found in a transaction
pub struct Certificates {
    /// Stake key registration certificates
    pub stake_registrations: Vec<StakeRegCert>,
    /// Stake key deregistration certificates
    pub stake_deregistrations: Vec<StakeRegCert>,
    /// Stake key delegation certificates
    pub stake_delegations: Vec<StakeDelegCert>,
    /// Stake pool registration certificates
    pub pool_registrations: Vec<PoolRegCert>,
    /// Stake pool retirement certificates
    pub pool_retirements: Vec<PoolRetireCert>,
    pub reg_certs: Vec<RegCert>,
    pub unreg_certs: Vec<UnRegCert>,
    pub vote_delegations: Vec<VoteDelegCert>,
    pub stake_vote_delegations: Vec<StakeVoteDelegCert>,
    pub stake_reg_delegations: Vec<StakeRegDelegCert>,
    pub vote_reg_delegations: Vec<VoteRegDelegCert>,
    pub stake_vote_reg_delegations: Vec<StakeVoteRegDelegCert>,
    pub auth_committee_hot_certs: Vec<AuthCommitteeHotCert>,
    pub resign_committee_cold_certs: Vec<ResignCommitteeColdCert>,
    pub reg_drep_certs: Vec<RegDRepCert>,
    pub unreg_drep_certs: Vec<UnRegDRepCert>,
    pub update_drep_certs: Vec<UpdateDRepCert>,
    /// Instantaneous rewards certificates
    pub mir_transfers: Vec<MirCert>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
/// Certificate for registering a stake key
pub struct StakeRegCert {
    /// Index of the certificate in the transaction
    pub cert_index: u64,
    /// Stake address corresponding to stake key being updated
    pub stake_address: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
/// Certificate for stake key delegation
pub struct StakeDelegCert {
    /// Index of the certificate in the transaction
    pub cert_index: u64,
    /// Stake address corresponding to stake key being delegated
    pub stake_address: String,
    /// Pool ID of the stake pool the stake key is delegating to
    pub pool_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
/// Certificate for registering or updating a stake pool
pub struct PoolRegCert {
    /// Index of the certificate in the transaction
    pub cert_index: u64,
    /// Pool ID of the stake pool being updated
    pub pool_id: String,
    /// Epoch at which the update will become active
    pub from_epoch: u64,
    /// VRF key hash of the stake pool
    pub vrf_key_hash: String,
    /// Pool margin
    pub margin: NumOrString,
    /// Pool fixed cost
    pub fixed_cost: NumOrString,
    /// Pool pledge
    pub pledge: NumOrString,
    /// Stake address which will receive rewards from the stake pool
    pub reward_address: String,
    /// Stake addresses which control the stake pool
    pub owner_addresses: Vec<String>,
    /// Relays declared by the stake pool
    pub relays: Vec<Relay>,
    /// URL pointing to pool metadata declared by the stake pool
    pub metadata_url: Option<String>,
    /// Hash of metadata that the metadata URL should point to
    pub metadata_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
/// Certificate for retiring a stake pool
pub struct PoolRetireCert {
    /// Index of the certificate in the transaction
    pub cert_index: u64,
    /// Bech32 pool ID of the pool being retired
    pub pool_id: String,
    /// Pool will be retired at the end of this epoch
    pub after_epoch: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
/// The pot from which an MIR reward is being funded by
pub enum MirSource {
    Reserves,
    Treasury,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
/// Where the MIR reward(s) are being sent
pub enum MirTarget {
    Reserves,
    Treasury,
    Accounts,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
/// Certificate for sending an instantaneous reward (moving funds from treasury or reserves pot to the other pot or to stake accounts)
pub struct MirCert {
    /// Index of the certificate in the transaction
    pub cert_index: u64,
    /// Pot from which the reward funds are being sent from ('reserves' or 'treasury')
    pub from: MirSource,
    /// Where the reward(s) are being sent to ('reserves', 'treasury' or 'accounts')
    pub to: MirTarget,
    /// Amount transfered to the other pot (null if `to` is 'accounts')
    pub other_pot: Option<u64>,
    /// List of stake accounts with corresponding reward amounts (null if `to` is 'reserves' or 'treasury')
    pub accounts: Option<Vec<Withdrawal>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
/// Registers stake credentials
pub struct RegCert {
    /// Index of the certificate in the transaction
    pub cert_index: u64,
    /// Stake address corresponding to stake key being updated
    pub stake_address: String,
    /// Stake registration deposit
    pub deposit: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
/// Unregisters stake credentials
pub struct UnRegCert {
    /// Index of the certificate in the transaction
    pub cert_index: u64,
    /// Stake address corresponding to stake key being updated
    pub stake_address: String,
    /// Stake registration deposit to be returned
    pub deposit: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
/// DRep
pub struct DRep {
    pub kind: DRepKind,
    /// DRep credential if kind is `Credential`
    pub credential: Option<DRepCredential>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DRepKind {
    Credential,
    Abstain,
    NoConfidence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
/// Payment credential, the payment part of a Cardano address
pub struct DRepCredential {
    pub kind: DRepCredKind,
    /// Bech32-encoding of the credential key hash or script hash
    pub bech32: String,
    /// Hex-encoding of the script or key credential
    pub hex: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum DRepCredKind {
    Key,
    Script,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
/// Delegates votes
pub struct VoteDelegCert {
    /// Index of the certificate in the transaction
    pub cert_index: u64,
    /// Stake address corresponding to stake key being updated
    pub stake_address: String,
    /// DRep being delegated to
    pub drep: DRep,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
/// Delegates to a stake pool and a DRep from the same certificate
pub struct StakeVoteDelegCert {
    /// Index of the certificate in the transaction
    pub cert_index: u64,
    /// Stake address corresponding to stake key being updated
    pub stake_address: String,
    /// Pool ID of the stake pool the stake key is delegating to
    pub pool_id: String,
    /// DRep being delegated to
    pub drep: DRep,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
/// Registers stake credentials and delegates to a stake pool
pub struct StakeRegDelegCert {
    /// Index of the certificate in the transaction
    pub cert_index: u64,
    /// Stake address corresponding to stake key being updated
    pub stake_address: String,
    /// Pool ID of the stake pool the stake key is delegating to
    pub pool_id: String,
    /// Stake registration deposit
    pub deposit: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
/// Registers stake credentials and delegates to a DRep
pub struct VoteRegDelegCert {
    /// Index of the certificate in the transaction
    pub cert_index: u64,
    /// Stake address corresponding to stake key being updated
    pub stake_address: String,
    /// DRep being delegated to
    pub drep: DRep,
    /// Stake registration deposit
    pub deposit: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
/// Registers stake credentials, delegates to a pool, and to a DRep
pub struct StakeVoteRegDelegCert {
    /// Index of the certificate in the transaction
    pub cert_index: u64,
    /// Stake address corresponding to stake key being updated
    pub stake_address: String,
    /// Pool ID of the stake pool the stake key is delegating to
    pub pool_id: String,
    /// DRep being delegated to
    pub drep: DRep,
    /// Stake registration deposit
    pub deposit: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
/// Authorizes the constitutional committee hot credential
pub struct AuthCommitteeHotCert {
    /// Index of the certificate in the transaction
    pub cert_index: u64,
    pub committee_cold_credential: String,
    pub committee_hot_credential: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
/// Authorizes the constitutional committee hot credential
pub struct ResignCommitteeColdCert {
    /// Index of the certificate in the transaction
    pub cert_index: u64,
    pub committee_cold_credential: String,
    pub anchor: Option<Anchor>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Anchor {
    /// URL
    pub url: String,
    /// Hash of data at URL
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
/// Registers DRep's credentials
pub struct RegDRepCert {
    /// Index of the certificate in the transaction
    pub cert_index: u64,
    /// DRep credential being registered
    pub drep_credential: DRepCredential,
    /// Registration deposit
    pub deposit: String,
    /// Metadata anchor
    pub anchor: Option<Anchor>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
/// Unregisters (retires) DRep's credentials
pub struct UnRegDRepCert {
    /// Index of the certificate in the transaction
    pub cert_index: u64,
    /// DRep credential being registered
    pub drep_credential: DRepCredential,
    /// Registration deposit to be returned
    pub deposit: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
/// Unregisters (retires) DRep's credentials
pub struct UpdateDRepCert {
    /// Index of the certificate in the transaction
    pub cert_index: u64,
    /// DRep credential being registered
    pub drep_credential: DRepCredential,
    /// Metadata anchor
    pub anchor: Option<Anchor>,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]

pub struct Redeemers {
    /// Redeemers attempting to spend a UTxO locked at a script
    pub spends: Vec<SpendRedeemer>,
    /// Redeemers attempting to mint assets of a script-controlled policy ID
    pub mints: Vec<MintRedeemer>,
    /// Redeemers attempting to withdraw rewards for a script-controlled stake account
    pub withdrawals: Vec<WdrlRedeemer>,
    /// Redeemers attempting to delegate or deregister a script-controlled stake account
    pub certificates: Vec<CertRedeemer>,
    /// Redeemers attempting to cast a vote for a script-controlled stake account
    pub votes: Vec<VoteRedeemer>,
    /// Redeemers attempting to submit a proposal
    pub proposals: Vec<ProposalRedeemer>,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SpendRedeemer {
    /// Script being executed
    pub script_hash: String,
    /// Transaction input that the redeemer is attempting to spend
    #[schema(inline)]
    pub input: UtxoRef,
    /// Position of input within the lexicographically sorted transaction inputs
    pub input_index: usize,
    /// Redeemer Plutus data
    pub data: Datum,
    /// Execution unit budget (memory, steps)
    pub ex_units: [u64; 2],
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]

pub struct MintRedeemer {
    /// Asset policy (hash of script being executed)
    pub policy: String,
    /// Redeemer Plutus data
    pub data: Datum,
    /// Execution unit budget (memory, steps)
    pub ex_units: [u64; 2],
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]

pub struct WdrlRedeemer {
    /// Stake account that the redeemer is attempting to withdraw rewards from
    pub stake_address: String,
    /// Redeemer Plutus data
    pub data: Datum,
    /// Execution unit budget (memory, steps)
    pub ex_units: [u64; 2],
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]

pub struct CertRedeemer {
    /// Position of certificate redeemer attempting to authenticate in sorted certificates
    pub cert_index: usize,
    /// Redeemer Plutus data
    pub data: Datum,
    /// Execution unit budget (memory, steps)
    pub ex_units: [u64; 2],
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]

pub struct VoteRedeemer {
    /// Index of relevant vote in sorted votes
    pub vote_index: usize,
    /// Redeemer Plutus data
    pub data: Datum,
    /// Execution unit budget (memory, steps)
    pub ex_units: [u64; 2],
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]

pub struct ProposalRedeemer {
    /// Index of the relevant proposal
    pub proposal_index: usize,
    /// Redeemer Plutus data
    pub data: Datum,
    /// Execution unit budget (memory, steps)
    pub ex_units: [u64; 2],
}

#[derive(Debug, PartialEq, Serialize, Deserialize, ToSchema)]
/// Transaction Information
pub struct TransactionInfo {
    /// Transaction hash (identifier)
    pub tx_hash: String,
    /// Hash of the block which includes the transaction
    pub block_hash: String,
    /// The transaction's position within the block which includes it
    pub block_tx_index: u16,
    /// Block height (number) of the block which includes the transaction
    pub block_height: u64,
    /// UNIX timestamp of the block which includes the transaction
    pub block_timestamp: u64,
    /// Absolute slot of the block which includes the transaction
    pub block_absolute_slot: u64,
    /// Epoch in which the block was minted in
    pub block_epoch: u64,
    /// Transaction inputs (lexicographically sorted)
    pub inputs: Vec<Utxo>,
    /// Transaction outputs
    pub outputs: Vec<Utxo>,
    /// Reference inputs
    pub reference_inputs: Vec<Utxo>,
    /// Collateral inputs, to be taken if Plutus scripts are not successful
    pub collateral_inputs: Vec<Utxo>,
    /// Collateral output to return excess funds from the collateral inputs
    pub collateral_return: Option<Utxo>,
    /// Native assets minted or burned by the transaction
    pub mint: Vec<MintAsset>,
    /// The slot before which the transaction would not be accepted onto the chain
    pub invalid_before: Option<u64>,
    /// The slot from which the transaction would not be accepted onto the chain
    pub invalid_hereafter: Option<u64>,
    /// The fee specified in the transaction
    pub fee: u64,
    /// The amount of lovelace used for deposits (negative if being returned)
    pub deposit: i64,
    /// Certificates in the transaction
    pub certificates: Certificates,
    /// Stake account withdrawals
    pub withdrawals: Vec<Withdrawal>,
    /// Additional required signers
    pub additional_signers: Vec<String>,
    /// Native and Plutus scripts which were executed while processing the transaction
    pub scripts_executed: Vec<Script>,
    /// False if any executed Plutus scripts failed (aka phase-two validity), meaning collateral was processed.
    pub scripts_successful: bool,
    /// Redeemers in the transaction
    pub redeemers: Redeemers,
    /// Transaction metadata JSON
    #[schema(value_type = Option<Object>)]
    pub metadata: Option<serde_json::Value>,
    /// Size of the transaction in bytes
    pub size: u64,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
/// Information decoded from a Cardano address
pub struct AddressInfo {
    pub bech32: Option<String>,
    pub hex: String,
    pub network: Option<NetworkId>,
    pub payment_cred: Option<PaymentCredential>,
    pub staking_cred: Option<StakingCredential>,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum NetworkId {
    Mainnet,
    Testnet,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
/// Payment credential, the payment part of a Cardano address
pub struct PaymentCredential {
    pub kind: PaymentCredKind,
    /// Bech32-encoding of the credential key hash or script hash
    pub bech32: String,
    /// Hex-encoding of the script or key credential
    pub hex: String,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum PaymentCredKind {
    Key,
    Script,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
/// Payment credential, the payment part of a Cardano address
pub struct Balance {
    /// Total amount of lovelace in controlled UTxOs
    pub lovelace: String,
    /// Total amount of different native assets in controlled UTxOs, as a map of minting policy to asset names to amounts
    pub assets: HashMap<String, HashMap<String, String>>,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
/// Staking credential, the delegation part of a Cardano address
pub struct StakingCredential {
    /// Details if the credential a key, script or a pointer to a registration cert
    pub kind: StakingCredKind,
    /// Bech32-encoding of the credential key hash or script hash
    pub bech32: Option<String>,
    pub reward_address: Option<String>,
    pub hex: Option<String>,
    pub pointer: Option<Pointer>,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum StakingCredKind {
    Key,
    Script,
    Pointer,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Pointer {
    pub slot: u64,
    pub tx_index: u64,
    pub cert_index: u64,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Cip68AssetType {
    ReferenceNft,
    UserNft,
    UserFt,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Cip68Metadata {
    /// Signals if this asset is a CIP-68 "reference NFT" or a "user token"
    pub purpose: Cip68AssetType,
    /// CIP-68 version
    pub version: u64,
    #[schema(value_type = Object)]
    /// Asset CIP-68 metadata
    pub metadata: serde_json::Value,
    /// Custom user defined Plutus data CBOR bytes
    pub extra: Option<String>,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum LedgerEra {
    Byron,
    Shelley,
    Allegra,
    Mary,
    Alonzo,
    Vasil,
    Valentine,
    Conway,
    NotRecognised,
}

impl LedgerEra {
    pub fn from_major_prot_ver(maj: u64) -> Self {
        match maj {
            1 => Self::Byron,
            2 => Self::Shelley,
            3 => Self::Allegra,
            4 => Self::Mary,
            5 => Self::Alonzo,
            7 => Self::Vasil,
            8 => Self::Valentine,
            9 => Self::Conway,
            _ => Self::NotRecognised,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct OperationalCert {
    pub hot_vkey: String,
    pub sequence_number: u64,
    pub kes_period: u64,
    pub kes_signature: String,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, ToSchema)]
/// Block information
pub struct BlockInfo {
    /// Block hash
    pub hash: String,
    /// Block height (number)
    pub height: u64,
    /// Absolute slot when block was minted
    pub absolute_slot: u64,
    /// UTC timestamp when the block was minted
    pub timestamp: String,
    /// Epoch in which block was minted
    pub epoch: u64,
    /// Epoch slot at which block was minted
    pub epoch_slot: u64,
    /// Identifier of stake pool which minted the block
    pub block_producer: Option<String>,
    /// Number of blocks which have been minted since the block
    pub confirmations: u64,
    /// Ordered transaction hashes for the transactions in the block
    pub tx_hashes: Vec<String>,
    /// Total transaction fees collected for all transactions minted in the block
    pub total_fees: NumOrString,
    /// Summed execution units budgets for all transactions in the block
    pub total_ex_units: ExUnits,
    /// Number of script invocations
    pub script_invocations: u32,
    /// Size of the block body in bytes
    pub size: u32,
    /// Block hash of the previous block
    pub previous_block: Option<String>,
    /// Block hash of the next block
    pub next_block: Option<String>,
    /// Total lovelace in outputs of transactions included in the block
    pub total_output_lovelace: String,
    /// Ledger era used by the block
    pub era: LedgerEra,
    /// Ledger protocol version (major, minor)
    pub protocol_version: (u64, u64),
    /// Null for Byron
    pub vrf_key: Option<String>,
    /// Null for Byron
    pub operational_certificate: Option<OperationalCert>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
/// Details of the most recent block processed by the indexer (aka chain tip); that is, the data returned is correct as of this block in time.
pub struct LastUpdated {
    /// UTC timestamp of when the most recently processed block was minted
    pub timestamp: String,
    /// Hex-encoded hash of the most recently processed block (aka chain tip)
    pub block_hash: String,
    /// Absolute slot of the most recently processed block (aka chain tip)
    pub block_slot: u64,
}

impl LastUpdated {
    pub fn new(chain: &ChainInfo, hash: String, slot: u64) -> Self {
        let genesis = match chain.magic {
            MAINNET_MAGIC => GenesisValues::mainnet(),
            PRE_PRODUCTION_MAGIC => GenesisValues::preprod(),
            PREVIEW_MAGIC => GenesisValues::preview(),
            _ => unreachable!(),
        };

        let timestamp = slot_to_timestamp_str(&genesis, slot);

        LastUpdated {
            timestamp,
            block_hash: hash,
            block_slot: slot,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[aliases(
    PaginatedAddress = PaginatedResponse<Address>,
    PaginatedAsset = PaginatedResponse<Asset>,
    PaginatedAccountHistory = PaginatedResponse<AccountHistory>,
    PaginatedAccountReward = PaginatedResponse<AccountReward>,
    PaginatedAccountUpdate = PaginatedResponse<AccountUpdate>,
    PaginatedAccountDelegation = PaginatedResponse<AccountDelegation>,
    PaginatedAddressTransaction = PaginatedResponse<AddressTransaction>,
    PaginatedUtxoRef = PaginatedResponse<UtxoRef>,
    PaginatedUtxoWithSlot = PaginatedResponse<UtxoWithSlot>,
    PaginatedUtxoWithBytes = PaginatedResponse<UtxoWithBytes>,
    PaginatedPaymentCredentialTransaction = PaginatedResponse<PaymentCredentialTransaction>,
    PaginatedPaymentCredentialsTransaction = PaginatedResponse<PaymentCredentialsTransaction>,
    PaginatedAssetHolderAccount = PaginatedResponse<AssetHolderAccount>,
    PaginatedAssetHolder = PaginatedResponse<AssetHolder>,
    PaginatedAssetTx = PaginatedResponse<AssetTx>,
    PaginatedTimestampedTransaction = PaginatedResponse<TimestampedTransaction>,
    PaginatedMintTransaction = PaginatedResponse<MintTransaction>,
    PaginatedPolicyMintTransaction = PaginatedResponse<PolicyMintTransaction>,
    PaginatedMintingTx = PaginatedResponse<MintingTx>,
    PaginatedAssetUtxo = PaginatedResponse<AssetUtxo>,
    PaginatedPolicyHolderAccount = PaginatedResponse<PolicyHolderAccount>,
    PaginatedPolicyHolder = PaginatedResponse<PolicyHolder>,
    PaginatedPolicyTransaction = PaginatedResponse<PolicyTransaction>,
    PaginatedAssetInfoOld = PaginatedResponse<AssetInfoOld>,
    PaginatedAssetInfoConcise = PaginatedResponse<AssetInfoConcise>,
    PaginatedPolicyUtxo = PaginatedResponse<PolicyUtxo>,
    PaginatedPoolListInfo = PaginatedResponse<PoolListInfo>,
    PaginatedPoolBlock = PaginatedResponse<PoolBlock>,
    PaginatedDelegatorInfo = PaginatedResponse<DelegatorInfo>,
    PaginatedHistoricalDelegatorInfo = PaginatedResponse<HistoricalDelegatorInfo>,
    PaginatedPoolHistory = PaginatedResponse<PoolHistory>,
)]
/// A paginated response. Pass in the `next_cursor` in a subsequent request as the `cursor` query parameter to fetch the next page of results.
pub struct PaginatedResponse<T> {
    /// Endpoint response data
    pub data: Vec<T>,
    /// Indexer chain-tip (data was correct as of this block)
    pub last_updated: LastUpdated,
    /// Pagination cursor
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[aliases(
    TimestampedAccountInfo = TimestampedResponse<AccountInfo>,
    TimestampedBalance = TimestampedResponse<Balance>,
    TimestampedTxCount = TimestampedResponse<TxCount>,
    TimestampedAssetInfo = TimestampedResponse<AssetInfo>,
    TimestampedPolicyInfo = TimestampedResponse<PolicyInfo>,
    TimestampedBlockInfo = TimestampedResponse<BlockInfo>,
    TimestampedAddress = TimestampedResponse<Address>,
    TimestampedCurrentEpochInfo = TimestampedResponse<CurrentEpochInfo>,
    TimestampedEpochInfo = TimestampedResponse<EpochInfo>,
    TimestampedPoolInfo = TimestampedResponse<PoolInfo>,
    TimestampedPoolMetadata = TimestampedResponse<PoolMetadata>,
    TimestampedPoolRelays = TimestampedResponse<PoolRelays>,
    TimestampedPoolUpdates = TimestampedResponse<PoolUpdates>,
    TimestampedScript = TimestampedResponse<Script>,
    TimestampedScriptFirstSeen = TimestampedResponse<ScriptFirstSeen>,
    TimestampedTxCbor = TimestampedResponse<TxCbor>,
    TimestampedTransactionInfo = TimestampedResponse<TransactionInfo>,
    TimestampedUtxo = TimestampedResponse<Utxo>,
    TimestampedChainTip = TimestampedResponse<ChainTip>,
    TimestampedEraSummaries = TimestampedResponse<Eras>,
    TimestampedDatum = TimestampedResponse<Datum>,
    TimestampedDatumMap = TimestampedResponse<HashMap<String, Datum>>,
    TimestampedProtocolParametersLegacy = TimestampedResponse<ProtocolParametersV5>,
    TimestampedProtocolParameters = TimestampedResponse<ProtocolParametersV6>,
    TimestampedSystemStart = TimestampedResponse<SystemStart>,
)]
/// Timestamped response. Returns the endpoint response data along with the chain-tip of the indexer, which details at which point in the chain's history the data was correct as-of.
pub struct TimestampedResponse<T> {
    /// Endpoint response data
    pub data: T,
    /// Indexer chain-tip (data was correct as of this block)
    pub last_updated: LastUpdated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Ada {
    pub lovelace: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
/// Minimum ADA fee
pub struct MinFeeConstant {
    /// Minimum ADA fee
    pub ada: Ada,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
/// Maximum number of bytes allowed for a block body
pub struct MaxBlockBodySize {
    /// Maximum number of bytes allowed for a block body
    pub bytes: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
/// Maximum number of bytes allowed for a block header
pub struct MaxBlockHeaderSize {
    /// Maximum number of bytes allowed for a block header
    pub bytes: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
/// Maximum number of bytes allowed for a transaction
pub struct MaxTransactionSize {
    /// Maximum number of bytes allowed for a transaction
    pub bytes: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
/// Stake deposit amount
pub struct StakeDeposit {
    /// Stake deposit amount
    pub ada: Ada,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
/// Minimum stake pool cost
pub struct MinStakePoolCost {
    /// Minimum stake pool cost
    pub ada: Ada,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
/// Minimum UTxO deposit amount
pub struct MinUtxoDepositConstant {
    /// Minimum UTxO deposit amount
    pub ada: Ada,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
/// Plutus script execution cost models
pub struct PlutusCostModels {
    #[serde(rename(deserialize = "plutus:v1"))]
    /// Plutus script v1 execution cost model
    pub plutus_v1: Vec<i64>,
    #[serde(rename(deserialize = "plutus:v2"))]
    /// Plutus script v2 execution cost model
    pub plutus_v2: Vec<i64>,
    #[serde(rename(deserialize = "plutus:v3"))]
    /// Plutus script v3 execution cost model (introduced in Conway)
    pub plutus_v3: Option<Vec<i64>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
/// Script execution prices
pub struct ScriptExecutionPrices {
    /// Script execution memory price
    pub memory: String,
    /// Script execution CPU price
    pub cpu: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
/// Maximum execution units
pub struct MaxExecutionUnits {
    /// Maximum execution memory units
    pub memory: i64,
    /// Maximum execution CPU units
    pub cpu: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
/// Maximum value size
pub struct MaxValueSize {
    /// Maximum value size
    pub bytes: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all(deserialize = "camelCase"))]
pub struct ProtocolParametersV6 {
    pub min_fee_coefficient: i64,
    pub min_fee_constant: MinFeeConstant,
    pub min_fee_reference_scripts: Option<MinFeeReferenceScripts>,
    pub max_block_body_size: MaxBlockBodySize,
    pub max_block_header_size: MaxBlockHeaderSize,
    pub max_transaction_size: MaxTransactionSize,
    pub max_reference_scripts_size: Option<MaxReferenceScriptsSize>,
    pub stake_credential_deposit: StakeDeposit,
    pub stake_pool_deposit: StakeDeposit,
    pub stake_pool_retirement_epoch_bound: i64,
    pub desired_number_of_stake_pools: i64,
    pub stake_pool_pledge_influence: String,
    pub monetary_expansion: String,
    pub treasury_expansion: String,
    pub min_stake_pool_cost: MinStakePoolCost,
    pub min_utxo_deposit_constant: MinUtxoDepositConstant,
    pub min_utxo_deposit_coefficient: i64,
    pub plutus_cost_models: PlutusCostModels,
    pub script_execution_prices: ScriptExecutionPrices,
    pub max_execution_units_per_transaction: MaxExecutionUnits,
    pub max_execution_units_per_block: MaxExecutionUnits,
    pub max_value_size: MaxValueSize,
    pub collateral_percentage: i64,
    pub max_collateral_inputs: i64,
    pub version: Version,
    pub stake_pool_voting_thresholds: Option<StakePoolVotingThresholds>,
    pub delegate_representative_voting_thresholds: Option<DelegateRepresentativeVotingThresholds>,
    pub constitutional_committee_min_size: Option<i64>,
    pub constitutional_committee_max_term_length: Option<i64>,
    pub governance_action_lifetime: Option<i64>,
    pub governance_action_deposit: Option<GovernanceActionDeposit>,
    pub delegate_representative_deposit: Option<DelegateRepresentativeDeposit>,
    pub delegate_representative_max_idle_time: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
/// Parameters for reference script fee calculation (introduced in Conway)
pub struct MinFeeReferenceScripts {
    pub base: f64,
    pub range: i64,
    pub multiplier: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
/// Maximum reference script size (introduced in Conway)
pub struct MaxReferenceScriptsSize {
    pub bytes: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all(deserialize = "camelCase"))]
/// Stake pool voting thresholds (introduced in Conway)
pub struct StakePoolVotingThresholds {
    pub no_confidence: String,
    pub constitutional_committee: ConstitutionalCommittee,
    pub hard_fork_initiation: String,
    pub protocol_parameters_update: ProtocolParametersUpdateStakePool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all(deserialize = "camelCase"))]
pub struct ConstitutionalCommittee {
    pub default: String,
    pub state_of_no_confidence: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ProtocolParametersUpdateStakePool {
    pub security: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ProtocolParametersUpdateDRep {
    pub network: String,
    pub economic: String,
    pub technical: String,
    pub governance: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all(deserialize = "camelCase"))]
/// DRep voting thresholds (introduced in Conway)
pub struct DelegateRepresentativeVotingThresholds {
    pub no_confidence: String,
    pub constitutional_committee: ConstitutionalCommittee,
    pub constitution: String,
    pub hard_fork_initiation: String,
    pub protocol_parameters_update: ProtocolParametersUpdateDRep,
    pub treasury_withdrawals: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
/// Governance action deposit (introduced in Conway)
pub struct GovernanceActionDeposit {
    pub ada: Ada,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
/// DRep action deposit (introduced in Conway)
pub struct DelegateRepresentativeDeposit {
    pub ada: Ada,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Eras(pub Vec<Era>);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Era {
    pub start: Start,
    pub end: End,
    pub parameters: Parameters,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Start {
    pub time: Time,
    pub slot: i64,
    pub epoch: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct End {
    pub time: Time,
    pub slot: i64,
    pub epoch: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Time {
    pub seconds: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all(deserialize = "camelCase"))]
pub struct Parameters {
    pub epoch_length: i64,
    pub slot_length: SlotLength,
    pub safe_zone: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SlotLength {
    pub milliseconds: i64,
}

#[cfg(test)]
mod tests {
    use super::{CurrentEpochInfo, EpochInfo};

    // Regression guard for MSTR-36: db-sync 13.7.2.1 widened the epoch columns
    // `no`, `tx_count`, `blk_count` from int4 to int8. These fields must be i64
    // so values exceeding i32::MAX both compile and round-trip through serde.
    const OVER_I32: i64 = i32::MAX as i64 + 1;

    #[test]
    fn epoch_info_holds_int8_columns() {
        let info = EpochInfo {
            epoch_no: OVER_I32,
            fees: "0".into(),
            tx_count: OVER_I32,
            blk_count: OVER_I32,
            start_time: 0,
            end_time: 0,
            active_stake: None,
            total_rewards: None,
            average_reward: None,
        };

        let json = serde_json::to_value(&info).unwrap();
        assert_eq!(json["epoch_no"], OVER_I32);
        assert_eq!(json["tx_count"], OVER_I32);
        assert_eq!(json["blk_count"], OVER_I32);
    }

    #[test]
    fn current_epoch_info_holds_int8_columns() {
        let info = CurrentEpochInfo {
            epoch_no: OVER_I32,
            fees: "0".into(),
            tx_count: OVER_I32,
            blk_count: OVER_I32,
            start_time: 0,
        };

        let json = serde_json::to_value(&info).unwrap();
        assert_eq!(json["epoch_no"], OVER_I32);
        assert_eq!(json["tx_count"], OVER_I32);
        assert_eq!(json["blk_count"], OVER_I32);
    }
}
