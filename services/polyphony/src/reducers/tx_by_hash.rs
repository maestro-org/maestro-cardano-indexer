/*
   Tx (CBOR) By Tx Hash

   Maps a transaction hash to the corresponding transaction bytes. The full
   transaction bytes are recreated by Pallas by combining the different parts
   of the transaction found in the block bytes.
*/

use pallas::ledger::traverse::{MultiEraBlock, MultiEraTx};
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
    pub tx_hash: [u8; 32],
    pub tx_bytes: Vec<u8>,
}

impl Reducer {
    fn send(
        &mut self,
        tx: &MultiEraTx,
        outputs: &mut Vec<ReducerOutput>,
    ) -> Result<(), gasket::error::Error> {
        outputs.push(ReducerOutput::TxByHash(Output {
            tx_hash: *tx.hash(),
            tx_bytes: tx.encode(),
        }));
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
                self.send(tx, outputs)?;
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
        super::Reducer::TxByHash(worker)
    }
}
