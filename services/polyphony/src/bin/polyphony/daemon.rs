use gasket::runtime::{StagePhase, TetherState};
use polyphony::{bootstrap, crosscut, reducers, sources, storage};
use serde::Deserialize;
use std::time::Duration;
use tracing::info;

use crate::console;

#[derive(Deserialize)]
struct ConfigRoot {
    source: sources::Config,
    reducers: Vec<reducers::Config>,
    storage: storage::Config,
    intersect: crosscut::IntersectConfig,
    policy: Option<crosscut::policies::RuntimePolicy>,
}

impl ConfigRoot {
    pub fn new(explicit_file: &Option<std::path::PathBuf>) -> Result<Self, config::ConfigError> {
        let mut s = config::Config::builder();

        // our base config will always be in /etc/polyphony
        s = s.add_source(config::File::with_name("/etc/polyphony/daemon.toml").required(false));

        // but we can override it by having a file in the working dir
        s = s.add_source(config::File::with_name("polyphony.toml").required(false));

        // if an explicit file was passed, then we load it as mandatory
        if let Some(explicit) = explicit_file.as_ref().and_then(|x| x.to_str()) {
            s = s.add_source(config::File::with_name(explicit).required(true));
        }

        // finally, we use env vars to make some last-step overrides
        s = s.add_source(
            config::Environment::with_prefix("POLYPHONY")
                .separator("__")
                .try_parsing(true),
        );

        s.build()?.try_deserialize()
    }
}

fn should_stop(pipeline: &bootstrap::Pipeline) -> bool {
    pipeline
        .tethers
        .iter()
        .any(|tether| match tether.check_state() {
            gasket::runtime::TetherState::Alive(p) => {
                if matches!(p, StagePhase::Ended) {
                    info!("{} stage has ended, should stop", tether.name());
                    true
                } else {
                    false
                }
            }
            s => {
                info!("{} stage not alive: {:?}", tether.name(), s);
                true
            }
        })
}

async fn shutdown(pipeline: bootstrap::Pipeline) {
    for tether in pipeline.tethers {
        let mut state = tether.check_state();
        while let TetherState::Alive(_) = state {
            tokio::time::sleep(Duration::from_secs(3)).await;
            state = tether.check_state();
        }
        tracing::warn!("dismissing stage: {} with state {:?}", tether.name(), state);
        _ = tether.dismiss_stage();

        // Can't join the stage because there's a risk of deadlock, usually
        // because a stage gets stuck sending into a port which depends on a
        // different stage not yet dismissed. The solution is to either create a
        // DAG of dependencies and dismiss in the correct order, or implement a
        // 2-phase teardown where ports are disconnected and flushed
        // before joining the stage.

        //tether.join_stage();
    }
}

pub async fn run(args: &Args) -> Result<(), polyphony::Error> {
    console::initialize(&args.console);

    let config = ConfigRoot::new(&args.config)
        .map_err(|err| polyphony::Error::ConfigError(format!("{:?}", err)))?;

    let policy = config.policy.unwrap_or_default();

    let source = config.source.bootstrapper(&config.intersect);

    // Registry names are handed to the storage stage so it can advertise this
    // instance (and its indexed tip) in the Redis instance registry.
    let mut instance_names: Vec<String> = config
        .reducers
        .iter()
        .map(|r| r.instance_name().to_string())
        .collect();
    instance_names.sort();
    instance_names.dedup();

    let reducer = reducers::Bootstrapper::new(config.reducers, &policy);

    let storage = config.storage.plugin(&policy, instance_names);

    let pipeline = bootstrap::build(source, reducer, storage).await?;

    tracing::info!("Polyphony is running...");

    while !should_stop(&pipeline) {
        console::refresh(&args.console, &pipeline);
        tokio::time::sleep(Duration::from_millis(1500)).await;
    }

    tracing::info!("Polyphony is stopping...");

    shutdown(pipeline).await;

    Ok(())
}

#[derive(clap::Args)]
#[clap(author, version, about, long_about = None)]
pub struct Args {
    #[clap(long, value_parser)]
    config: Option<std::path::PathBuf>,

    #[clap(long, value_parser)]
    console: Option<console::Mode>,
}
