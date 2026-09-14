/*
    Block by tx hash
*/

use pallas::ledger::traverse::{MultiEraBlock, MultiEraTx};
use serde::Deserialize;

use super::ReducerOutput;

#[derive(Deserialize)]
pub struct Config {}

pub struct Reducer {}

#[derive(Clone, Debug)]
pub struct Output {
    pub tx_hash: [u8; 32],
    pub block_height: u64,
    pub block_index: u16,
}

impl Reducer {
    fn process_tx(
        &mut self,
        tx: &MultiEraTx,
        block_height: u64,
        block_index: u16,
        outputs: &mut Vec<ReducerOutput>,
    ) -> Result<(), gasket::error::Error> {
        outputs.push(ReducerOutput::BlockByTx(Output {
            tx_hash: *tx.hash(),
            block_height,
            block_index,
        }));
        Ok(())
    }

    pub fn reduce_block<'b>(
        &mut self,
        block: &'b MultiEraBlock<'b>,
        outputs: &mut Vec<ReducerOutput>,
    ) -> Result<(), gasket::error::Error> {
        for (idx, tx) in block.txs().iter().enumerate() {
            self.process_tx(tx, block.number(), idx as u16, outputs)?;
        }

        Ok(())
    }
}

impl Config {
    pub fn plugin(self) -> super::Reducer {
        let worker = Reducer {};
        super::Reducer::BlockByTx(worker)
    }
}
