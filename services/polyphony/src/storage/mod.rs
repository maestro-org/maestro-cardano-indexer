pub mod redis_entry;
pub mod tikv;

use gasket::{error::AsWorkError, messaging::tokio::InputPort};
use log::debug;
use pallas::network::miniprotocols::Point;
use serde::Deserialize;
use tikv_client::{Timestamp, TimestampExt, Transaction};
use tracing::info;

use crate::{bootstrap, crosscut, model, rollback::PersistentBufferValue};

#[derive(Deserialize)]
#[serde(tag = "type")]
pub enum Config {
    TiKV(tikv::Config),
}

impl Config {
    pub fn plugin(
        self,
        policy: &crosscut::policies::RuntimePolicy,
        instance_names: Vec<String>,
    ) -> Bootstrapper {
        match self {
            Config::TiKV(c) => Bootstrapper::TiKV(c.bootstrapper(policy, instance_names)),
        }
    }
}

pub enum Bootstrapper {
    TiKV(tikv::Bootstrapper),
}

impl Bootstrapper {
    pub fn borrow_input_port(&mut self) -> &'_ mut InputPort<model::StorageActionPayload> {
        match self {
            Bootstrapper::TiKV(x) => x.borrow_input_port(),
        }
    }

    pub fn build_cursor(&mut self) -> Cursor {
        info!("building cursor");
        match self {
            Bootstrapper::TiKV(x) => Cursor::TiKV(x.build_cursor()),
        }
    }

    pub fn spawn_stages(
        self,
        pipeline: &mut bootstrap::Pipeline,
        intersect: Option<Point>,
        buf: Option<Vec<PersistentBufferValue>>,
    ) {
        match self {
            Bootstrapper::TiKV(x) => x.spawn_stages(pipeline, intersect, buf),
        }
    }
}

pub enum Cursor {
    TiKV(tikv::Cursor),
}

impl Cursor {
    pub async fn last_point(&mut self) -> Result<Option<Point>, crate::Error> {
        match self {
            Cursor::TiKV(x) => x.last_point().await,
        }
    }

    pub async fn fetch_persistent_buffer(
        &mut self,
    ) -> Result<Option<Vec<PersistentBufferValue>>, crate::Error> {
        info!("fetching persistent buffer");
        match self {
            Cursor::TiKV(x) => x.fetch_persistent_buffer().await,
        }
    }
}

pub async fn commit_txn_or_rollback(
    txn: &mut Transaction,
) -> Result<Option<Timestamp>, gasket::error::Error> {
    match txn.commit().await {
        Ok(ts) => {
            debug!(
                "finished committing: {ts:?} ({:?})",
                ts.as_ref().map(|t| t.version())
            );

            Ok(ts)
        }
        Err(e) => {
            info!("error while committing, rolling back txn...");

            txn.rollback().await.or_panic()?;

            info!("rollbacked database txn successfully");

            // If the error returned means we don't know whether the transaction committed
            // successfully or not, then panic. We will then resolve on start up.
            if matches!(
                e,
                tikv_client::Error::Grpc(_)
                    | tikv_client::Error::GrpcAPI(_)
                    | tikv_client::Error::Canceled(_)
                    | tikv_client::Error::UndeterminedError(_)
            ) {
                Err(e).or_panic()
            } else {
                Err(e).or_restart()
            }
        }
    }
}
