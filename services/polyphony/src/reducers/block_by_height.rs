/*
   Block information by block hash or height
*/

use itertools::Itertools;
use pallas::codec::minicbor::to_vec;
use pallas::ledger::traverse::{Era, MultiEraBlock, MultiEraTx};
use serde::Deserialize;

use crate::{crosscut, model, prelude::*};

use super::ReducerOutput;

#[derive(Deserialize)]
pub struct Config {
    pub filter: Option<crosscut::filters::Predicate>,
}

pub struct Reducer {
    _config: Config,
    policy: crosscut::policies::RuntimePolicy,
}

#[derive(Clone, Debug)]
pub struct Output {
    pub hash: [u8; 32],
    pub height: u64,
    pub header_bytes: Vec<u8>,
    pub tx_hashes: Vec<[u8; 32]>,
    pub size: u32,
    pub total_lovelace_output: u128,
    pub total_fees: u64,
    pub total_script_invocations: u32,
    pub total_ex_units_mem: u64,
    pub total_ex_units_steps: u64,
}

impl Reducer {
    pub fn reduce_block<'b>(
        &mut self,
        block: &'b MultiEraBlock<'b>,
        ctx: &model::BlockContext,
        outputs: &mut Vec<ReducerOutput>,
    ) -> Result<(), gasket::error::Error> {
        let hash = *block.hash();
        let header = block.header();

        let header_bytes = header.cbor().to_vec();

        let size = match block {
            MultiEraBlock::EpochBoundary(b) => {
                to_vec(b.body.clone()).unwrap().len() + to_vec(b.extra.clone()).unwrap().len()
            }
            MultiEraBlock::AlonzoCompatible(b, _) => {
                to_vec(b.transaction_bodies.clone()).unwrap().len()
                    + to_vec(b.transaction_witness_sets.clone()).unwrap().len()
                    + to_vec(b.auxiliary_data_set.clone()).unwrap().len()
                    + to_vec(b.invalid_transactions.clone()).unwrap().len()
            }
            MultiEraBlock::Babbage(b) => {
                to_vec(b.transaction_bodies.clone()).unwrap().len()
                    + to_vec(b.transaction_witness_sets.clone()).unwrap().len()
                    + to_vec(b.auxiliary_data_set.clone()).unwrap().len()
                    + to_vec(b.invalid_transactions.clone()).unwrap().len()
            }
            MultiEraBlock::Conway(b) => {
                to_vec(b.transaction_bodies.clone()).unwrap().len()
                    + to_vec(b.transaction_witness_sets.clone()).unwrap().len()
                    + to_vec(b.auxiliary_data_set.clone()).unwrap().len()
                    + to_vec(b.invalid_transactions.clone()).unwrap().len()
            }
            MultiEraBlock::Byron(b) => {
                to_vec(b.body.clone()).unwrap().len() + to_vec(b.extra.clone()).unwrap().len()
            }
            _ => unreachable!("unexpected block kind"),
        };

        let mut total_lovelace_output = 0;
        let mut total_fees = 0;
        let mut total_script_invocations = 0;
        let mut total_ex_units_mem = 0;
        let mut total_ex_units_steps = 0;
        let mut tx_hashes = Vec::with_capacity(block.tx_count());

        for tx in block.txs() {
            if tx.is_valid() {
                for output in tx.outputs() {
                    total_lovelace_output += output.value().coin() as u128
                }

                let fees = match tx.fee() {
                    Some(c) => c,
                    None => self.compute_byron_tx_fee(ctx, &tx)?,
                };

                total_fees += fees;
            } else {
                // the collateral is the fee if tx invalid
                total_fees += self.compute_total_collateral(ctx, &tx)?;
            }

            // --- ex units

            for rdmr in tx.redeemers() {
                total_script_invocations += 1;
                total_ex_units_mem += rdmr.ex_units().mem;
                total_ex_units_steps += rdmr.ex_units().steps;
            }

            // --- store tx hash

            tx_hashes.push(*tx.hash())
        }

        outputs.push(ReducerOutput::BlockByHeight(Output {
            hash,
            height: header.number(),
            header_bytes,
            tx_hashes,
            size: size as u32,
            total_lovelace_output,
            total_fees,
            total_script_invocations,
            total_ex_units_mem,
            total_ex_units_steps,
        }));
        Ok(())
    }

    /// Computes the tx fee for a Byron era transaction
    fn compute_byron_tx_fee(
        &self,
        ctx: &model::BlockContext,
        tx: &MultiEraTx,
    ) -> Result<u64, gasket::error::Error> {
        assert_eq!(
            tx.era(),
            Era::Byron,
            "Using compute_byron_tx_fee for non-Byron tx"
        );

        let mut consumed = 0;
        let mut produced = 0;

        for txo_ref in tx
            .consumes()
            .iter()
            .map(|i| i.output_ref())
            .unique_by(|x| x.to_string())
        {
            let utxo = ctx
                .find_utxo(&txo_ref)
                .apply_policy(&self.policy)
                .or_panic()?;

            let ada_consumed = match utxo {
                Some((_, u)) => u.value().coin(),
                None => 0,
            };

            consumed += ada_consumed;
        }

        for txo in tx.produces() {
            produced += txo.1.value().coin();
        }

        // the amount of lovelace in the inputs minus the lovelace in the
        // outputs (the remaining lovelace is used as txfee)
        Ok(consumed - produced)
    }

    /// Calculate the consumed collateral (sum collateral inputs, subtract
    /// collret)
    fn compute_total_collateral(
        &self,
        ctx: &model::BlockContext,
        tx: &MultiEraTx,
    ) -> Result<u64, gasket::error::Error> {
        let mut collateral = 0;

        for txo_ref in tx
            .collateral()
            .iter()
            .map(|i| i.output_ref())
            .unique_by(|x| x.to_string())
        {
            let utxo = ctx
                .find_utxo(&txo_ref)
                .apply_policy(&self.policy)
                .or_panic()?;

            let txo_ada = match utxo {
                Some((_, u)) => u.value().coin(),
                None => 0,
            };

            collateral += txo_ada;
        }

        if let Some(collret) = tx.collateral_return() {
            collateral -= collret.value().coin();
        }

        Ok(collateral)
    }
}

impl Config {
    pub fn plugin(self, policy: &crosscut::policies::RuntimePolicy) -> super::Reducer {
        let reducer = Reducer {
            _config: self,
            policy: policy.clone(),
        };

        super::Reducer::BlockByHeight(reducer)
    }
}
