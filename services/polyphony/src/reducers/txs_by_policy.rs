/*
    Txs By Policy

    For every transaction, create outputs for every policy of native asset that
    was moved including the names of the assets. It scans the mint field and the
    outputs of valid transactions (collateral inputs cannot contain assets).
*/

use pallas::ledger::primitives::babbage::PolicyId;
use pallas::ledger::traverse::{MultiEraBlock, MultiEraTx};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};

use super::ReducerOutput;

#[derive(Deserialize)]
pub struct Config {}

pub struct Reducer;

#[derive(Clone, Debug)]
pub struct Output {
    pub policy: PolicyId,
    pub slot: u64,
    pub tx_hash: [u8; 32],
    pub tx_block_index: u16,
    pub assets: Vec<Vec<u8>>,
}

impl Reducer {
    fn process_tx(
        &mut self,
        tx: &MultiEraTx,
        seen: &mut HashMap<PolicyId, HashSet<Vec<u8>>>,
    ) -> Result<(), gasket::error::Error> {
        for policy in tx.mints() {
            let set = seen.entry(*policy.policy()).or_default();

            for asset in policy.assets() {
                if asset.mint_coin().unwrap() != 0 {
                    set.insert(asset.name().to_vec());
                }
            }
        }

        for txo in tx.outputs() {
            for policy in txo.value().assets() {
                let set = seen.entry(*policy.policy()).or_default();

                for asset in policy.assets() {
                    if asset.output_coin().unwrap() != 0 {
                        set.insert(asset.name().to_vec());
                    }
                }
            }
        }

        Ok(())
    }

    pub fn reduce_block<'b>(
        &mut self,
        block: &'b MultiEraBlock<'b>,
        outputs: &mut Vec<ReducerOutput>,
    ) -> Result<(), gasket::error::Error> {
        for (idx, tx) in block.txs().into_iter().enumerate() {
            if tx.is_valid() {
                let mut seen: HashMap<PolicyId, HashSet<Vec<u8>>> = HashMap::new();

                self.process_tx(&tx, &mut seen)?;

                for (policy, assets) in seen {
                    let assets = assets.into_iter().collect::<Vec<Vec<u8>>>();

                    outputs.push(ReducerOutput::TxsByPolicy(Output {
                        policy,
                        slot: block.slot(),
                        tx_hash: *tx.hash(),
                        tx_block_index: idx as u16,
                        assets,
                    }))
                }
            }
        }

        Ok(())
    }
}

impl Config {
    pub fn plugin(self) -> super::Reducer {
        let reducer = Reducer;

        super::Reducer::TxsByPolicy(reducer)
    }
}
