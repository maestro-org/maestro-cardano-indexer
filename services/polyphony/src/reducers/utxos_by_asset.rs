/*
   UTxOs By Asset

   Creates reducer outputs to signal that a UTxO containing at least one of the
   asset (policy ID and asset name) was consumed or produced, along with the
   amount of the asset in the UTxO and the address which controls the UTxO.
*/

use itertools::Itertools;
use pallas::ledger::addresses::Address;
use pallas::ledger::primitives::babbage::{AssetName, PolicyId};
use pallas::ledger::traverse::{MultiEraBlock, MultiEraOutput, MultiEraTx, OutputRef};
use serde::Deserialize;

use crate::{crosscut, model, prelude::*};

use super::{get_multiassets, ReducerOutput, UtxoAction};

#[derive(Deserialize)]
pub struct Config {}

pub struct Reducer {
    policy: crosscut::policies::RuntimePolicy,
}

#[derive(Clone, Debug)]
pub struct Output {
    pub policy: PolicyId,
    pub asset_name: AssetName,
    pub slot: u64,
    pub u_hash: [u8; 32],
    pub u_index: u64,
    pub action: UtxoAction<(Address, u64)>, // utxo owner and amount of asset in UTxO
}

impl Reducer {
    fn process_consumed_txo(
        &mut self,
        ctx: &model::BlockContext,
        input: &OutputRef,
        outputs: &mut Vec<ReducerOutput>,
    ) -> Result<(), gasket::error::Error> {
        let utxo = ctx.find_utxo(input).apply_policy(&self.policy).or_panic()?;

        let (slot, utxo) = match utxo {
            Some((s, u)) => (s, u),
            None => return Ok(()),
        };

        let _address = utxo.address().map(|x| x.to_string()).or_panic()?;

        let multiassets = get_multiassets(utxo);

        if let Some(ma) = multiassets {
            for (policy, assets) in ma.iter() {
                for (asset, _amount) in assets.iter() {
                    outputs.push(ReducerOutput::UtxosByAsset(Output {
                        policy: *policy,
                        asset_name: asset.clone(),
                        slot,
                        u_hash: **input.hash(),
                        u_index: input.index(),
                        action: UtxoAction::Consumed,
                    }))
                }
            }
        }

        Ok(())
    }

    fn process_produced_txo(
        &mut self,
        tx: &MultiEraTx,
        tx_output: MultiEraOutput,
        output_idx: usize,
        chained_txos: &[OutputRef],
        outputs: &mut Vec<ReducerOutput>,
        slot: u64,
    ) -> Result<(), gasket::error::Error> {
        let output_ref = OutputRef::new(tx.hash(), output_idx as u64);

        // skip if this txo is consumed later in the block
        if chained_txos.contains(&output_ref) {
            return Ok(());
        }

        let address = tx_output.address().or_panic()?;

        let multiassets = get_multiassets(tx_output);

        if let Some(ma) = multiassets {
            for (policy, assets) in ma.iter() {
                for (asset, amount) in assets.iter() {
                    outputs.push(ReducerOutput::UtxosByAsset(Output {
                        policy: *policy,
                        asset_name: asset.clone(),
                        slot,
                        u_hash: *tx.hash(),
                        u_index: output_idx as u64,
                        action: UtxoAction::Produced((address.clone(), *amount)),
                    }))
                }
            }
        }

        Ok(())
    }

    pub fn reduce_block<'b>(
        &mut self,
        block: &'b MultiEraBlock<'b>,
        ctx: &model::BlockContext,
        outputs: &mut Vec<ReducerOutput>,
    ) -> Result<(), gasket::error::Error> {
        let consumed_txo_refs = block
            .txs()
            .iter()
            .flat_map(|tx| tx.consumes())
            .map(|x| x.output_ref())
            .collect::<Vec<_>>();

        let produced_txo_refs = block
            .txs()
            .iter()
            .flat_map(|tx| {
                tx.produces()
                    .iter()
                    .map(|(idx, _)| OutputRef::new(tx.hash(), *idx as u64))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();

        let chained_txo_refs = produced_txo_refs
            .into_iter()
            .filter(|x| consumed_txo_refs.contains(x))
            .collect::<Vec<_>>();

        for tx in block.txs().into_iter() {
            for consumed in tx
                .consumes()
                .iter()
                .map(|i| i.output_ref())
                .filter(|x| !chained_txo_refs.contains(x)) // skip utxos produced in this block
                .unique_by(|x| x.to_string())
            {
                self.process_consumed_txo(ctx, &consumed, outputs)?;
            }

            for (idx, produced) in tx.produces() {
                self.process_produced_txo(
                    &tx,
                    produced,
                    idx,
                    &chained_txo_refs,
                    outputs,
                    block.slot(),
                )?;
            }
        }

        Ok(())
    }
}

impl Config {
    pub fn plugin(self, policy: &crosscut::policies::RuntimePolicy) -> super::Reducer {
        let reducer = Reducer {
            policy: policy.clone(),
        };

        super::Reducer::UtxosByAsset(reducer)
    }
}
