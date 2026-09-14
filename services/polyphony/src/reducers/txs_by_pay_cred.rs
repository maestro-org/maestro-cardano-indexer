/*
   Txs By Payment Credential

   Maps a payment credential to transactions it has been involved in, where
   involved is defined as at least one of the following is true:
       - a UTxO that was consumed (input or collateral) belonged to an address
         whose payment credential is the one in question
       - a UTxO that was produced (output or collateral output) belonged to an
         address whose payment credential is the one in question
*/

use itertools::Itertools;
use pallas::ledger::addresses::{Address, ShelleyPaymentPart};
use pallas::ledger::traverse::{MultiEraBlock, MultiEraOutput, MultiEraSigners, OutputRef};
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
    RequiredSigner,
}

#[derive(Clone, Debug)]
pub struct Output {
    pub payment_cred: ShelleyPaymentPart,
    pub slot: u64,
    pub tx_hash: [u8; 32],
    pub tx_block_index: u16,
    pub input: bool,           // an input belonged to the payment cred
    pub output: bool,          // an output belonged to the payment cred
    pub required_signer: bool, // the cred was listed as a required signer
}

impl Reducer {
    fn process_inbound_txo(
        &mut self,
        ctx: &model::BlockContext,
        input: &OutputRef,
        seen: &mut HashMap<ShelleyPaymentPart, Vec<TxInvolvement>>,
    ) -> Result<(), gasket::error::Error> {
        let utxo = ctx.find_utxo(input).apply_policy(&self.policy).or_panic()?;

        let utxo = match utxo {
            Some((_, u)) => u,
            None => return Ok(()),
        };

        match utxo.address().or_panic()? {
            Address::Shelley(s) => {
                let payment_cred = s.payment();

                if let Some(actions) = seen.get_mut(payment_cred) {
                    actions.push(TxInvolvement::Input);
                } else {
                    seen.insert(payment_cred.clone(), vec![TxInvolvement::Input]);
                }

                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn process_outbound_txo(
        &mut self,
        tx_output: &MultiEraOutput,
        seen: &mut HashMap<ShelleyPaymentPart, Vec<TxInvolvement>>,
    ) -> Result<(), gasket::error::Error> {
        match tx_output.address().or_panic()? {
            Address::Shelley(s) => {
                let payment_cred = s.payment();

                if let Some(actions) = seen.get_mut(payment_cred) {
                    actions.push(TxInvolvement::Output);
                } else {
                    seen.insert(payment_cred.clone(), vec![TxInvolvement::Output]);
                }

                Ok(())
            }
            _ => Ok(()),
        }
    }

    pub fn reduce_block<'b>(
        &mut self,
        block: &'b MultiEraBlock<'b>,
        ctx: &model::BlockContext,
        outputs: &mut Vec<ReducerOutput>,
    ) -> Result<(), gasket::error::Error> {
        for (idx, tx) in block.txs().into_iter().enumerate() {
            if filter_matches!(self, block, &tx, ctx) {
                let mut seen: HashMap<ShelleyPaymentPart, Vec<TxInvolvement>> = HashMap::new();

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

                if let MultiEraSigners::AlonzoCompatible(req_signers) = tx.required_signers() {
                    for vkh in req_signers {
                        let cred = ShelleyPaymentPart::Key(*vkh);

                        if let Some(actions) = seen.get_mut(&cred) {
                            actions.push(TxInvolvement::RequiredSigner);
                        } else {
                            seen.insert(cred.clone(), vec![TxInvolvement::RequiredSigner]);
                        }
                    }
                }

                for (payment_cred, actions) in seen {
                    let input = actions.contains(&TxInvolvement::Input);
                    let output = actions.contains(&TxInvolvement::Output);
                    let required_signer = actions.contains(&TxInvolvement::RequiredSigner);

                    outputs.push(ReducerOutput::TxsByPayCred(Output {
                        payment_cred,
                        slot: block.slot(),
                        tx_hash: *tx.hash(),
                        tx_block_index: idx as u16,
                        input,
                        output,
                        required_signer,
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

        super::Reducer::TxsByPayCred(reducer)
    }
}
