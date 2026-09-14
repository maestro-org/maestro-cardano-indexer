use std::path::PathBuf;

use clap::{Parser, Subcommand};
use common::LoggingConfig;
use miette::{Context, IntoDiagnostic, Result};
use serde::Deserialize;

mod common;
mod daemon;
mod serve;
mod sync;

#[derive(Debug, Subcommand)]
enum Command {
    Sync(sync::Args),
    Serve(serve::Args),
    Daemon(daemon::Args),
}

#[derive(Debug, Parser)]
#[clap(name = "Compressor")]
#[clap(bin_name = "compressor")]
#[clap(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,

    config: Option<std::path::PathBuf>,
}

#[derive(Deserialize)]
pub struct ChainDBConfig {
    path: std::path::PathBuf,
    immutable_after_slots: Option<u64>,
}

#[derive(Deserialize)]
pub struct GenesisFileRef {
    path: PathBuf,
}

#[derive(Deserialize)]
pub struct Config {
    pub logging: LoggingConfig,
    pub chain_db: ChainDBConfig,
    pub sync: compressor::sync::Config,
    pub serve: compressor::serve::Config,
    pub byron: GenesisFileRef,
}

impl Config {
    pub fn new(explicit_file: &Option<std::path::PathBuf>) -> Result<Self, config::ConfigError> {
        let mut s = config::Config::builder();

        // file in the working dir
        s = s.add_source(config::File::with_name("compressor.toml").required(false));

        // if an explicit file was passed, then we load it as mandatory
        if let Some(explicit) = explicit_file.as_ref().and_then(|x| x.to_str()) {
            s = s.add_source(config::File::with_name(explicit).required(true));
        }

        // finally, we use env vars to make some last-step overrides
        s = s.add_source(config::Environment::with_prefix("COMPR").separator("_"));

        s.build()?.try_deserialize()
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Cli::parse();
    let config = Config::new(&args.config)
        .into_diagnostic()
        .context("parsing configuration")?;

    match args.command {
        Command::Sync(x) => sync::run(&config, &x)?,
        Command::Serve(x) => serve::run(&config, &x).await?,
        Command::Daemon(x) => daemon::run(&config, &x).await?,
    };

    Ok(())
}
