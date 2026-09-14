use std::time::Duration;

use gasket::messaging::{RecvPort, SendPort};
use pallas_configs::byron;
use serde::Deserialize;
use tracing::info;

use crate::prelude::Error;
use crate::storage::txos::TxoKV;
use crate::storage::ChainDB;

mod health;
mod model;
mod pull;
mod roll;

#[derive(Deserialize)]
pub struct Config {
    pub node_socket: String,
    pub network_magic: u64,
    pub health_endpoint: String,
}

fn define_gasket_policy(config: &Option<gasket::retries::Policy>) -> gasket::runtime::Policy {
    let default_retries = gasket::retries::Policy {
        max_retries: 20,
        backoff_unit: Duration::from_secs(1),
        backoff_factor: 2,
        max_backoff: Duration::from_secs(60),
        dismissible: false,
    };

    let retries = config.clone().unwrap_or(default_retries);

    gasket::runtime::Policy {
        tick_timeout: std::time::Duration::from_secs(600).into(),
        bootstrap_retry: retries.clone(),
        work_retry: retries.clone(),
        teardown_retry: retries.clone(),
    }
}

pub fn pipeline(
    config: &Config,
    chain_db: &ChainDB,
    byron_config: byron::GenesisFile,
    retries: &Option<gasket::retries::Policy>,
) -> Result<gasket::daemon::Daemon, Error> {
    let pull_cursor = chain_db
        .intersect_options(5)
        .map_err(Error::storage)?
        .into_iter()
        .collect();

    let mut pull = pull::Stage::new(
        config.node_socket.clone(),
        config.network_magic,
        pull_cursor,
    );

    let chain_cursor = chain_db.cursor().map_err(Error::storage)?;
    info!(?chain_cursor, "chain cursor");

    TxoKV::insert_genesis_txos(&chain_db.db, byron_config).map_err(Error::server)?;

    let mut roll = roll::Stage::new(chain_db.clone());

    let (to_roll, from_pull) = gasket::messaging::tokio::mpsc_channel(50);
    pull.downstream.connect(to_roll);
    roll.upstream.connect(from_pull);

    let (pull_to_health, pull_from_health) = gasket::messaging::tokio::mpsc_channel(50);
    let (roll_to_health, roll_from_health) = gasket::messaging::tokio::mpsc_channel(50);

    let mut health = health::Stage::new(config.health_endpoint.clone());

    health.pull_upstream.connect(pull_from_health);
    pull.health_downstream.connect(pull_to_health);

    health.roll_upstream.connect(roll_from_health);
    roll.health_downstream.connect(roll_to_health);

    let policy = define_gasket_policy(retries);

    let pull = gasket::runtime::spawn_stage(pull, policy.clone());
    let roll = gasket::runtime::spawn_stage(roll, policy.clone());
    let health = gasket::runtime::spawn_stage(health, policy);

    Ok(gasket::daemon::Daemon(vec![pull, roll, health]))
}
