/*
    Script by Hash

    Creates a reducer output for every script seen, along with the tx hash and
    block slot which allows us to know when the script was first seen.

    It scans outputs for inline scripts, and then the scripts in the transaction
    witnesses.
*/

use pallas::ledger::primitives::conway::ScriptRef;
use pallas::ledger::primitives::Fragment;
use pallas::ledger::traverse::{ComputeHash, MultiEraBlock, MultiEraTx, OriginalHash};
use serde::Deserialize;

use super::ReducerOutput;

#[derive(Deserialize)]
pub struct Config {}

pub struct Reducer;

#[derive(Clone, Debug)]
pub struct Output {
    pub hash: [u8; 28],
    pub tx_hash: [u8; 32],
    pub slot: u64,
    pub kind: u8,
    pub bytes: Vec<u8>,
}

impl Reducer {
    fn process_tx(
        &self,
        tx: &MultiEraTx,
        slot: u64,
        outputs: &mut Vec<ReducerOutput>,
    ) -> Result<(), gasket::error::Error> {
        for txo in tx.outputs().into_iter().chain(tx.collateral_return()) {
            if let Some(script) = txo.script_ref() {
                let (kind, hash, bytes) = match script {
                    ScriptRef::NativeScript(s) => (0, *s.original_hash(), s.raw_cbor().into()),
                    ScriptRef::PlutusV1Script(s) => {
                        (1, *s.compute_hash(), s.encode_fragment().unwrap())
                    }
                    ScriptRef::PlutusV2Script(s) => {
                        (2, *s.compute_hash(), s.encode_fragment().unwrap())
                    }
                    ScriptRef::PlutusV3Script(s) => {
                        (3, *s.compute_hash(), s.encode_fragment().unwrap())
                    }
                };

                outputs.push(ReducerOutput::ScriptByHash(Output {
                    hash,
                    tx_hash: *tx.hash(),
                    slot,
                    kind,
                    bytes,
                }))
            }
        }

        for d in tx.native_scripts() {
            outputs.push(ReducerOutput::ScriptByHash(Output {
                hash: *d.original_hash(),
                tx_hash: *tx.hash(),
                slot,
                kind: 0,
                bytes: d.raw_cbor().into(),
            }))
        }

        for d in tx.plutus_v1_scripts() {
            outputs.push(ReducerOutput::ScriptByHash(Output {
                hash: *d.compute_hash(),
                tx_hash: *tx.hash(),
                slot,
                kind: 1,
                bytes: d.encode_fragment().unwrap(),
            }))
        }

        for d in tx.plutus_v2_scripts() {
            outputs.push(ReducerOutput::ScriptByHash(Output {
                hash: *d.compute_hash(),
                tx_hash: *tx.hash(),
                slot,
                kind: 2,
                bytes: d.encode_fragment().unwrap(),
            }))
        }

        for d in tx.plutus_v3_scripts() {
            outputs.push(ReducerOutput::ScriptByHash(Output {
                hash: *d.compute_hash(),
                tx_hash: *tx.hash(),
                slot,
                kind: 3,
                bytes: d.encode_fragment().unwrap(),
            }))
        }

        Ok(())
    }

    pub fn reduce_block<'b>(
        &mut self,
        block: &'b MultiEraBlock<'b>,
        outputs: &mut Vec<ReducerOutput>,
    ) -> Result<(), gasket::error::Error> {
        for tx in &block.txs() {
            self.process_tx(tx, block.slot(), outputs)?;
        }

        Ok(())
    }
}

impl Config {
    pub fn plugin(self) -> super::Reducer {
        let worker = Reducer;

        super::Reducer::ScriptByHash(worker)
    }
}
