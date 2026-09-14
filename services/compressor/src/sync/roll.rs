use gasket::framework::*;
use tracing::{debug, info};

use crate::storage;

use super::model::PullEvent;

pub type UpstreamPort = gasket::messaging::tokio::InputPort<PullEvent>;
pub type DownstreamPort = gasket::messaging::tokio::OutputPort<PullEvent>;

#[derive(Stage)]
#[stage(name = "roll", unit = "PullEvent", worker = "Worker")]
pub struct Stage {
    chain_db: storage::ChainDB,

    pub upstream: UpstreamPort,
    pub health_downstream: DownstreamPort,

    pub mutable: bool,
    // TODO
    // #[metric]
    // block_count: gasket::metrics::Counter,
}

impl Stage {
    pub fn new(chain_db: storage::ChainDB) -> Self {
        Self {
            chain_db,
            upstream: Default::default(),
            health_downstream: Default::default(),
            mutable: false,
            // block_count: Default::default(),
        }
    }
}

pub struct Worker {}

impl Worker {}

#[async_trait::async_trait(?Send)]
impl gasket::framework::Worker<Stage> for Worker {
    async fn bootstrap(_stage: &Stage) -> Result<Self, WorkerError> {
        Ok(Self {})
    }

    async fn schedule(
        &mut self,
        stage: &mut Stage,
    ) -> Result<WorkSchedule<PullEvent>, WorkerError> {
        let msg = stage.upstream.recv().await.or_panic()?;

        Ok(WorkSchedule::Unit(msg.payload))
    }

    async fn execute(&mut self, unit: &PullEvent, stage: &mut Stage) -> Result<(), WorkerError> {
        match unit {
            PullEvent::RollForward(slot, hash, body, tip) => {
                if !stage.mutable {
                    if let Some(imm_after_slots) = stage.chain_db.immutable_after_slots {
                        if *slot + imm_after_slots > tip.0.slot_or_default() {
                            info!(
                                "switching to mutable mode ({} + {} > {:?})",
                                slot, imm_after_slots, tip.0
                            );
                            stage.mutable = true;
                        }
                    }
                }

                stage
                    .chain_db
                    .apply_block_with_context(body.clone(), stage.mutable)
                    .or_retry()?;

                debug!(slot, %hash, ?tip, "chaindb rollforwarded to point");
            }
            PullEvent::RollBack(rb_slot, rb_hash, tip) => {
                if !stage.mutable {
                    if let Some(imm_after_slots) = stage.chain_db.immutable_after_slots {
                        if *rb_slot + imm_after_slots > tip.0.slot_or_default() {
                            info!(
                                "switching to mutable mode ({} + {} > {:?})",
                                rb_slot, imm_after_slots, tip.0
                            );
                            stage.mutable = true;
                        }
                    }
                }

                stage
                    .chain_db
                    .rollback(*rb_slot, *rb_hash, stage.mutable)
                    .or_retry()?;

                debug!(rb_slot, %rb_hash, ?tip, "chaindb rollbacked to point");
            }
            PullEvent::RollBackOrigin(tip) => {
                if !stage.mutable {
                    if let Some(imm_after_slots) = stage.chain_db.immutable_after_slots {
                        if imm_after_slots > tip.0.slot_or_default() {
                            info!(
                                "switching to mutable mode (origin + {} > {:?})",
                                imm_after_slots, tip.0
                            );
                            stage.mutable = true;
                        }
                    }
                }

                stage
                    .chain_db
                    .rollback_to_origin(stage.mutable)
                    .or_retry()?;
                debug!("chaindb rollbacked to origin");
            }
        }

        stage
            .health_downstream
            .send(unit.clone().into())
            .await
            .or_panic()?;

        Ok(())
    }
}
