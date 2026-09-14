use clap::Parser;
use std::process;

mod console;
mod daemon;

#[derive(Parser)]
#[clap(name = "Polyphony")]
#[clap(bin_name = "polyphony")]
#[clap(author, version, about, long_about = None)]
enum Polyphony {
    Daemon(daemon::Args),
}

#[tokio::main]
async fn main() {
    let args = Polyphony::parse();

    let result = match args {
        Polyphony::Daemon(x) => daemon::run(&x).await,
    };

    if let Err(err) = &result {
        eprintln!("ERROR: {:#?}", err);
        process::exit(1);
    }

    process::exit(0);
}
