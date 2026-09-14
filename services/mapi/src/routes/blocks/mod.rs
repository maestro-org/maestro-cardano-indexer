mod block_info;
mod latest_block;

use axum::{routing::get, Router};

pub use block_info::*;
pub use latest_block::*;

pub fn get_blocks_routes() -> Router {
    Router::new()
        .route("/latest", get(latest_block))
        .route("/:hash_or_height", get(block_info))
}
