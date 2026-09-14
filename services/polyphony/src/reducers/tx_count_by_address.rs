/*
   Tx Count By Address

   Maps an address to its transaction count, where transaction count is
   defined as the number of transactions where at least one of the following is
   true:
       - a UTxO that was consumed (input or collateral) belonged to the address
       - a UTxO that was produced (output or collateral output) belonged to the
       address
*/

use itertools::Itertools;
use pallas::ledger::addresses::Address;
use pallas::ledger::traverse::{MultiEraBlock, MultiEraOutput, OutputRef};
use serde::Deserialize;
use std::collections::HashSet;

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
}

impl Reducer {
    fn process_inbound_txo(
        &mut self,
        ctx: &model::BlockContext,
        input: &OutputRef,
        seen: &mut HashSet<Vec<u8>>,
        outputs: &mut Vec<ReducerOutput>,
    ) -> Result<(), gasket::error::Error> {
        let utxo = ctx.find_utxo(input).apply_policy(&self.policy).or_panic()?;

        let utxo = match utxo {
            Some((_, u)) => u,
            None => return Ok(()),
        };

        let address = utxo.address().or_panic()?;

        if seen.insert(address.clone().to_vec()) {
            outputs.push(ReducerOutput::TxCountByAddress(Output { address }))
        }

        Ok(())
    }

    fn process_outbound_txo(
        &mut self,
        tx_output: &MultiEraOutput,
        seen: &mut HashSet<Vec<u8>>,
        outputs: &mut Vec<ReducerOutput>,
    ) -> Result<(), gasket::error::Error> {
        let address = tx_output.address().or_panic()?;

        if seen.insert(address.clone().to_vec()) {
            outputs.push(ReducerOutput::TxCountByAddress(Output { address }))
        }

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
                let mut seen = HashSet::new();

                for consumed in tx
                    .consumes()
                    .iter()
                    .map(|i| i.output_ref())
                    .unique_by(|x| x.to_string())
                {
                    self.process_inbound_txo(ctx, &consumed, &mut seen, outputs)?;
                }

                for (_idx, output) in tx.produces() {
                    self.process_outbound_txo(&output, &mut seen, outputs)?;
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

        super::Reducer::TxCountByAddress(reducer)
    }
}
