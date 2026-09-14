use super::model::PullEvent;
use actix_web::{
    middleware,
    web::{self, Data, Json},
    App, HttpRequest, HttpServer,
};
use gasket::framework::*;
use pallas::network::miniprotocols::Point;
use serde::Serialize;
use std::{ops::Deref, sync::RwLock};

pub type PullUpstreamPort = gasket::messaging::tokio::InputPort<PullEvent>;
pub type RollUpstreamPort = gasket::messaging::tokio::InputPort<PullEvent>;

#[derive(Debug)]
pub enum HealthUnit {
    Pull(PullEvent),
    Roll(PullEvent),
}

#[derive(Stage)]
#[stage(name = "health", unit = "HealthUnit", worker = "Worker")]
pub struct Stage {
    endpoint: String,
    pub pull_upstream: PullUpstreamPort,
    pub roll_upstream: RollUpstreamPort,
}

impl Stage {
    pub fn new(endpoint: String) -> Self {
        Self {
            endpoint,
            pull_upstream: Default::default(),
            roll_upstream: Default::default(),
        }
    }
}

#[derive(Serialize, Clone)]
pub struct Tip {
    slot: u64,
    hash: Option<String>,
}

#[derive(Default, Serialize, Clone)]
pub struct Health {
    upstream: Tip,
    pull_stage: Tip,
    roll_stage: Tip,
    stats: Tip,
}

impl Default for Tip {
    fn default() -> Self {
        Self {
            slot: 0,
            hash: None,
        }
    }
}

pub struct Worker {
    health: web::Data<RwLock<Health>>,
}

#[async_trait::async_trait(?Send)]
impl gasket::framework::Worker<Stage> for Worker {
    async fn bootstrap(stage: &Stage) -> Result<Self, WorkerError> {
        let health_lock = web::Data::new(RwLock::new(Default::default()));

        let worker = Worker {
            health: health_lock.clone(),
        };

        let server = HttpServer::new(move || {
            App::new()
                .app_data(health_lock.clone())
                .wrap(middleware::Logger::default())
                .service(web::resource("/health").to(health))
        })
        .bind(stage.endpoint.clone())
        .or_panic()?
        .run();

        tokio::task::spawn(server);

        Ok(worker)
    }

    async fn schedule(
        &mut self,
        stage: &mut Stage,
    ) -> Result<WorkSchedule<HealthUnit>, WorkerError> {
        tokio::select! {
            msg = stage.pull_upstream.recv() => {
                let msg = msg.or_panic()?;
                Ok(WorkSchedule::Unit(HealthUnit::Pull(msg.payload)))
            }
            msg = stage.roll_upstream.recv() => {
                let msg = msg.or_panic()?;
                Ok(WorkSchedule::Unit(HealthUnit::Roll(msg.payload)))
            }
        }
    }

    async fn execute(&mut self, unit: &HealthUnit, _stage: &mut Stage) -> Result<(), WorkerError> {
        match unit {
            HealthUnit::Pull(x) => {
                let mut health_lock = self.health.write().or_restart()?;

                let (slot, hash, tip) = match x {
                    PullEvent::RollForward(s, h, _, t) => (*s, Some(h), t),
                    PullEvent::RollBack(s, h, t) => (*s, Some(h), t),
                    PullEvent::RollBackOrigin(t) => (0, None, t),
                };

                health_lock.pull_stage.slot = slot;
                health_lock.pull_stage.hash = hash.map(|x| hex::encode(x.to_vec()));

                if let Point::Specific(s, h) = &tip.0 {
                    health_lock.upstream.slot = *s;
                    health_lock.upstream.hash = Some(hex::encode(h.to_vec()))
                } else {
                    health_lock.upstream.slot = 0;
                    health_lock.upstream.hash = None;
                }
            }
            HealthUnit::Roll(x) => {
                let mut health_lock = self.health.write().or_restart()?;

                let (slot, hash, _) = match x {
                    PullEvent::RollForward(s, h, _, t) => (*s, Some(h), t),
                    PullEvent::RollBack(s, h, t) => (*s, Some(h), t),
                    PullEvent::RollBackOrigin(t) => (0, None, t),
                };

                health_lock.roll_stage.slot = slot;
                health_lock.roll_stage.hash = hash.map(|x| hex::encode(x.to_vec()));
            }
        }

        Ok(())
    }
}

/// endpoint handler
async fn health(health: Data<RwLock<Health>>, _: HttpRequest) -> Json<Health> {
    let health = health.read().unwrap();
    Json(health.deref().clone())
}
