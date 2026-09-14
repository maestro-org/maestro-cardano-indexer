/*
   Supply By Asset

   Creates reducer outputs to signal a change in the total supply of an asset:
   so for the entries of a transactions mint field.
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
    pub asset_name: Vec<u8>,
    pub delta: i64,
}

impl Reducer {
    fn process_tx(
        &mut self,
        tx: MultiEraTx,
        outputs: &mut Vec<ReducerOutput>,
    ) -> Result<(), gasket::error::Error> {
        for policy in tx.mints() {
            for asset in policy.assets() {
                outputs.push(ReducerOutput::SupplyByAsset(Output {
                    policy: *asset.policy(),
                    asset_name: asset.name().to_vec(),
                    delta: asset.mint_coin().unwrap(),
                }))
            }
        }

        Ok(())
    }

    pub fn reduce_block<'b>(
        &mut self,
        block: &'b MultiEraBlock<'b>,
        outputs: &mut Vec<ReducerOutput>,
    ) -> Result<(), gasket::error::Error> {
        for tx in block.txs().into_iter() {
            if tx.is_valid() {
                self.process_tx(tx, outputs)?;
            }
        }

        Ok(())
    }
}

impl Config {
    pub fn plugin(self) -> super::Reducer {
        let reducer = Reducer;

        super::Reducer::SupplyByAsset(reducer)
    }
}
