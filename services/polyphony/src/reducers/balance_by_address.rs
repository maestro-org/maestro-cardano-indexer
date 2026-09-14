/*
   (Lovelace) Balance by Address
*/

use itertools::Itertools;
use pallas::ledger::addresses::Address;
use pallas::ledger::traverse::MultiEraOutput;
use pallas::ledger::traverse::{MultiEraBlock, OutputRef};
use serde::Deserialize;

use crate::{crosscut, model, prelude::*};

use super::ReducerOutput;

#[derive(Deserialize)]
pub struct Config {
    pub filter: Option<crosscut::filters::Predicate>,
}

pub struct Reducer {
    config: Config,
    policy: crosscut::policies::RuntimePolicy,
}

#[derive(Clone, Debug)]
pub struct Output {
    pub address: Address,
    pub amount: u64,
    pub increment: bool,
}

impl Reducer {
    fn process_consumed_txo(
        &mut self,
        ctx: &model::BlockContext,
        input: &OutputRef,
        outputs: &mut Vec<ReducerOutput>,
    ) -> Result<(), gasket::error::Error> {
        let utxo = ctx.find_utxo(input).apply_policy(&self.policy).or_panic()?;

        let utxo = match utxo {
            Some((_, u)) => u,
            None => return Ok(()),
        };

        let address = utxo.address().or_panic()?;

        outputs.push(ReducerOutput::BalanceByAddress(Output {
            address,
            amount: utxo.value().coin(),
            increment: false,
        }));
        Ok(())
    }

    fn process_produced_txo(
        &mut self,
        tx_output: &MultiEraOutput,
        outputs: &mut Vec<ReducerOutput>,
    ) -> Result<(), gasket::error::Error> {
        let address = tx_output.address().or_panic()?;

        outputs.push(ReducerOutput::BalanceByAddress(Output {
            address,
            amount: tx_output.value().coin(),
            increment: true,
        }));
        Ok(())
    }

    pub fn reduce_block<'b>(
        &mut self,
        block: &'b MultiEraBlock<'b>,
        ctx: &model::BlockContext,
        outputs: &mut Vec<ReducerOutput>,
    ) -> Result<(), gasket::error::Error> {
        for tx in block.txs().into_iter() {
            if filter_matches!(self, block, &tx, ctx) {
                for consumed in tx
                    .consumes()
                    .iter()
                    .map(|i| i.output_ref())
                    .unique_by(|x| x.to_string())
                {
                    self.process_consumed_txo(ctx, &consumed, outputs)?;
                }

                for (_, produced) in tx.produces() {
                    self.process_produced_txo(&produced, outputs)?;
                }
            }
        }

        Ok(())
    }
}

impl Config {
    pub fn plugin(self, policy: &crosscut::policies::RuntimePolicy) -> super::Reducer {
        let reducer = Reducer {
            config: self,
            policy: policy.clone(),
        };

        super::Reducer::BalanceByAddress(reducer)
    }
}
