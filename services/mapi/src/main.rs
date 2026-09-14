extern crate lazy_static;

use std::{fs, net::SocketAddr};

use tracing_subscriber::fmt;
use utoipa::OpenApi;

use mapi::{app, args::Args, config::Config, ApiDoc};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let format = fmt::format()
        .with_level(true)
        .with_target(false)
        .with_thread_ids(false)
        .with_thread_names(false)
        .with_ansi(false) // Remove colors and styling
        .without_time(); // Remove timestamps

    fmt().event_format(format).init();

    let args = Args::default();

    let mut docs = ApiDoc::openapi();

    docs.paths
        .paths
        .iter_mut()
        .flat_map(|(_, path)| &mut path.operations)
        .for_each(|(_, operation)| {
            if let Some(raw_description) = operation.description.as_ref() {
                let (summary, desc) = raw_description
                    .split_once("\n\n")
                    .unwrap_or((raw_description, ""));
                operation.summary = Some(summary.into());
                operation.description = if desc.is_empty() {
                    None
                } else {
                    Some(desc.to_string())
                };
            }
        });

    match args {
        Args::Docs => {
            fs::write("docs/indexer/swagger.json", docs.to_pretty_json()?)?;
        }
        Args::Server => {
            tracing::info!("Starting mapi server...");
            let config = Config::default();

            let addr = SocketAddr::from(([0, 0, 0, 0], config.mapi_port));

            let app = app(config).await?;

            tracing::info!("🚀 server listening on {}", addr);

            axum::Server::bind(&addr)
                .serve(app.into_make_service())
                .await?;
        }
    }

    Ok(())
}
