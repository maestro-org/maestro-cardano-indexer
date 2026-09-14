/*
   Datum by Hash

   Creates a reducer output for every datum hash seen, and the corresponding
   data if it is immediately available - note that it is possible to have seen a
   datum hash on-chain for which the datum bytes have not been seen.

   It scans the TxDats section of the TxWits, the TxOuts and CollRet.
*/

use pallas::ledger::primitives::babbage::DatumOption;
use pallas::ledger::traverse::{MultiEraBlock, MultiEraTx, OriginalHash};
use serde::Deserialize;

use crate::prelude::*;
use crate::{crosscut, model};

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
    pub hash: [u8; 32],
    pub bytes: Option<Vec<u8>>,
}

impl Reducer {
    fn process_tx(
        &self,
        tx: &MultiEraTx,
        outputs: &mut Vec<ReducerOutput>,
    ) -> Result<(), gasket::error::Error> {
        for txo in tx.outputs() {
            match txo.datum() {
                Some(DatumOption::Data(d)) => outputs.push(ReducerOutput::DatumByHash(Output {
                    hash: *d.0.original_hash(),
                    bytes: Some(d.0.raw_cbor().into()),
                })),
                Some(DatumOption::Hash(h)) => outputs.push(ReducerOutput::DatumByHash(Output {
                    hash: *h,
                    bytes: None,
                })),
                _ => (),
            }
        }

        if let Some(collret) = tx.collateral_return() {
            match collret.datum() {
                Some(DatumOption::Data(d)) => outputs.push(ReducerOutput::DatumByHash(Output {
                    hash: *d.0.original_hash(),
                    bytes: Some(d.0.raw_cbor().into()),
                })),
                Some(DatumOption::Hash(h)) => outputs.push(ReducerOutput::DatumByHash(Output {
                    hash: *h,
                    bytes: None,
                })),
                _ => (),
            }
        }

        for d in tx.plutus_data() {
            outputs.push(ReducerOutput::DatumByHash(Output {
                hash: *d.original_hash(),
                bytes: Some(d.raw_cbor().into()),
            }))
        }

        Ok(())
    }

    pub fn reduce_block<'b>(
        &mut self,
        block: &'b MultiEraBlock<'b>,
        ctx: &model::BlockContext,
        outputs: &mut Vec<ReducerOutput>,
    ) -> Result<(), gasket::error::Error> {
        for tx in &block.txs() {
            if filter_matches!(self, block, &tx, ctx) {
                self.process_tx(tx, outputs)?;
            }
        }

        Ok(())
    }
}

impl Config {
    pub fn plugin(self, policy: &crosscut::policies::RuntimePolicy) -> super::Reducer {
        let worker = Reducer {
            config: self,
            policy: policy.clone(),
        };
        super::Reducer::DatumByHash(worker)
    }
}
