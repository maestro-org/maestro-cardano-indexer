use std::collections::{HashMap, HashSet};

use pallas::ledger::addresses::ByronAddress;
use pallas::ledger::primitives::byron::TxOut;
use pallas::ledger::traverse::MultiEraBlock;
use pallas_configs::byron::{self, GenesisFile, GenesisUtxo};
use rocksdb::{Transaction, TransactionDB};
use tracing::info;

use super::{Era, Error};
use crate::storage::kvtable::*;
use crate::storage::{TxoBody, TxoRef};

// txo ref -> (slot, era, txo cbor)
pub struct TxoKV;

impl KVTable<DBSerde<TxoRef>, DBSerde<TxoBody>> for TxoKV {
    const CF_NAME: &'static str = "TxoKV";
}

impl TxoKV {
    pub fn insert_genesis_txos(db: &TransactionDB, byron_config: GenesisFile) -> Result<(), Error> {
        let db_tx = byron::genesis_utxos(&byron_config)
            .into_iter()
            .map(genesis_utxo_to_kv)
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .fold(db.transaction(), |mut db_tx, (k, v)| {
                info!(tx = %k.0 .0, "inserting genesis txo");
                Self::stage_upsert(db, k, v, &mut db_tx).unwrap();
                db_tx
            });

        db_tx.commit().map_err(Error::Rocks)
    }

    pub fn resolve_block_tx_inputs(
        db: &TransactionDB,
        tx: &mut Transaction<TransactionDB>,
        block: MultiEraBlock,
    ) -> Result<HashMap<TxoRef, (TxoBody, bool)>, Error> {
        let txs = block.txs();

        let input_refs = txs
            .iter()
            .map(|x| x.requires())
            .flatten()
            .map(|x| TxoRef(*x.hash(), x.index()))
            .collect::<HashSet<TxoRef>>();

        // get txos which are produced AND consumed in the same block
        let mut produced_and_consumed_in_block = HashMap::new();

        for tx in block.txs() {
            for (idx, produced) in tx.produces() {
                let txo_ref = TxoRef(tx.hash(), idx as u64);

                if input_refs.contains(&txo_ref) {
                    let body = produced.encode();
                    let era: Era = tx.era().into();
                    produced_and_consumed_in_block.insert(txo_ref, (block.slot(), era, body));
                }
            }
        }

        let keys = input_refs.into_iter().map(DBSerde).collect::<Vec<_>>();

        let txo_cf = Self::cf(db);

        let txos = tx
            .multi_get_cf(keys.iter().map(|x| (&txo_cf, Box::<[u8]>::from(x.clone()))))
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .map_err(Error::Rocks)?;

        let kvs = keys.into_iter().map(|x| x.0).zip(txos);

        let mut out = HashMap::new();

        for (txo_ref, txo) in kvs {
            if let Some(b) = txo {
                let DBSerde(txo_body) = <DBSerde<TxoBody>>::from(Box::from(b.as_slice()));
                out.insert(txo_ref, (txo_body, false));
            } else {
                if let Some(body) = produced_and_consumed_in_block.remove(&txo_ref) {
                    out.insert(txo_ref, (body, true));
                } else {
                    return Err(Error::MissingTxo(block.hash(), txo_ref.0, txo_ref.1));
                }
            }
        }

        Ok(out)
    }
}

fn build_byron_txout(addr: ByronAddress, amount: u64) -> TxOut {
    TxOut {
        address: pallas::ledger::primitives::byron::Address {
            payload: addr.payload,
            crc: addr.crc,
        },
        amount,
    }
}

fn genesis_utxo_to_kv(utxo: GenesisUtxo) -> Result<(DBSerde<TxoRef>, DBSerde<TxoBody>), Error> {
    let (tx, addr, amount) = utxo;

    let key = DBSerde(TxoRef(tx, 0));

    let txout = build_byron_txout(addr, amount);
    let txout =
        pallas::codec::minicbor::to_vec(txout).map_err(|e| Error::GenesisTxo(e.to_string()))?;
    let value = DBSerde((0, 0, txout));

    Ok((key, value))
}
