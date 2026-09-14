/*
   Txs By Address

   Maps an address to transactions it has been involved in, where involved is
   defined as at least one of the following is true:
       - a UTxO that was consumed (input or collateral) belonged to the address
       - a UTxO that was produced (output or collateral output) belonged to the
       address
*/

use itertools::Itertools;
use pallas::ledger::addresses::Address;
use pallas::ledger::traverse::{MultiEraBlock, MultiEraOutput, OutputRef};
use serde::Deserialize;
use std::collections::HashMap;

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

#[derive(PartialEq)]
enum TxInvolvement {
    Input,
    Output,
}

#[derive(Clone, Debug)]
pub struct Output {
    pub address: Address,
    pub slot: u64,
    pub tx_hash: [u8; 32],
    pub tx_block_index: u16,
    pub input: bool,  // an input belonged to the address
    pub output: bool, // an output belonged to the address
}

impl Reducer {
    fn process_inbound_txo(
        &mut self,
        ctx: &model::BlockContext,
        input: &OutputRef,
        seen: &mut HashMap<Address, Vec<TxInvolvement>>,
    ) -> Result<(), gasket::error::Error> {
        let utxo = ctx.find_utxo(input).apply_policy(&self.policy).or_panic()?;

        let utxo = match utxo {
            Some((_, u)) => u,
            None => return Ok(()),
        };

        let address = utxo.address().or_panic()?;

        if let Some(actions) = seen.get_mut(&address) {
            actions.push(TxInvolvement::Input);
        } else {
            seen.insert(address, vec![TxInvolvement::Input]);
        }

        Ok(())
    }

    fn process_outbound_txo(
        &mut self,
        tx_output: &MultiEraOutput,
        seen: &mut HashMap<Address, Vec<TxInvolvement>>,
    ) -> Result<(), gasket::error::Error> {
        let address = tx_output.address().or_panic()?;

        if let Some(actions) = seen.get_mut(&address) {
            actions.push(TxInvolvement::Output);
        } else {
            seen.insert(address, vec![TxInvolvement::Output]);
        }

        Ok(())
    }

    pub fn reduce_block<'b>(
        &mut self,
        block: &'b MultiEraBlock<'b>,
        ctx: &model::BlockContext,
        outputs: &mut Vec<ReducerOutput>,
    ) -> Result<(), gasket::error::Error> {
        for (idx, tx) in block.txs().into_iter().enumerate() {
            if filter_matches!(self, block, &tx, ctx) {
                let mut seen: HashMap<Address, Vec<TxInvolvement>> = HashMap::new();

                for consumed in tx
                    .consumes()
                    .iter()
                    .map(|i| i.output_ref())
                    .unique_by(|x| x.to_string())
                {
                    self.process_inbound_txo(ctx, &consumed, &mut seen)?;
                }

                for (_idx, output) in tx.produces() {
                    self.process_outbound_txo(&output, &mut seen)?;
                }

                for (address, actions) in seen {
                    let input = actions.contains(&TxInvolvement::Input);
                    let output = actions.contains(&TxInvolvement::Output);

                    outputs.push(ReducerOutput::TxsByAddress(Output {
                        address,
                        slot: block.slot(),
                        tx_hash: *tx.hash(),
                        tx_block_index: idx as u16,
                        input,
                        output,
                    }))
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

        super::Reducer::TxsByAddress(reducer)
    }
}
