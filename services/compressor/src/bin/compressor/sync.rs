use compressor::storage::ChainDB;
use miette::{Context, IntoDiagnostic};

#[derive(Debug, clap::Args)]
pub struct Args {}

pub fn run(config: &super::Config, _args: &Args) -> miette::Result<()> {
    super::common::setup_tracing(&config.logging)?;
    super::common::setup_os_signal_hooks()?;

    let byron_genesis = pallas_configs::byron::from_file(&config.byron.path)
        .into_diagnostic()
        .context("loading byron config")?;

    let chain_db = &ChainDB::open(&config.chain_db.path, config.chain_db.immutable_after_slots)
        .into_diagnostic()
        .context("opening chaindb")?;

    compressor::sync::pipeline(&config.sync, chain_db, byron_genesis, &None)
        .into_diagnostic()
        .context("initialising sync pipeline")?
        .block();

    Ok(())
}
