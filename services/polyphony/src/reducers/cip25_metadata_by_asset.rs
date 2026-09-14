/*
    Latest CIP25 Metadata By Asset

    Creates an output whenever CIP25 metadata (new or updated) is detected for an asset. Specifically, an asset has a positive amount in the mint field, the transaction is valid, the metadata has a "721" key with the expected fields (policy -> name) for the asset with a positive mint amount.
*/

use pallas::ledger::primitives::babbage::PolicyId;
use pallas::ledger::primitives::Fragment;
use pallas::ledger::traverse::{MultiEraBlock, MultiEraTx};
use serde::Deserialize;

use super::ReducerOutput;

#[derive(Deserialize)]
pub struct Config {}

pub struct Reducer;

#[derive(Clone, Debug)]
pub struct Output {
    pub policy: PolicyId,
    pub name: Vec<u8>,
    pub metadata_721: Vec<u8>,
}

impl Reducer {
    fn process_tx(
        &mut self,
        tx: &MultiEraTx,
        outputs: &mut Vec<ReducerOutput>,
    ) -> Result<(), gasket::error::Error> {
        let metadata_721 = if let Some(md) = tx.metadata().find(721) {
            md.encode_fragment().unwrap()
        } else {
            return Ok(());
        };

        for policy in tx.mints() {
            for asset in policy.assets() {
                if asset.mint_coin().unwrap() > 0 {
                    outputs.push(ReducerOutput::Cip25MetadataByAsset(Output {
                        policy: *policy.policy(),
                        name: asset.name().to_vec(),
                        metadata_721: metadata_721.clone(),
                    }))
                }
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
                self.process_tx(&tx, outputs)?;
            }
        }

        Ok(())
    }
}

impl Config {
    pub fn plugin(self) -> super::Reducer {
        let reducer = Reducer;

        super::Reducer::Cip25MetadataByAsset(reducer)
    }
}
