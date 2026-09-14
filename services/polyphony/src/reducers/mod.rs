use gasket::runtime::spawn_stage;
use pallas::codec::utils::{Bytes, KeyValuePairs};
use pallas::crypto::hash::Hash;
use pallas::ledger::primitives::{alonzo, babbage, conway};
use pallas::ledger::traverse::MultiEraOutput;
use pallas::{ledger::traverse::MultiEraBlock, network::miniprotocols::Point};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::ops::Deref;
use std::time::Duration;

use crate::{
    bootstrap, crosscut,
    model::{self, StorageActionPayload},
};

type InputPort = gasket::messaging::tokio::InputPort<model::EnrichedBlockPayload>;
type OutputPort = gasket::messaging::tokio::OutputPort<StorageActionPayload>;

pub mod macros;

mod worker;

pub mod balance_by_address;
pub mod block_by_height;
pub mod block_by_tx;
pub mod cip25_metadata_by_asset;
pub mod datum_by_hash;
pub mod mint_metadata_by_asset;
pub mod script_by_hash;
pub mod supply_by_asset;
pub mod tx_by_hash;
pub mod tx_count_by_address;
pub mod txs_by_address;
pub mod txs_by_pay_cred;
pub mod txs_by_policy;
pub mod updates_by_policy;
pub mod utxo_cbor_by_address;
pub mod utxos_by_asset;
pub mod utxos_by_policy;

#[derive(Deserialize)]
#[serde(tag = "type")]
pub enum Config {
    BalanceByAddress(balance_by_address::Config),
    BlockByHeight(block_by_height::Config),
    BlockByTx(block_by_tx::Config),
    DatumByHash(datum_by_hash::Config),
    Cip25MetadataByAsset(cip25_metadata_by_asset::Config),
    MintMetadataByAsset(mint_metadata_by_asset::Config),
    ScriptByHash(script_by_hash::Config),
    SupplyByAsset(supply_by_asset::Config),
    TxByHash(tx_by_hash::Config),
    TxCountByAddress(tx_count_by_address::Config),
    TxsByAddress(txs_by_address::Config),
    TxsByPayCred(txs_by_pay_cred::Config),
    TxsByPolicy(txs_by_policy::Config),
    UpdatesByPolicy(updates_by_policy::Config),
    UtxoCborByAddress(utxo_cbor_by_address::Config),
    UtxosByAsset(utxos_by_asset::Config),
    UtxosByPolicy(utxos_by_policy::Config),
}

impl Config {
    /// The instance-registry name this reducer is advertised under, as used in
    /// the Redis keys (`cardano:<network>:<name>:scores`) the API layer
    /// resolves instances from. Must match the API's naming exactly. Several
    /// asset/policy reducers are grouped under one registry name.
    pub fn instance_name(&self) -> &'static str {
        match self {
            Config::BalanceByAddress(_) => "balancebyaddress",
            Config::BlockByHeight(_) => "blockbyheight",
            Config::BlockByTx(_) => "blockbytx",
            Config::DatumByHash(_) => "datumbyhash",
            Config::Cip25MetadataByAsset(_) => "assets-policies",
            Config::MintMetadataByAsset(_) => "assets-policies",
            Config::ScriptByHash(_) => "assets-policies",
            Config::SupplyByAsset(_) => "assets-policies",
            Config::UpdatesByPolicy(_) => "assets-policies",
            Config::TxByHash(_) => "txbyhash",
            Config::TxCountByAddress(_) => "txcountbyaddress",
            Config::TxsByAddress(_) => "txsbyaddress",
            Config::TxsByPayCred(_) => "txsbypaymentcred",
            Config::TxsByPolicy(_) => "txsbypolicy",
            Config::UtxoCborByAddress(_) => "utxocborbyaddress",
            Config::UtxosByAsset(_) => "utxosbyasset",
            Config::UtxosByPolicy(_) => "utxosbypolicy",
        }
    }

    fn plugin(self, policy: &crosscut::policies::RuntimePolicy) -> Reducer {
        match self {
            Config::BalanceByAddress(c) => c.plugin(policy),
            Config::BlockByHeight(c) => c.plugin(policy),
            Config::BlockByTx(c) => c.plugin(),
            Config::DatumByHash(c) => c.plugin(policy),
            Config::Cip25MetadataByAsset(c) => c.plugin(),
            Config::MintMetadataByAsset(c) => c.plugin(),
            Config::ScriptByHash(c) => c.plugin(),
            Config::SupplyByAsset(c) => c.plugin(),
            Config::TxByHash(c) => c.plugin(policy),
            Config::TxCountByAddress(c) => c.plugin(policy),
            Config::TxsByAddress(c) => c.plugin(policy),
            Config::TxsByPayCred(c) => c.plugin(policy),
            Config::TxsByPolicy(c) => c.plugin(),
            Config::UpdatesByPolicy(c) => c.plugin(),
            Config::UtxoCborByAddress(c) => c.plugin(policy),
            Config::UtxosByAsset(c) => c.plugin(policy),
            Config::UtxosByPolicy(c) => c.plugin(policy),
        }
    }
}

pub struct Bootstrapper {
    input: InputPort,
    output: OutputPort,
    reducers: Vec<Reducer>,
    policy: crosscut::policies::RuntimePolicy,
}

impl Bootstrapper {
    pub fn new(configs: Vec<Config>, policy: &crosscut::policies::RuntimePolicy) -> Self {
        Self {
            reducers: configs.into_iter().map(|x| x.plugin(policy)).collect(),
            input: Default::default(),
            output: Default::default(),
            policy: policy.clone(),
        }
    }

    pub fn borrow_input_port(&mut self) -> &'_ mut InputPort {
        &mut self.input
    }

    pub fn borrow_output_port(&mut self) -> &'_ mut OutputPort {
        &mut self.output
    }

    pub fn spawn_stages(self, pipeline: &mut bootstrap::Pipeline) {
        let worker = worker::Worker::new(self.reducers, self.input, self.output, self.policy);
        pipeline.register_stage(spawn_stage(
            worker,
            gasket::runtime::Policy {
                tick_timeout: Some(Duration::from_secs(1200)),
                ..Default::default()
            },
            Some("reducers"),
        ));
    }
}

pub enum Reducer {
    BalanceByAddress(balance_by_address::Reducer),
    BlockByHeight(block_by_height::Reducer),
    BlockByTx(block_by_tx::Reducer),
    DatumByHash(datum_by_hash::Reducer),
    Cip25MetadataByAsset(cip25_metadata_by_asset::Reducer),
    MintMetadataByAsset(mint_metadata_by_asset::Reducer),
    ScriptByHash(script_by_hash::Reducer),
    SupplyByAsset(supply_by_asset::Reducer),
    TxByHash(tx_by_hash::Reducer),
    TxCountByAddress(tx_count_by_address::Reducer),
    TxsByAddress(txs_by_address::Reducer),
    TxsByPayCred(txs_by_pay_cred::Reducer),
    TxsByPolicy(txs_by_policy::Reducer),
    UpdatesByPolicy(updates_by_policy::Reducer),
    UtxoCborByAddress(utxo_cbor_by_address::Reducer),
    UtxosByAsset(utxos_by_asset::Reducer),
    UtxosByPolicy(utxos_by_policy::Reducer),
}

#[derive(Clone, Debug)]
pub enum ReducerOutput {
    BalanceByAddress(balance_by_address::Output),
    BlockByHeight(block_by_height::Output),
    BlockByTx(block_by_tx::Output),
    DatumByHash(datum_by_hash::Output),
    Cip25MetadataByAsset(cip25_metadata_by_asset::Output),
    MintMetadataByAsset(mint_metadata_by_asset::Output),
    ScriptByHash(script_by_hash::Output),
    SupplyByAsset(supply_by_asset::Output),
    TxByHash(tx_by_hash::Output),
    TxCountByAddress(tx_count_by_address::Output),
    TxsByAddress(txs_by_address::Output),
    TxsByPayCred(txs_by_pay_cred::Output),
    TxsByPolicy(txs_by_policy::Output),
    UpdatesByPolicy(updates_by_policy::Output),
    UtxoCborByAddress(utxo_cbor_by_address::Output),
    UtxosByAsset(utxos_by_asset::Output),
    UtxosByPolicy(utxos_by_policy::Output),

    Cursor((Point, u64)),
}

impl Reducer {
    pub fn reduce_block<'b>(
        &mut self,
        block: &'b MultiEraBlock<'b>,
        ctx: &model::BlockContext,
        outputs: &mut Vec<ReducerOutput>,
    ) -> Result<(), gasket::error::Error> {
        match self {
            Reducer::BalanceByAddress(x) => x.reduce_block(block, ctx, outputs),
            Reducer::BlockByHeight(x) => x.reduce_block(block, ctx, outputs),
            Reducer::BlockByTx(x) => x.reduce_block(block, outputs),
            Reducer::DatumByHash(x) => x.reduce_block(block, ctx, outputs),
            Reducer::Cip25MetadataByAsset(x) => x.reduce_block(block, outputs),
            Reducer::MintMetadataByAsset(x) => x.reduce_block(block, outputs),
            Reducer::ScriptByHash(x) => x.reduce_block(block, outputs),
            Reducer::SupplyByAsset(x) => x.reduce_block(block, outputs),
            Reducer::TxByHash(x) => x.reduce_block(block, ctx, outputs),
            Reducer::TxCountByAddress(x) => x.reduce_block(block, ctx, outputs),
            Reducer::TxsByAddress(x) => x.reduce_block(block, ctx, outputs),
            Reducer::TxsByPayCred(x) => x.reduce_block(block, ctx, outputs),
            Reducer::TxsByPolicy(x) => x.reduce_block(block, outputs),
            Reducer::UpdatesByPolicy(x) => x.reduce_block(block, outputs),
            Reducer::UtxoCborByAddress(x) => x.reduce_block(block, ctx, outputs),
            Reducer::UtxosByAsset(x) => x.reduce_block(block, ctx, outputs),
            Reducer::UtxosByPolicy(x) => x.reduce_block(block, ctx, outputs),
        }
    }
}

#[derive(Clone, Debug)]
pub enum UtxoAction<T> {
    Consumed,
    Produced(T),
}

fn convert_multiasset<A>(
    input: &BTreeMap<Hash<28>, BTreeMap<Bytes, A>>,
) -> KeyValuePairs<Hash<28>, KeyValuePairs<Bytes, u64>>
where
    A: Copy + Into<u64>,
{
    let converted: Vec<(Hash<28>, KeyValuePairs<Bytes, u64>)> = input
        .iter()
        .map(|(k, v)| {
            (
                *k,
                v.iter()
                    .map(|(inner_k, inner_v)| (inner_k.clone(), (*inner_v).into()))
                    .collect::<Vec<_>>()
                    .into(),
            )
        })
        .collect();
    converted.into()
}

fn get_multiassets(
    txout: MultiEraOutput,
) -> Option<KeyValuePairs<Hash<28>, KeyValuePairs<Bytes, u64>>> {
    match txout {
        MultiEraOutput::Byron(_) => None,
        MultiEraOutput::Babbage(x) => match x.deref().deref() {
            babbage::MintedTransactionOutput::Legacy(x) => match &x.amount {
                babbage::Value::Coin(_) => None,
                babbage::Value::Multiasset(_, x) => Some(convert_multiasset(x)),
            },
            babbage::MintedTransactionOutput::PostAlonzo(x) => match &x.value {
                babbage::Value::Coin(_) => None,
                babbage::Value::Multiasset(_, x) => Some(convert_multiasset(x)),
            },
        },
        MultiEraOutput::Conway(x) => match x.deref().deref() {
            conway::MintedTransactionOutput::Legacy(x) => match &x.amount {
                babbage::Value::Coin(_) => None,
                babbage::Value::Multiasset(_, x) => Some(convert_multiasset(x)),
            },
            conway::MintedTransactionOutput::PostAlonzo(x) => match &x.value {
                conway::Value::Coin(_) => None,
                conway::Value::Multiasset(_, x) => Some(convert_multiasset(x)),
            },
        },
        MultiEraOutput::AlonzoCompatible(x, _) => match &x.amount {
            alonzo::Value::Coin(_) => None,
            alonzo::Value::Multiasset(_, x) => Some(convert_multiasset(x)),
        },
        x => {
            tracing::warn!("unexpected MultiEraOutput: {:?}", x);

            None
        }
    }
}
