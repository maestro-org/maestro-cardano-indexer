/*
    Mint Metadata By Asset

    Creates an output per asset with a positive amount in the mint field if the
    transaction contains metadata, so that the asset can be linked to the most
    recent metadata, which is useful for CIP25 for example.
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
    pub slot: u64,
    pub tx_block_index: u16,
    pub tx_hash: [u8; 32],
    pub amount: i64,
    pub metadata: Vec<u8>,
}

impl Reducer {
    fn process_tx(
        &mut self,
        tx: &MultiEraTx,
        slot: u64,
        tx_block_index: u16,
        outputs: &mut Vec<ReducerOutput>,
    ) -> Result<(), gasket::error::Error> {
        let metadata = if let Some(md) = tx.metadata().as_alonzo() {
            md.encode_fragment().unwrap()
        } else {
            return Ok(());
        };

        for policy in tx.mints() {
            for asset in policy.assets() {
                if asset.mint_coin().unwrap() != 0 {
                    outputs.push(ReducerOutput::MintMetadataByAsset(Output {
                        policy: *policy.policy(),
                        name: asset.name().to_vec(),
                        slot,
                        tx_hash: *tx.hash(),
                        tx_block_index,
                        amount: asset.mint_coin().unwrap(),
                        metadata: metadata.clone(),
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

        super::Reducer::MintMetadataByAsset(reducer)
    }
}
