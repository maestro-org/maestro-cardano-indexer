/*
   UTxO (CBOR) by Address

   Creates reducer outputs to signal:
       - a UTxO belonging to the address was consumed
       - a UTxO which belong to the address was produced, along with
       the CBOR encoding of the transaction output
*/

use itertools::Itertools;
use pallas::ledger::addresses::Address;
use pallas::ledger::traverse::{MultiEraBlock, MultiEraOutput, MultiEraTx, OutputRef};
use serde::Deserialize;

use crate::{crosscut, model, prelude::*};

use super::{ReducerOutput, UtxoAction};

#[derive(Deserialize)]
pub struct Config {
    pub filter: Option<Vec<String>>,
}

pub struct Reducer {
    config: Config,
    policy: crosscut::policies::RuntimePolicy,
}

#[derive(Clone, Debug)]
pub struct Output {
    pub address: Address,
    pub slot: u64,
    pub u_hash: [u8; 32],
    pub u_index: u64,
    pub action: UtxoAction<Vec<u8>>,
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

        let address = utxo.address().or_panic()?;

        if let Some(addresses) = &self.config.filter {
            if !addresses.contains(&address.to_string()) {
                return Ok(());
            }
        }

        outputs.push(ReducerOutput::UtxoCborByAddress(Output {
            address,
            slot,
            u_hash: **input.hash(),
            u_index: input.index(),
            action: UtxoAction::Consumed,
        }));
        Ok(())
    }

    /// A new UTxO was created at an address. Push the new UTxO to the right of
    /// the address' UTxO list.
    fn process_produced_txo(
        &mut self,
        tx: &MultiEraTx,
        tx_output: &MultiEraOutput,
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

        if let Some(addresses) = &self.config.filter {
            if !addresses.contains(&address.to_string()) {
                return Ok(());
            }
        }

        let txo_cbor = tx_output.encode();

        outputs.push(ReducerOutput::UtxoCborByAddress(Output {
            address,
            slot,
            u_hash: *tx.hash(),
            u_index: output_idx as u64,
            action: UtxoAction::Produced(txo_cbor),
        }));
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
                .filter(|x| !chained_txo_refs.contains(x)) // skip txos produced in this block
                .unique_by(|x| x.to_string())
            {
                self.process_consumed_txo(ctx, &consumed, outputs)?;
            }

            for (idx, produced) in tx.produces() {
                self.process_produced_txo(
                    &tx,
                    &produced,
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
            config: self,
            policy: policy.clone(),
        };

        super::Reducer::UtxoCborByAddress(reducer)
    }
}
