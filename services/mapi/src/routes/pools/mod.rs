mod list_pools;
mod pool_blocks;
mod pool_delegators;
mod pool_historical_delegators;
mod pool_history;
mod pool_info;
mod pool_metadata;
mod pool_relays;
mod pool_updates;

use axum::{routing::get, Router};

pub use list_pools::*;
pub use pool_blocks::*;
pub use pool_delegators::*;
pub use pool_historical_delegators::*;
pub use pool_history::*;
pub use pool_info::*;
pub use pool_metadata::*;
pub use pool_relays::*;
pub use pool_updates::*;

pub fn get_pools_routes() -> Router {
    Router::new()
        .route("/", get(list_pools))
        .route("/:pool_id/blocks", get(pool_blocks))
        .route("/:pool_id/delegators", get(pool_delegators))
        .route(
            "/:pool_id/delegators/:epoch_no",
            get(pool_historical_delegators),
        )
        .route("/:pool_id/history", get(pool_history))
        .route("/:pool_id/info", get(pool_info))
        .route("/:pool_id/metadata", get(pool_metadata))
        .route("/:pool_id/relays", get(pool_relays))
        .route("/:pool_id/updates", get(pool_updates))
}
