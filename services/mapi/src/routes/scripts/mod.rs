mod datum_by_hash;
mod datums_by_hashes;
mod script_by_hash;

use axum::{
    routing::{get, post},
    Router,
};

pub use datum_by_hash::*;
pub use datums_by_hashes::*;
pub use script_by_hash::*;

pub fn get_scripts_routes() -> Router {
    Router::new().route("/:script_hash", get(script_by_hash))
}

pub fn get_datums_routes() -> Router {
    Router::new()
        .route("/", post(datums_by_hashes))
        .route("/:datum_hash", get(datum_by_hash))
}
