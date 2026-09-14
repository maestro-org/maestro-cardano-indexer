/*
    Updates By Policy

    For every transaction, create outputs for every policy of native asset that
    was minted or burned, along with the amounts of each asset of that policy
    that was minted or burned.
*/

use pallas::ledger::primitives::babbage::PolicyId;
use pallas::ledger::traverse::{MultiEraBlock, MultiEraTx};
use serde::Deserialize;

use super::ReducerOutput;

#[derive(Deserialize)]
pub struct Config {}

pub struct Reducer;

#[derive(Clone, Debug)]
pub struct Output {
    pub policy: PolicyId,
    pub slot: u64,
    pub tx_hash: [u8; 32],
    pub tx_block_index: u16,
    pub assets: Vec<(Vec<u8>, i64)>,
}

impl Reducer {
    fn process_tx(
        &mut self,
        tx: &MultiEraTx,
        slot: u64,
        tx_block_index: u16,
        outputs: &mut Vec<ReducerOutput>,
    ) -> Result<(), gasket::error::Error> {
        for policy in tx.mints() {
            let assets = policy
                .assets()
                .iter()
                .map(|a| (a.name().to_vec(), a.mint_coin().unwrap()))
                .collect();

            outputs.push(ReducerOutput::UpdatesByPolicy(Output {
                policy: *policy.policy(),
                slot,
                tx_hash: *tx.hash(),
                tx_block_index,
                assets,
            }))
        }

        Ok(())
    }

    pub fn reduce_block<'b>(
        &mut self,
        block: &'b MultiEraBlock<'b>,
        outputs: &mut Vec<ReducerOutput>,
    ) -> Result<(), gasket::error::Error> {
        for (idx, tx) in block.txs().into_iter().enumerate() {
            if tx.is_valid() {
                self.process_tx(&tx, block.slot(), idx as u16, outputs)?;
            }
        }

        Ok(())
    }
}

impl Config {
    pub fn plugin(self) -> super::Reducer {
        let reducer = Reducer;

        super::Reducer::UpdatesByPolicy(reducer)
    }
}
