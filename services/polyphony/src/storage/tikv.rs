use gasket::{
    error::AsWorkError,
    runtime::{spawn_stage, ScheduleResult, WorkSchedule},
};
use pallas::{ledger::addresses::Address, network::miniprotocols::Point};
use serde::Deserialize;
use std::collections::HashMap;
use std::time::Duration;
use tikv_client::{
    Error, Key, KvPair, Timestamp, Transaction, TransactionClient, TransactionOptions,
};
use tracing::{debug, info, warn};
use tracing::{error, trace};

use super::redis_entry;
use crate::{
    bootstrap, crosscut,
    model::{
        self,
        StorageAction::{self, *},
        StorageActionPayload,
    },
    prelude::AppliesPolicy,
    reducers::{
        balance_by_address, block_by_height, block_by_tx, cip25_metadata_by_asset, datum_by_hash,
        mint_metadata_by_asset, script_by_hash, supply_by_asset, tx_by_hash, tx_count_by_address,
        txs_by_address, txs_by_pay_cred, txs_by_policy, updates_by_policy, utxo_cbor_by_address,
        utxos_by_asset, utxos_by_policy, ReducerOutput, UtxoAction,
    },
    rollback::{
        buffer::{RollbackBuffer, MAX_BUFFER_LEN},
        PersistentBufferValue,
    },
    storage::commit_txn_or_rollback,
};

use timbre::{
    encoding::{
        decode::{decode_cursor_value, decode_rollback_metadata_key},
        encode::*,
    },
    RollbackKey,
};

type InputPort = gasket::messaging::tokio::InputPort<model::StorageActionPayload>;

type StorageActions = Vec<StorageAction>;

#[derive(Deserialize, Clone, Debug)]
pub struct Config {
    /// TiKV PD
    pub connection_params: String,
    /// Polyphony dataplane identifier
    pub dataplane_id: u8,
    /// Polyphony instance identifier so multiple Polyphony can use the same DB
    pub instance_id: u8,
    /// The max number of blocks to batch into a single DB transaction during
    /// sync
    pub sync_batch_size: Option<usize>,
    /// TiKV config value `raft-entry-max-size` in bytes
    pub tikv_raft_entry_max_size: Option<usize>,
    /// Enable paranoid mode to verify storage mutations after commit
    pub paranoid_mode: Option<bool>,
    /// Redis used for cursor entries and the instance registry the API layer
    /// resolves instances from
    pub redis_address: String,
    /// Network name used in cursor entries and registry keys
    /// (`mainnet` / `preprod` / `preview`)
    pub network: String,
    /// Advertise in the instance registry immediately instead of waiting to
    /// reach the mutable window near the chain tip (development setups)
    pub advertise_immediately: Option<bool>,
}

impl Config {
    pub fn bootstrapper(
        self,
        policy: &crosscut::policies::RuntimePolicy,
        instance_names: Vec<String>,
    ) -> Bootstrapper {
        Bootstrapper {
            config: self,
            policy: policy.clone(),
            input: Default::default(),
            instance_names,
        }
    }
}

pub struct Bootstrapper {
    config: Config,
    policy: crosscut::policies::RuntimePolicy,
    input: InputPort,
    instance_names: Vec<String>,
}

impl Bootstrapper {
    pub fn borrow_input_port(&mut self) -> &'_ mut InputPort {
        &mut self.input
    }

    pub fn build_cursor(&self) -> Cursor {
        Cursor {
            config: self.config.clone(),
        }
    }

    pub fn spawn_stages(
        self,
        pipeline: &mut bootstrap::Pipeline,
        intersect: Option<Point>,
        buf: Option<Vec<PersistentBufferValue>>,
    ) {
        let mut rollback_buffer: RollbackBuffer<StorageActions> = Default::default();

        if let Some(persistent_buf) = buf {
            for entry in persistent_buf {
                rollback_buffer.add_block(entry.point, entry.inverse_actions)
            }

            tracing::debug!(
                "populated memory rollback buffer for storage stage: {:?}",
                rollback_buffer
            )
        } else {
            tracing::debug!("no persistent rollback buffer found to populate memory buf")
        }

        let mut tx_opts = TransactionOptions::new_optimistic();

        tx_opts = tx_opts.drop_check(tikv_client::CheckLevel::Warn);

        let worker = Worker {
            config: self.config.clone(),
            policy: self.policy.clone(),
            connection: None,
            input: self.input,
            ops_count: Default::default(),
            key_encoder: KeyEncoder::new(self.config.dataplane_id, self.config.instance_id),
            sync_batcher: SyncBatcher::new(
                self.config.sync_batch_size.unwrap_or(1),
                self.config.tikv_raft_entry_max_size.unwrap_or(4000000),
            ),
            tx_options: tx_opts,
            rollback_buffer,
            work_unit_2pc: None,
            last_processed: intersect,
            paranoid_mode: self.config.paranoid_mode.unwrap_or(false),
            pending_verification: None,
            redis_connection: None,
            instance_names: self.instance_names,
            registered: false,
        };

        info!(
            "using dataplane id {} with instance id {}",
            self.config.dataplane_id, self.config.instance_id
        );

        pipeline.register_stage(spawn_stage(
            worker,
            gasket::runtime::Policy {
                tick_timeout: Some(Duration::from_secs(1200)),
                bootstrap_retry: gasket::retries::Policy {
                    max_retries: 20,
                    backoff_unit: Duration::from_secs(1),
                    backoff_factor: 2,
                    max_backoff: Duration::from_secs(60),
                },
                ..Default::default()
            },
            Some("tikv"),
        ));
    }
}

pub struct Cursor {
    config: Config,
}

impl Cursor {
    pub async fn last_point(&mut self) -> Result<Option<Point>, crate::Error> {
        let connection = TransactionClient::new(vec![self.config.connection_params.clone()])
            .await
            .map_err(crate::Error::storage)?;

        let encoder = KeyEncoder::new(self.config.dataplane_id, self.config.instance_id);

        let mut snap = connection.snapshot(
            connection
                .current_timestamp()
                .await
                .map_err(crate::Error::storage)?,
            TransactionOptions::new_optimistic().drop_check(tikv_client::CheckLevel::Warn),
        );

        let raw = snap
            .get(encoder.encode_cursor_key())
            .await
            .map_err(crate::Error::storage)?;

        let point = raw.map(|x| decode_cursor_value(&x).0);

        if let Some(p) = point.clone() {
            let key = [encoder.dataplane_id(), encoder.instance_id(), b'P'];

            let verified = snap
                .get(key.to_vec())
                .await
                .map_err(crate::Error::storage)?;

            if let Some(verified) = verified {
                let height = u64::from_be_bytes(verified.try_into().unwrap());

                if height != p.slot_or_default() {
                    warn!(
                        "paranoid mode verification KV mismatch {} {point:?}",
                        height
                    )
                } else {
                    debug!("paranoid mode KV match")
                }
            } else {
                warn!("no paranoid mode verification KV")
            };
        }

        Ok(point)
    }

    /// Fetch the persistent buffer from storage, if it exists, to bootstrap the
    /// stage rollback buffers on start-up.
    ///
    /// TODO: Refactor...
    pub async fn fetch_persistent_buffer(
        &mut self,
    ) -> Result<Option<Vec<PersistentBufferValue>>, crate::Error> {
        let connection = TransactionClient::new(vec![self.config.connection_params.clone()])
            .await
            .map_err(crate::Error::storage)?;

        let encoder = KeyEncoder::new(self.config.dataplane_id, self.config.instance_id);

        let mut persistent_buf: Vec<PersistentBufferValue> = Vec::new();

        let mut snap = connection.snapshot(
            connection
                .current_timestamp()
                .await
                .map_err(crate::Error::storage)?,
            TransactionOptions::new_optimistic().drop_check(tikv_client::CheckLevel::Warn),
        );

        // scan all metadata keys, for each point create a vec of inverse
        // storage ops

        let mut range = encoder.encode_rollback_metadata_range(None, None);

        let mut current_point = Point::Origin;
        let mut current_point_slot = 0;
        let mut actions = Vec::new();

        let mut scan_size = 2000;
        let mut kvs = Vec::new();

        loop {
            match snap.scan(range.clone(), scan_size).await {
                Ok(kvs_iter) => {
                    let kvs_vec: Vec<KvPair> = kvs_iter.collect();

                    kvs.extend(kvs_vec.clone());

                    if kvs_vec.is_empty() {
                        break;
                    }

                    let mut last_key =
                        Into::<Vec<u8>>::into(kvs_vec.last().unwrap().clone().into_key());

                    debug!(
                        "scanned {} rb metadata, {} total, last key {}",
                        kvs_vec.len(),
                        kvs.len(),
                        hex::encode(&last_key)
                    );

                    last_key.push(0);
                    range = last_key..range.end;
                }
                Err(Error::Grpc(e))
                    if e.to_string().contains("Received message larger than max") =>
                {
                    scan_size /= 2
                }
                Err(e) => {
                    error!("error when trying to fetch persistent buffer {e:?}");
                    return Err(crate::Error::storage(e));
                }
            }
        }

        for kv in kvs {
            let metadata_key = decode_rollback_metadata_key(kv.key().into());

            debug!("rollback metadata key found: {metadata_key:?}");

            // we are now looking at metadata for a new point, push info for
            // previous point to buffer
            if metadata_key.point().slot_or_default() != current_point_slot {
                // don't include the init point
                if current_point_slot != 0 {
                    let bufval = PersistentBufferValue {
                        point: current_point.clone(),
                        inverse_actions: actions.clone(),
                    };

                    debug!("finished parsing point rb metadata: {bufval:?}");

                    persistent_buf.push(bufval);
                }

                // reset accumulators and set point to next
                current_point = metadata_key.point().clone();
                current_point_slot = current_point.slot_or_default();
                actions.clear();
            }

            match metadata_key {
                // TODO: remove enrich rb keys
                RollbackKey::Enrich(k) => {
                    warn!("found enrich rb key: {:?}", k)
                }
                RollbackKey::Storage(k) => {
                    if kv.value().is_empty() {
                        actions.push(StorageAction::Delete(k.key))
                    } else {
                        actions.push(StorageAction::Set(k.key, kv.into_value()))
                    }
                }
            }
        }

        // we have finished building the buf val for the most recent point as we
        // have exhausted all the rb metadata keys, push the final point. don't push
        // if it is the origin point though.
        if current_point != Point::Origin {
            let bufval = PersistentBufferValue {
                point: current_point.clone(),
                inverse_actions: actions.clone(),
            };

            debug!("finished parsing final point rb metadata: {bufval:?}");

            persistent_buf.push(bufval);
        }

        if persistent_buf.is_empty() {
            info!("no persistent buffer");

            Ok(None)
        } else {
            info!("persistent buffer found ({})", persistent_buf.len());

            Ok(Some(persistent_buf))
        }
    }
}

#[derive(Clone)]
struct SyncBatcher {
    last_point: Option<Point>,
    batch_actions: HashMap<model::Key, StorageAction>,
    max_len: usize,
    current_len: usize,
    max_bytes: usize,
    estimated_bytes: usize, // ~0.6 of resulting raft msg
    merge_counter: usize,
    merge_counter_incr: (usize, usize),
    merge_counter_set: (usize, usize),
}

impl SyncBatcher {
    fn new(max_len: usize, tikv_max_raft_entry_bytes: usize) -> Self {
        SyncBatcher {
            last_point: None,
            batch_actions: HashMap::new(),
            max_len,
            current_len: 0,
            max_bytes: tikv_max_raft_entry_bytes,
            estimated_bytes: 0,
            merge_counter: 0,
            merge_counter_incr: (0, 0),
            merge_counter_set: (0, 0),
        }
    }

    fn push_actions(&mut self, point: Point, actions: StorageActions) {
        for action in actions {
            self.estimated_bytes += action.size();

            match action {
                StorageAction::Set(_, _) | Delete(_) => self.merge_counter_set.1 += 1,
                _ => self.merge_counter_incr.1 += 1,
            }

            if let Some(prev) = self.batch_actions.get_mut(action.key()) {
                self.merge_counter += 1;
                match action {
                    StorageAction::Set(_, _) | Delete(_) => self.merge_counter_set.0 += 1,
                    _ => self.merge_counter_incr.0 += 1,
                }

                prev.merge(action);
            } else {
                self.batch_actions.insert(action.key().clone(), action);
            }
        }

        self.current_len += 1;
        self.last_point = Some(point);
    }

    fn wipe(&mut self) {
        self.last_point = None;
        self.current_len = 0;
        self.batch_actions = HashMap::new();
        self.estimated_bytes = 0;
        self.merge_counter = 0;
        self.merge_counter_incr = (0, 0);
        self.merge_counter_set = (0, 0);
    }

    fn get_batch_actions(&self) -> StorageActions {
        self.batch_actions.values().cloned().collect() // TODO
    }

    /// If the estimated bytes size of the batcher with the addition of the
    /// incoming actions will be greater than half of the TiKV
    /// `max-raft-entry-size` limit then we should flush the batch now, and
    /// start a new batch with the incoming actions.
    fn should_flush(&self, next_actions: &StorageActions) -> bool {
        let next_size: usize = next_actions.iter().map(|x| x.size()).sum();

        if (self.estimated_bytes + next_size) * 2 > self.max_bytes {
            debug!(
                "should flush due to estimated batch bytes: [curr: {}, next: {}, max: {}]",
                self.estimated_bytes, next_size, self.max_bytes
            );

            true
        } else if self.current_len >= self.max_len {
            debug!("should flush due to reaching max blocks per batch");

            true
        } else {
            false
        }
    }

    fn is_empty(&self) -> bool {
        self.batch_actions.is_empty()
    }
}

pub struct Worker {
    config: Config,
    policy: crosscut::policies::RuntimePolicy,
    connection: Option<tikv_client::TransactionClient>,
    ops_count: gasket::metrics::Counter,
    input: InputPort,
    key_encoder: KeyEncoder,
    sync_batcher: SyncBatcher,
    tx_options: TransactionOptions,
    rollback_buffer: RollbackBuffer<StorageActions>,
    work_unit_2pc: Option<StorageActionPayload>,
    last_processed: Option<Point>,
    paranoid_mode: bool,
    pending_verification: Option<Vec<(Vec<u8>, Option<Vec<u8>>)>>,
    redis_connection: Option<redis::Client>,
    /// Instance-registry names of the reducers this instance runs
    instance_names: Vec<String>,
    /// Whether this instance has advertised itself in the instance registry
    registered: bool,
}

// TODO rearrange
impl Worker {
    /// Publish a cursor entry to Redis for a committed point, recording the
    /// TiKV commit timestamp. These entries feed the tikv-gc safepoint
    /// invariants and refresh this instance's registry scores.
    fn insert_timestamp_entry(
        &mut self,
        point: &Point,
        ts: &Timestamp,
    ) -> Result<(), gasket::error::Error> {
        let Point::Specific(slot, hash) = point else {
            return Ok(());
        };

        let block_hash: [u8; 32] = hash
            .clone()
            .try_into()
            .map_err(|_| gasket::error::Error::WorkPanic)?;

        let mut conn = self
            .redis_connection
            .as_ref()
            .unwrap()
            .get_connection()
            .or_restart()?;

        let key = format!(
            "tikv-timestamps:{}:{}",
            self.config.dataplane_id, self.config.instance_id
        );

        // update most recent timestamp for this instance
        let _: redis::Value =
            redis::Commands::zadd(&mut conn, "tikv-timestamps-keys", &key, ts.physical)
                .or_restart()?;

        // add the timestamp entry for this point
        let _: redis::Value = redis::Commands::zadd(
            &mut conn,
            key,
            redis_entry::RedisEntry {
                slot: *slot,
                block_hash,
                commit_ts: ts.clone(),
                network: self.config.network.clone(),
            },
            ts.physical,
        )
        .or_restart()?;

        // refresh this instance's scores in the registry (once registered)
        if self.registered {
            self.update_registry_scores(*slot)?;
        }

        Ok(())
    }

    /// Advertise this instance in the per-reducer instance registry the API
    /// layer resolves instances from. Called once the instance enters the
    /// mutable window near the chain tip (or immediately, when
    /// `advertise_immediately` is set) — a backfilling instance stays
    /// invisible to the API until then.
    fn maybe_advertise(&mut self, mutable: bool, slot: u64) -> Result<(), gasket::error::Error> {
        if self.registered {
            return Ok(());
        }

        let advertise_now = self.config.advertise_immediately.unwrap_or(false);

        if !(mutable || advertise_now) {
            return Ok(());
        }

        info!("instance reached the mutable window, advertising in instance registry");
        self.registered = true;
        self.update_registry_scores(slot)
    }

    fn update_registry_scores(&mut self, slot: u64) -> Result<(), gasket::error::Error> {
        let mut conn = self
            .redis_connection
            .as_ref()
            .unwrap()
            .get_connection()
            .or_restart()?;

        let member = [self.config.dataplane_id, self.config.instance_id];
        let network = self.config.network.to_lowercase();

        for name in &self.instance_names {
            let key = format!("cardano:{}:{}:scores", network, name);
            let _: redis::Value =
                redis::Commands::zadd(&mut conn, key, &member[..], slot).or_restart()?;
        }

        Ok(())
    }

    /// Convert a ReducerOutput to a StorageAction, by specifying what key and
    /// values should be written to, or which keys should be deleted from,
    /// the database. This means the key and value encoding logic for a
    /// ReducerOutput can be different for different storage backends.
    fn reducer_output_to_storage_ops(&self, output: ReducerOutput) -> Vec<StorageAction> {
        match output {
            ReducerOutput::BalanceByAddress(balance_by_address::Output {
                address,
                amount,
                increment,
            }) => {
                let key = self.key_encoder.encode_lovelace_by_address_key(address);

                if increment {
                    vec![StorageAction::IncrementU64(key, amount)]
                } else {
                    vec![StorageAction::DecrementU64(key, amount)]
                }
            }
            ReducerOutput::BlockByHeight(block_by_height::Output {
                hash,
                height,
                header_bytes,
                tx_hashes,
                size,
                total_lovelace_output,
                total_fees,
                total_script_invocations,
                total_ex_units_mem,
                total_ex_units_steps,
            }) => {
                let height_by_hash_key = self.key_encoder.encode_height_by_block_hash_key(&hash);
                let height_by_hash_value = u64::to_be_bytes(height);

                let block_by_height_key = self.key_encoder.encode_block_by_height_key(height);
                let block_by_height_value = encode_block_by_height_value(
                    &hash,
                    header_bytes,
                    tx_hashes,
                    size,
                    total_lovelace_output,
                    total_fees,
                    total_script_invocations,
                    total_ex_units_mem,
                    total_ex_units_steps,
                );

                vec![
                    StorageAction::Set(height_by_hash_key, height_by_hash_value.to_vec()),
                    StorageAction::Set(block_by_height_key, block_by_height_value),
                ]
            }
            ReducerOutput::BlockByTx(block_by_tx::Output {
                tx_hash,
                block_height,
                block_index,
            }) => {
                let key = self.key_encoder.encode_block_by_tx_key(tx_hash);
                let value = encode_block_by_tx_value(block_height, block_index);

                vec![StorageAction::Set(key, value)]
            }
            ReducerOutput::Cip25MetadataByAsset(cip25_metadata_by_asset::Output {
                policy,
                name,
                metadata_721,
            }) => {
                let key = self
                    .key_encoder
                    .encode_cip25_metadata_by_asset_key(&policy, name);

                vec![StorageAction::Set(key, metadata_721)]
            }
            ReducerOutput::DatumByHash(datum_by_hash::Output { hash, bytes }) => {
                let key = self.key_encoder.encode_datum_by_hash_key(&hash);

                // if we have the datum bytes, always write them. if we only have
                // the hash, write an empty value only when the key does not
                // already exist, so we never overwrite resolved bytes with none.
                if let Some(b) = bytes {
                    vec![StorageAction::Set(key, b)]
                } else {
                    vec![StorageAction::Insert(key, vec![])]
                }
            }
            ReducerOutput::MintMetadataByAsset(mint_metadata_by_asset::Output {
                policy,
                name,
                slot,
                tx_block_index,
                tx_hash,
                metadata,
                amount,
            }) => {
                let key = self.key_encoder.encode_mint_metadata_by_asset_key(
                    &policy,
                    name,
                    slot,
                    tx_block_index,
                );

                let value = encode_mint_metadata_by_asset_value(tx_hash, amount, metadata);

                vec![StorageAction::Set(key, value)]
            }
            ReducerOutput::ScriptByHash(script_by_hash::Output {
                hash,
                tx_hash,
                slot,
                kind,
                bytes,
            }) => {
                let key = self.key_encoder.encode_script_by_hash_key(&hash);

                let value = encode_script_by_hash_value(tx_hash, slot, kind, bytes);

                // will only write this key once on first occurrence, so that we
                // record first txhash/slot
                vec![StorageAction::Insert(key, value)]
            }
            ReducerOutput::SupplyByAsset(supply_by_asset::Output {
                policy,
                asset_name,
                delta,
            }) => {
                let key = self
                    .key_encoder
                    .encode_supply_by_asset_key(&policy, asset_name);

                if delta < 0 {
                    // no delete as we want to be able to find assets which existed but now don't
                    vec![StorageAction::DecrementU128NoDelete(key, -delta as u128)]
                } else {
                    vec![StorageAction::IncrementU128(key, delta as u128)]
                }
            }
            ReducerOutput::TxByHash(tx_by_hash::Output { tx_hash, tx_bytes }) => {
                let key = self.key_encoder.encode_tx_by_hash_key(&tx_hash);

                vec![StorageAction::Set(key, tx_bytes)]
            }
            ReducerOutput::TxCountByAddress(tx_count_by_address::Output { address }) => {
                let key = self.key_encoder.encode_tx_count_by_address_key(address);

                vec![StorageAction::IncrementU64(key, 1)]
            }
            ReducerOutput::TxsByAddress(txs_by_address::Output {
                address,
                slot,
                tx_hash,
                tx_block_index,
                input,
                output,
            }) => {
                let key = self.key_encoder.encode_txs_by_address_key(
                    &address,
                    slot,
                    tx_block_index,
                    &tx_hash,
                );

                let value = encode_txs_by_address_value(input, output);

                vec![StorageAction::Set(key, value)]
            }
            ReducerOutput::TxsByPayCred(txs_by_pay_cred::Output {
                payment_cred,
                slot,
                tx_hash,
                tx_block_index,
                input,
                output,
                required_signer,
            }) => {
                let key = self.key_encoder.encode_txs_by_payment_cred_key(
                    &payment_cred,
                    slot,
                    tx_block_index,
                    &tx_hash,
                );

                let value = encode_txs_by_payment_cred_value(input, output, required_signer);

                vec![StorageAction::Set(key, value)]
            }
            ReducerOutput::TxsByPolicy(txs_by_policy::Output {
                policy,
                slot,
                tx_hash,
                tx_block_index,
                assets,
            }) => {
                let key = self
                    .key_encoder
                    .encode_txs_by_policy_key(&policy, slot, tx_block_index);

                let value = encode_txs_by_policy_value(tx_hash, assets);

                vec![StorageAction::Set(key, value)]
            }
            ReducerOutput::UpdatesByPolicy(updates_by_policy::Output {
                policy,
                slot,
                tx_hash,
                tx_block_index,
                assets,
            }) => {
                let key =
                    self.key_encoder
                        .encode_updates_by_policy_key(&policy, slot, tx_block_index);

                let value = encode_updates_by_policy_value(tx_hash, assets);

                vec![StorageAction::Set(key, value)]
            }
            ReducerOutput::UtxoCborByAddress(utxo_cbor_by_address::Output {
                address,
                slot,
                u_hash,
                u_index,
                action,
            }) => {
                // TODO move to timbre?
                let key = match address {
                    Address::Shelley(a) => self
                        .key_encoder
                        .encode_utxos_by_shelley_address_key(a, slot, u_hash, u_index),
                    Address::Byron(a) => self
                        .key_encoder
                        .encode_utxos_by_byron_address_key(a, slot, u_hash, u_index),
                    _ => unreachable!(),
                };

                match action {
                    UtxoAction::Consumed => vec![StorageAction::Delete(key)],
                    UtxoAction::Produced(txo_cbor) => vec![StorageAction::Set(key, txo_cbor)],
                }
            }
            ReducerOutput::UtxosByAsset(utxos_by_asset::Output {
                policy,
                asset_name,
                slot,
                u_hash,
                u_index,
                action,
            }) => {
                let key = self
                    .key_encoder
                    .encode_utxos_by_asset_key(policy, asset_name, slot, u_hash, u_index);

                match action {
                    UtxoAction::Consumed => vec![StorageAction::Delete(key)],
                    UtxoAction::Produced((addr, amount)) => {
                        vec![StorageAction::Set(
                            key,
                            encode_utxos_by_asset_value(&addr, amount),
                        )]
                    }
                }
            }
            ReducerOutput::UtxosByPolicy(utxos_by_policy::Output {
                policy,
                slot,
                u_hash,
                u_index,
                action,
            }) => {
                let key = self
                    .key_encoder
                    .encode_utxos_by_policy_key(policy, slot, u_hash, u_index);

                match action {
                    UtxoAction::Consumed => vec![StorageAction::Delete(key)],
                    UtxoAction::Produced((addr, assets)) => {
                        vec![StorageAction::Set(
                            key,
                            encode_utxos_by_policy_value(&addr, assets),
                        )]
                    }
                }
            }
            ReducerOutput::Cursor((point, height)) => {
                let cursor_key = self.key_encoder.encode_cursor_key();
                let cursor_value = encode_cursor_value(point.clone(), height);

                let current_ts = chrono::Utc::now().timestamp() as u64;
                let info_key = self.key_encoder.encode_info_key();
                let info_value = encode_info_value(point.clone(), height, current_ts);

                match point {
                    Point::Origin => {
                        vec![StorageAction::Set(cursor_key, cursor_value)]
                    }
                    Point::Specific(_slot, _hash) => {
                        vec![
                            StorageAction::Set(cursor_key, cursor_value),
                            StorageAction::Set(info_key, info_value),
                        ]
                    }
                }
            }
        }
    }

    /// Given a transaction and a StorageAction, perform the required operations
    /// against to database to execute the storage action.
    async fn execute_storage_op_in_txn(
        &self,
        txn: &mut Transaction,
        op: StorageAction,
        kv_map: &mut HashMap<Vec<u8>, Vec<u8>>,
        verification_ops: Option<&mut Vec<(Vec<u8>, Option<Vec<u8>>)>>,
    ) -> Result<(), gasket::error::Error> {
        match op {
            Set(k, v) => {
                if let Some(ops) = verification_ops {
                    ops.push((k.clone(), Some(v.clone())));
                }
                txn.put(k, v).await.or_restart()
            }
            Delete(k) => {
                if let Some(ops) = verification_ops {
                    ops.push((k.clone(), None));
                }
                txn.delete(k).await.or_restart()
            }
            Insert(k, v) => match kv_map.remove(&k) {
                Some(_) => Ok(()),
                None => {
                    if let Some(ops) = verification_ops {
                        ops.push((k.clone(), Some(v.clone())));
                    }
                    txn.put(k.clone(), v).await.or_restart()
                }
            },
            IncrementU64(k, d) => {
                if d == 0 {
                    return Ok(());
                };

                match kv_map.remove(&k) {
                    Some(prev_value) => {
                        let prev_amount = u64::from_be_bytes(prev_value[0..8].try_into().unwrap());

                        let new_value = prev_amount + d;
                        let new_bytes = u64::to_be_bytes(new_value);

                        if let Some(ops) = verification_ops {
                            ops.push((k.clone(), Some(new_bytes.to_vec())));
                        }

                        txn.put(k, new_bytes).await.or_restart()
                    }
                    None => {
                        let new_bytes = u64::to_be_bytes(d);

                        if let Some(ops) = verification_ops {
                            ops.push((k.clone(), Some(new_bytes.to_vec())));
                        }

                        txn.put(k, new_bytes).await.or_restart()
                    }
                }
            }
            DecrementU64(k, d) => {
                if d == 0 {
                    return Ok(());
                };

                match kv_map.remove(&k) {
                    Some(prev_value) => {
                        let prev_amount = u64::from_be_bytes(prev_value[0..8].try_into().unwrap());

                        match prev_amount.checked_sub(d) {
                            Some(0) => {
                                if let Some(ops) = verification_ops {
                                    ops.push((k.clone(), None));
                                }
                                txn.delete(k).await.or_restart()
                            },
                            Some(v) => {
                                let new_bytes = u64::to_be_bytes(v);

                                if let Some(ops) = verification_ops {
                                    ops.push((k.clone(), Some(new_bytes.to_vec())));
                                }

                                txn.put(k, new_bytes).await.or_restart()
                            },
                            None => {
                                warn!(
                                    "Trying to decrement u64 key by more than its current value: [{}] {prev_amount} - {d}, deleting key",
                                    hex::encode(k.clone())
                                );

                                if let Some(ops) = verification_ops {
                                    ops.push((k.clone(), None));
                                }

                                txn.delete(k).await.or_restart()
                            }
                        }
                    }
                    None => Ok(tracing::error!(
                        "Trying to decrement u64 key which does not exist: [{}] - {d}, doing nothing",
                        hex::encode(k)
                    )),
                }
            }
            IncrementU128(k, d) => {
                if d == 0 {
                    return Ok(());
                };

                match kv_map.remove(&k) {
                    Some(prev_value) => {
                        let prev_amount =
                            u128::from_be_bytes(prev_value[0..16].try_into().unwrap());

                        let new_value = prev_amount + d;
                        let new_bytes = u128::to_be_bytes(new_value);

                        if let Some(ops) = verification_ops {
                            ops.push((k.clone(), Some(new_bytes.to_vec())));
                        }

                        txn.put(k, new_bytes).await.or_restart()
                    }
                    None => {
                        let new_bytes = u128::to_be_bytes(d);

                        if let Some(ops) = verification_ops {
                            ops.push((k.clone(), Some(new_bytes.to_vec())));
                        }

                        txn.put(k, new_bytes).await.or_restart()
                    }
                }
            }
            DecrementU128NoDelete(k, d) => {
                if d == 0 {
                    return Ok(());
                };

                match kv_map.remove(&k) {
                    Some(prev_value) => {
                        let prev_amount = u128::from_be_bytes(prev_value[0..16].try_into().unwrap());

                        match prev_amount.checked_sub(d) {
                            Some(v) => {
                                let new_bytes = u128::to_be_bytes(v);

                                if let Some(ops) = verification_ops {
                                    ops.push((k.clone(), Some(new_bytes.to_vec())));
                                }

                                txn.put(k, new_bytes).await.or_restart()
                            },
                            None => {
                                warn!(
                                    "Trying to decrement u128 key by more than its current value: [{}] {prev_amount} - {d}, setting to 0",
                                    hex::encode(k.clone())
                                );

                                let zero_bytes = u128::to_be_bytes(0);

                                if let Some(ops) = verification_ops {
                                    ops.push((k.clone(), Some(zero_bytes.to_vec())));
                                }

                                txn.put(k, zero_bytes).await.or_restart()
                            }
                        }
                    }
                    None => Ok(tracing::error!(
                        "Trying to decrement u128 key which does not exist: [{}] - {d}, doing nothing",
                        hex::encode(k)
                    )),
                }
            }
        }
    }

    /// Same as above but does extra work in order to return the StorageAction
    /// which will revert the effects of the action being applied.
    /// TODO double check
    async fn execute_storage_op_in_txn_with_inverse(
        &self,
        txn: &mut Transaction,
        op: StorageAction,
        kv_map: &mut HashMap<Vec<u8>, Vec<u8>>,
        verification_ops: Option<&mut Vec<(Vec<u8>, Option<Vec<u8>>)>>,
    ) -> Result<Option<StorageAction>, gasket::error::Error> {
        match op {
            Set(k, v) => match kv_map.remove(&k) {
                Some(prev_value) => {
                    if *prev_value == v {
                        Ok(None)
                    } else {
                        if let Some(ops) = verification_ops {
                            ops.push((k.clone(), Some(v.clone())));
                        }

                        txn.put(k.clone(), v).await.or_restart()?;

                        Ok(Some(StorageAction::Set(k, prev_value)))
                    }
                }
                None => {
                    if let Some(ops) = verification_ops {
                        ops.push((k.clone(), Some(v.clone())));
                    }

                    txn.put(k.clone(), v).await.or_restart()?;

                    Ok(Some(StorageAction::Delete(k)))
                }
            },
            Delete(k) => match kv_map.remove(&k) {
                Some(prev_value) => {
                    if let Some(ops) = verification_ops {
                        ops.push((k.clone(), None));
                    }

                    txn.delete(k.clone()).await.or_restart()?;

                    Ok(Some(StorageAction::Set(k, prev_value)))
                }
                None => {
                    tracing::warn!("Trying to delete a non-existent key: {}", hex::encode(k));

                    Ok(None)
                }
            },
            Insert(k, v) => match kv_map.remove(&k) {
                Some(_) => Ok(None),
                None => {
                    if let Some(ops) = verification_ops {
                        ops.push((k.clone(), Some(v.clone())));
                    }

                    txn.put(k.clone(), v).await.or_restart()?;

                    Ok(Some(StorageAction::Delete(k)))
                }
            },
            IncrementU64(k, d) => match kv_map.remove(&k) {
                Some(prev_value) => {
                    let prev_amount = u64::from_be_bytes(prev_value[0..8].try_into().unwrap());

                    let new_value = prev_amount + d;
                    let new_bytes = u64::to_be_bytes(new_value);

                    // Track for verification if paranoid mode is enabled
                    if let Some(ops) = verification_ops {
                        ops.push((k.clone(), Some(new_bytes.to_vec())));
                    }

                    txn.put(k.clone(), new_bytes).await.or_restart()?;

                    Ok(Some(StorageAction::Set(k, prev_value)))
                }
                None => {
                    let new_bytes = u64::to_be_bytes(d);

                    // Track for verification if paranoid mode is enabled
                    if let Some(ops) = verification_ops {
                        ops.push((k.clone(), Some(new_bytes.to_vec())));
                    }

                    txn.put(k.clone(), new_bytes).await.or_restart()?;

                    Ok(Some(StorageAction::Delete(k)))
                }
            },
            DecrementU64(k, d) => match kv_map.remove(&k) {
                Some(prev_value) => {
                    let prev_amount = u64::from_be_bytes(prev_value[0..8].try_into().unwrap());

                    match prev_amount.checked_sub(d) {
                        Some(0) => {
                            if let Some(ops) = verification_ops {
                                ops.push((k.clone(), None));
                            }

                            txn.delete(k.clone()).await.or_restart()?;
                        }
                        Some(v) => {
                            let new_bytes = u64::to_be_bytes(v);

                            if let Some(ops) = verification_ops {
                                ops.push((k.clone(), Some(new_bytes.to_vec())));
                            }

                            txn.put(k.clone(), new_bytes).await.or_restart()?;
                        }
                        None => {
                            warn!(
                                "Trying to decrement u64 key by more than its current value: [{}] {prev_amount} - {d}, deleting key",
                                hex::encode(k.clone())
                            );

                            if let Some(ops) = verification_ops {
                                ops.push((k.clone(), None));
                            }

                            txn.delete(k.clone()).await.or_restart()?;
                        }
                    }

                    Ok(Some(StorageAction::Set(k, prev_value)))
                }
                None => {
                    tracing::warn!("Trying to decrement u64 key which does not exist: [{}] - {d}, doing nothing", hex::encode(k));

                    Ok(None)
                }
            },
            IncrementU128(k, d) => match kv_map.remove(&k) {
                Some(prev_value) => {
                    let prev_amount = u128::from_be_bytes(prev_value[0..16].try_into().unwrap());

                    let new_value = prev_amount + d;
                    let new_bytes = u128::to_be_bytes(new_value);

                    if let Some(ops) = verification_ops {
                        ops.push((k.clone(), Some(new_bytes.to_vec())));
                    }

                    txn.put(k.clone(), new_bytes).await.or_restart()?;

                    Ok(Some(StorageAction::Set(k, prev_value)))
                }
                None => {
                    let new_bytes = u128::to_be_bytes(d);

                    if let Some(ops) = verification_ops {
                        ops.push((k.clone(), Some(new_bytes.to_vec())));
                    }

                    txn.put(k.clone(), new_bytes).await.or_restart()?;

                    Ok(Some(StorageAction::Delete(k)))
                }
            },
            DecrementU128NoDelete(k, d) => match kv_map.remove(&k) {
                Some(prev_value) => {
                    let prev_amount = u128::from_be_bytes(prev_value[0..16].try_into().unwrap());

                    match prev_amount.checked_sub(d) {
                        Some(v) => {
                            let new_bytes = u128::to_be_bytes(v);

                            if let Some(ops) = verification_ops {
                                ops.push((k.clone(), Some(new_bytes.to_vec())));
                            }

                            txn.put(k.clone(), new_bytes).await.or_restart()?;
                        }
                        None => {
                            warn!(
                                "Trying to decrement u128 key by more than its current value: [{}] {prev_amount} - {d}, setting to 0",
                                hex::encode(k.clone())
                            );

                            let zero_bytes = u128::to_be_bytes(0);

                            if let Some(ops) = verification_ops {
                                ops.push((k.clone(), Some(zero_bytes.to_vec())));
                            }

                            txn.put(k.clone(), zero_bytes).await.or_restart()?;
                        }
                    }

                    Ok(Some(StorageAction::Set(k, prev_value)))
                }
                None => {
                    tracing::warn!("Trying to decrement u128 key which does not exist: [{}] - {d}, doing nothing", hex::encode(k));

                    Ok(None)
                }
            },
        }
    }

    async fn apply_storage_rb_actions(
        &mut self,
        txn: &mut Transaction,
        inverse_ops: &StorageActions,
        b_slot: u64,
        b_hash: &[u8; 32],
    ) -> Result<(), gasket::error::Error> {
        let mut verification_ops = if self.paranoid_mode {
            Some(Vec::new())
        } else {
            None
        };

        for (idx, action) in inverse_ops.iter().enumerate() {
            let key = self.key_encoder.encode_storage_rollback_metadata_key(
                b_slot,
                b_hash,
                idx.try_into().unwrap(),
                action.key(),
            );

            // Value is NIL if the inverse action deleting the key, otherwise is
            // the overwritten value.
            let value = match action {
                Set(_, v) => v.clone(),
                Delete(_) => vec![],
                d => {
                    tracing::error!("inverse operation was not 'Set' or 'Delete': {d:?}");

                    return Err(gasket::error::Error::WorkPanic);
                }
            };

            debug!(
                "write storage rb [{}] -> [{}]",
                hex::encode(&key),
                hex::encode(&value)
            );

            if let Some(ops) = verification_ops.as_mut() {
                ops.push((key.clone(), Some(value.clone())));
            }

            txn.put(key, value).await.or_restart()?
        }

        if let Some(ops) = verification_ops {
            if let Some(pending) = self.pending_verification.as_mut() {
                pending.extend(ops);
            } else {
                warn!("no pending verification in apply rb")
            }
        }

        Ok(())
    }

    async fn garbage_collect_rollback_metadata(
        &self,
        txn: &mut Transaction,
        b_slot: u64,
    ) -> Result<usize, gasket::error::Error> {
        // garbage collect (delete) old rollback metadata keys
        if b_slot >= (MAX_BUFFER_LEN as u64 * 20) {
            let gc_range = self
                .key_encoder
                .encode_rollback_metadata_range(None, Some(b_slot - (MAX_BUFFER_LEN as u64 * 20)));

            debug!(
                "garbage collecting rb range: [{}] - [{}]",
                hex::encode(&gc_range.start),
                hex::encode(&gc_range.end)
            );

            let mut total = 0;

            loop {
                let old_keys: Vec<Key> = txn
                    .scan_keys(gc_range.clone(), 1000)
                    .await
                    .or_restart()?
                    .collect();

                if old_keys.is_empty() {
                    break;
                }

                for k in old_keys {
                    let k_bytes = Into::<Vec<u8>>::into(k);

                    trace!("gc enrich rb [{}]", hex::encode(&k_bytes));

                    txn.delete(Key::from(k_bytes)).await.or_restart()?;

                    total += 1;
                }
            }

            Ok(total)
        } else {
            Ok(0)
        }
    }

    /// Given a list of storage actions, execute them against the database. If
    /// return_inverses flag is set the function returns the storage actions
    /// which will inverse the given actions (same ordering as they were
    /// applied).
    ///
    /// BATCH GET:
    /// - if the action is INCR/DECR AND/OR rollback handling is enabled, then
    ///   we need to get a key for the action. if we batch get all the keys, and
    ///   then pass these as a map to the execute storage op functions.
    async fn apply_actions(
        &mut self,
        txn: &mut Transaction,
        actions: Vec<StorageAction>,
        return_inverses: bool,
    ) -> Result<Option<Vec<StorageAction>>, gasket::error::Error> {
        let mut kv_map = self
            .batch_get_required_kvs_for_batch(txn, &actions, return_inverses)
            .await?;

        debug!("batch fetched required {} kvs", kv_map.len());

        let mut verification_ops = if self.paranoid_mode {
            Some(Vec::new())
        } else {
            None
        };

        if return_inverses {
            let mut inverses: Vec<StorageAction> = Vec::new();

            for action in actions {
                debug!("executing storage op with inverse: {action:?}");

                if let Some(inverse_op) = self
                    .execute_storage_op_in_txn_with_inverse(
                        txn,
                        action,
                        &mut kv_map,
                        verification_ops.as_mut(),
                    )
                    .await
                    .or_restart()?
                {
                    debug!("storing inverse storage op: {inverse_op:?}");
                    inverses.push(inverse_op)
                }
            }

            if let Some(ops) = verification_ops {
                self.pending_verification = Some(ops);
            }

            Ok(Some(inverses))
        } else {
            for action in actions {
                debug!("executing storage op: {action:?}");

                self.execute_storage_op_in_txn(txn, action, &mut kv_map, verification_ops.as_mut())
                    .await
                    .or_restart()?
            }

            if let Some(ops) = verification_ops {
                self.pending_verification = Some(ops);
            }

            Ok(None)
        }
    }

    /// Remove persistent rollback metadata after a specific slot (because we
    /// have rollbacked to that slot and so that metadata is not valid anymore)
    async fn trim_persistent_rollback_metadata(
        &mut self,
        txn: &mut Transaction,
        after_slot: u64,
    ) -> Result<usize, gasket::error::Error> {
        let mut verification_ops = if self.paranoid_mode {
            Some(Vec::new())
        } else {
            None
        };

        // delete all rollback metadata keys which are for points after a specific slot
        let range = self
            .key_encoder
            .encode_rollback_metadata_range(Some(after_slot + 1), None);

        debug!(
            "trimming rb range: [{}] - [{}]",
            hex::encode(&range.start),
            hex::encode(&range.end)
        );

        let mut total = 0;

        loop {
            let old_keys: Vec<Key> = txn
                .scan_keys(range.clone(), 100)
                .await
                .or_restart()?
                .collect();

            if old_keys.is_empty() {
                break;
            }

            for k in old_keys {
                let k_bytes = Into::<Vec<u8>>::into(k);

                debug!("rewind rb metadata [{}]", hex::encode(&k_bytes));

                if let Some(ops) = verification_ops.as_mut() {
                    ops.push((k_bytes.clone(), None::<Vec<u8>>));
                }

                txn.delete(k_bytes).await.or_restart()?;

                total += 1;
            }
        }

        if let Some(ops) = verification_ops {
            if let Some(pending) = self.pending_verification.as_mut() {
                pending.extend(ops);
            } else {
                warn!("no pending verification in trim")
            }
        }

        Ok(total)
    }

    async fn batch_get_required_kvs_for_batch(
        &mut self,
        txn: &mut Transaction,
        actions: &StorageActions,
        need_inverses: bool,
    ) -> Result<HashMap<Vec<u8>, Vec<u8>>, gasket::error::Error> {
        let required_keys: Vec<Vec<u8>> = if need_inverses {
            // if we need inverses, then we require all keys
            actions.iter().map(|a| a.key().clone()).collect()
        } else {
            // if not, we only need the keys for incr/decrs/inserts
            actions
                .iter()
                .filter(|a| a.requires_previous_value())
                .map(|a| a.key().clone())
                .collect()
        };

        // split the required keys into smaller batches to avoid large response size
        // which can cause an error

        let mut acc = HashMap::new();

        for key_batch in required_keys.chunks(1000) {
            let kvs: HashMap<Vec<u8>, Vec<u8>> = txn
                .batch_get(key_batch.to_vec())
                .await
                .or_restart()?
                .map(|KvPair(k, v)| (k.into(), v))
                .collect();

            acc.extend(kvs)
        }

        Ok(acc)
    }
}

#[async_trait::async_trait(?Send)]
impl gasket::runtime::Worker for Worker {
    type WorkUnit = StorageActionPayload;

    fn metrics(&self) -> gasket::metrics::Registry {
        gasket::metrics::Builder::new()
            .with_counter("storage_ops", &self.ops_count)
            .build()
    }

    async fn bootstrap(&mut self) -> Result<(), gasket::error::Error> {
        info!(
            "bootstrapping storage (dataplane {}, instance {}, network {})",
            self.config.dataplane_id, self.config.instance_id, self.config.network
        );

        self.connection = TransactionClient::new_with_config(
            vec![self.config.connection_params.clone()],
            tikv_client::Config::default(),
        )
        .await
        .or_retry()?
        .into();

        self.redis_connection = Some(
            redis::Client::open(self.config.redis_address.clone())
                .map_err(|_| gasket::error::Error::WorkPanic)?,
        );

        Ok(())
    }

    async fn schedule(&mut self) -> ScheduleResult<Self::WorkUnit> {
        // if we haven't committed the last work unit then reschedule that, else
        // pull the next message from the upstream stage
        match &self.work_unit_2pc {
            Some(w) => {
                info!("found uncommitted 2pc work unit, re-scheduling...");
                Ok(WorkSchedule::Unit(w.clone()))
            }
            None => {
                debug!("scheduling");
                // receive the MsgRollForward/Backwards (and list of storage
                // actions so we can add them to the persistent rollback buffer)
                let msg = self.input.recv().await?;

                Ok(WorkSchedule::Unit(msg.payload))
            }
        }
    }

    // TODO check state changes are sound with possible restarts
    async fn execute(&mut self, unit: &Self::WorkUnit) -> Result<(), gasket::error::Error> {
        self.pending_verification = None;

        match unit {
            StorageActionPayload::RollForward(point, outputs, mutable) => {
                tracing::debug!(
                    "processing roll forwards msg for point {point:?} (batcher actions: {}) (mutable: {mutable})",
                    self.sync_batcher.batch_actions.len()
                );

                // convert the reducer outputs into a list of storage actions
                // to be executed against the db
                let reducer_storage_ops: StorageActions = outputs
                    .clone()
                    .into_iter()
                    .flat_map(|x| self.reducer_output_to_storage_ops(x))
                    .collect();

                // If chain mutable:
                // - if it is the first block we are processing as mutable, we
                // need to add the last point to the rollback buffer in case
                // we see a rollback to that point
                // - check if sync batcher contains any actions, if so flush/apply them
                // without returning inverses
                // - apply block actions and get inverse actions
                // - apply inverse actions stuff and gc

                if !mutable {
                    // chain not mutable: no rollback handling, batched txs

                    if self.sync_batcher.should_flush(&reducer_storage_ops) {
                        // 2pc lock
                        self.work_unit_2pc = Some(unit.clone());

                        // start db transaction
                        let mut txn = self
                            .connection
                            .as_mut()
                            .unwrap()
                            .begin_with_options(self.tx_options.clone()) // TODO
                            .await
                            .or_restart()?;

                        let merge_percentage = if self.sync_batcher.merge_counter == 0
                            && self.sync_batcher.batch_actions.is_empty()
                        {
                            0
                        } else {
                            (self.sync_batcher.merge_counter * 100)
                                / (self.sync_batcher.merge_counter
                                    + self.sync_batcher.batch_actions.len())
                        };

                        debug!(
                            "flushing batch with {} actions, {} merged ({}%, {}/{} set/delete, {}/{} incr/decr) (estimated size {}) ({:?})",
                            self.sync_batcher.batch_actions.len(),
                            self.sync_batcher.merge_counter,
                            merge_percentage,
                            self.sync_batcher.merge_counter_set.0,
                            self.sync_batcher.merge_counter_set.1,
                            self.sync_batcher.merge_counter_incr.0,
                            self.sync_batcher.merge_counter_incr.1,
                            self.sync_batcher.estimated_bytes,
                            txn.start_timestamp()
                        );

                        self.apply_actions(&mut txn, self.sync_batcher.get_batch_actions(), false)
                            .await?;

                        debug!("committing batch");

                        let ts = commit_txn_or_rollback(&mut txn).await?;

                        // try commit now, before mutating sync batcher
                        debug!("finished committing: {:?}", ts,);

                        // 2pc unlock
                        self.work_unit_2pc = None;

                        if let Some(ts) = &ts {
                            self.insert_timestamp_entry(&point, ts)?;
                        }
                        self.maybe_advertise(false, point.slot_or_default())?;

                        // verify mutations in paranoid mode
                        if self.paranoid_mode {
                            if let Some(operations) = self.pending_verification.take() {
                                if let Some(p) = self.sync_batcher.last_point.clone() {
                                    let mut verify = true;

                                    while verify {
                                        // we will retry until we successfully confirm all the
                                        // changes
                                        // (even if there is a tikv error, keep retrying the checks)
                                        verify = self
                                            .verify_mutations(&p, operations.clone())
                                            .await
                                            .unwrap_or(true); // if err, retry

                                        if verify {
                                            tokio::time::sleep(Duration::from_secs(2)).await
                                        }
                                    }
                                }
                            } else {
                                warn!("paranoid mode but no verification operations")
                            }
                        }

                        self.last_processed = Some(point.clone());

                        self.ops_count
                            .inc(self.sync_batcher.batch_actions.len() as u64);

                        self.sync_batcher.wipe();

                        debug!(
                            "initialising new batch with {} actions",
                            reducer_storage_ops.len()
                        );

                        self.sync_batcher
                            .push_actions(point.clone(), reducer_storage_ops);

                        Ok(())
                    } else {
                        self.sync_batcher
                            .push_actions(point.clone(), reducer_storage_ops);

                        Ok(())
                    }
                } else {
                    let first_mutable = self.rollback_buffer.is_empty();

                    // 2pc lock
                    self.work_unit_2pc = Some(unit.clone());

                    let mut total_ops = 0;

                    // start db transaction
                    let mut txn = self
                        .connection
                        .as_mut()
                        .unwrap()
                        .begin_with_options(self.tx_options.clone()) // TODO
                        .await
                        .or_restart()?;

                    // chain is mutable: rollback handling, no batch txs
                    debug!("started tx ({:?})", txn.start_timestamp());

                    // check if any actions remain in batch from sync, if so apply/flush them
                    // TODO not restart successful
                    if !self.sync_batcher.is_empty() {
                        let rem_actions = self.sync_batcher.get_batch_actions();

                        debug!(
                            "applying {} actions remaining in sync batcher...",
                            rem_actions.len()
                        );

                        self.apply_actions(&mut txn, rem_actions, false).await?;
                    }

                    // create a syncbatcher so we can use its merging
                    // functionality for this single block
                    let mut merger = SyncBatcher::new(1, usize::MAX);
                    merger.push_actions(point.clone(), reducer_storage_ops);

                    let actions = merger.get_batch_actions();

                    debug!("applying {} actions", actions.len());

                    let inverse_ops = self.apply_actions(&mut txn, actions, true).await?.unwrap();

                    // initialise the persistent rollback buffer if it is empty
                    // and this is the first mutable block (we do the same for
                    // in-memory buffer after db tx success)
                    if first_mutable {
                        if let Some(Point::Specific(last_slot, last_hash)) = &self.last_processed {
                            self.apply_storage_rb_actions(
                                &mut txn,
                                &vec![StorageAction::Delete(vec![])],
                                *last_slot,
                                &last_hash.clone().try_into().unwrap(),
                            )
                            .await?
                        }
                    }

                    if let Point::Specific(b_slot, b_hash) = point {
                        debug!("applying rb actions ({} storage)", inverse_ops.len());

                        self.apply_storage_rb_actions(
                            &mut txn,
                            &inverse_ops,
                            *b_slot,
                            &b_hash.clone().try_into().unwrap(),
                        )
                        .await?;

                        total_ops += inverse_ops.len();

                        debug!("gcing rb actions");

                        // TODO: Don't do on every block
                        let gc_amt = self
                            .garbage_collect_rollback_metadata(&mut txn, *b_slot)
                            .await?;

                        total_ops += gc_amt;
                    }

                    debug!("committing tx");

                    let ts = commit_txn_or_rollback(&mut txn).await?;

                    // try commit now, before mutating sync batcher
                    debug!("finished committing: {:?}", ts,);

                    // unlock 2pc
                    self.work_unit_2pc = None;

                    if let Some(ts) = &ts {
                        self.insert_timestamp_entry(&point, ts)?;
                    }
                    self.maybe_advertise(true, point.slot_or_default())?;

                    // Verify mutations in paranoid mode
                    if self.paranoid_mode {
                        if let Some(operations) = self.pending_verification.take() {
                            let mut verify = true;

                            while verify {
                                // we will retry until we successfully confirm all the changes
                                // (even if there is a tikv error, keep retrying the checks)
                                verify = self
                                    .verify_mutations(&point, operations.clone())
                                    .await
                                    .unwrap_or(true);

                                if verify {
                                    tokio::time::sleep(Duration::from_secs(2)).await
                                }
                            }
                        } else {
                            warn!("paranoid mode but no verification operations")
                        }
                    }

                    // initialise the in-memory rollback buffer by pushing the
                    // last point, in case we need to rollback to it
                    if first_mutable {
                        let init_point = self.last_processed.clone().unwrap_or(Point::Origin);
                        info!("initialising rollback buffer with {:?}", init_point);
                        self.rollback_buffer.add_block(init_point, vec![])
                    }

                    self.last_processed = Some(point.clone());

                    // push the point and the inverse storage actions onto the
                    // front of the in-memory rollback buffer
                    self.rollback_buffer.add_block(point.clone(), inverse_ops);

                    // clear sync-batcher now we can safely mutate
                    if !self.sync_batcher.is_empty() {
                        self.sync_batcher.wipe();
                    }

                    self.ops_count.inc(total_ops as u64);

                    Ok(())
                }
            }
            StorageActionPayload::RollBack(point) => {
                // 2pc lock
                self.work_unit_2pc = Some(unit.clone());

                tracing::info!("processing roll backwards msg for point {point:?}");

                let mut txn = self
                    .connection
                    .as_mut()
                    .unwrap()
                    .begin_with_options(
                        TransactionOptions::new_optimistic()
                            .drop_check(tikv_client::CheckLevel::Warn),
                    )
                    .await
                    .or_restart()?;

                // fetch the required points and inverse storage actions from
                // the memory rollback buffer
                let points_and_results = self
                    .rollback_buffer
                    .points_since(point)
                    .map_err(crate::Error::rollback)
                    .apply_policy(&self.policy)
                    .or_panic()?;

                // apply the error policy for unhandleable rollbacks
                let points_and_results = match points_and_results {
                    Some(x) => x,
                    None => panic!("could not handle rollback"),
                };

                debug!(
                    "found {} points in rb buf after rb point",
                    points_and_results.len()
                );

                // Create a list of inverse actions, each of which reverses a storage action
                // that occurred as a result of a block which has been rollbacked. The first
                // inverse action in the vec corresponds to the most recently applied action,
                // that is the last action performed by the most recent block.
                let inverse_actions: Vec<StorageAction> = points_and_results
                    .into_iter()
                    .flat_map(|point| point.result.into_iter().rev())
                    .collect();

                // apply all the inverse actions
                self.apply_actions(&mut txn, inverse_actions, false).await?;

                // remove the rollbacked points from the persistent rollback buffer
                self.trim_persistent_rollback_metadata(&mut txn, point.slot_or_default())
                    .await?;

                debug!("committing batch");

                let ts = commit_txn_or_rollback(&mut txn).await?;

                // try commit now, before mutating sync batcher
                debug!("finished committing: {:?}", ts,);

                // 2pc unlock
                self.work_unit_2pc = None;

                if let Some(ts) = &ts {
                    self.insert_timestamp_entry(&point, ts)?;
                }

                // verify mutations in paranoid mode
                if self.paranoid_mode {
                    if let Some(operations) = self.pending_verification.take() {
                        let mut verify = true;

                        while verify {
                            // we will retry until we successfully confirm all the changes
                            // (even if there is a tikv error, keep retrying the checks)
                            verify = self
                                .verify_mutations(&point, operations.clone())
                                .await
                                .unwrap_or(true);

                            if verify {
                                tokio::time::sleep(Duration::from_secs(2)).await
                            }
                        }
                    } else {
                        warn!("paranoid mode but no verification operations")
                    }
                }

                self.last_processed = Some(point.clone());

                // Now we have successfully sent a transaction to storage using data from the
                // memory rollback buffer, we can trim those points from the
                // buffer.
                self.rollback_buffer
                    .rollback_to_point(point)
                    .map_err(crate::Error::rollback)
                    .apply_policy(&self.policy)
                    .or_panic()?;

                // TODO
                // count the number of storage actions which were performed
                // self.ops_count.inc(inverse_actions.len() as u64);

                Ok(())
            }
        }
    }

    async fn teardown(&mut self) -> Result<(), gasket::error::Error> {
        Ok(())
    }
}

impl Worker {
    /// Verifies that the mutations performed in a transaction were successful
    async fn verify_mutations(
        &mut self,
        point: &Point,
        operations: Vec<(Vec<u8>, Option<Vec<u8>>)>,
    ) -> Result<bool, tikv_client::Error> {
        debug!("verifying {} storage operations", operations.len());

        let timestamp = self
            .connection
            .as_ref()
            .unwrap()
            .current_timestamp()
            .await?;

        let mut snap = self.connection.as_ref().unwrap().snapshot(
            timestamp,
            TransactionOptions::new_optimistic().drop_check(tikv_client::CheckLevel::Warn),
        );

        let mut failures = Vec::new();

        // Extract all keys that need verification
        let keys: Vec<Vec<u8>> = operations.iter().map(|(k, _)| k.clone()).collect();

        // Create a map of expected values for quick lookup
        let expected_map: HashMap<Vec<u8>, Option<Vec<u8>>> = operations.into_iter().collect();

        // Batch get values in chunks to avoid large responses
        for key_batch in keys.chunks(1000) {
            let kvs: HashMap<Vec<u8>, Vec<u8>> = snap
                .batch_get(key_batch.to_vec())
                .await?
                .map(|KvPair(k, v)| (k.into(), v))
                .collect();

            // Check each key in this batch
            for key in key_batch {
                let expected_value = expected_map.get(key).unwrap();
                let actual_value = kvs.get(key);

                match (expected_value, actual_value) {
                    (&Some(ref expected), Some(actual)) => {
                        if expected != actual {
                            error!(
                                "Key [{}] has unexpected value. Expected: [{}], Actual: [{}]",
                                hex::encode(key),
                                hex::encode(expected),
                                hex::encode(actual)
                            );
                            failures.push((key.clone(), Some(expected.clone())));
                        }
                    }
                    (&None, Some(actual)) => {
                        error!(
                            "Key [{}] should be deleted but has value: [{}]",
                            hex::encode(key),
                            hex::encode(actual)
                        );
                        failures.push((key.clone(), None));
                    }
                    (&Some(ref expected), None) => {
                        error!(
                            "Key [{}] should have value [{}] but doesn't exist",
                            hex::encode(key),
                            hex::encode(expected)
                        );
                        failures.push((key.clone(), Some(expected.clone())))
                    }
                    (&None, None) => {}
                }
            }
        }

        // retry the failures
        if !failures.is_empty() {
            error!(
                "paranoid mode verification failed with {} errors detected",
                failures.len()
            );

            // start db transaction
            let mut txn = self
                .connection
                .as_mut()
                .unwrap()
                .begin_with_options(self.tx_options.clone()) // TODO
                .await?;

            for (key, value) in failures.iter() {
                if let Some(v) = value {
                    txn.put(key.clone(), v.clone()).await?;
                } else {
                    txn.delete(key.clone()).await?;
                }
            }

            txn.commit().await?;

            Ok(true)
        } else {
            debug!(
                "verified {} actions for point {point:?}",
                expected_map.len()
            );

            // start db transaction
            let mut txn = self
                .connection
                .as_mut()
                .unwrap()
                .begin_with_options(self.tx_options.clone())
                .await?;

            let key = [
                self.key_encoder.dataplane_id(),
                self.key_encoder.instance_id(),
                b'P',
            ];

            txn.put(key.to_vec(), point.slot_or_default().to_be_bytes())
                .await?;

            txn.commit().await?;

            Ok(false)
        }
    }
}
