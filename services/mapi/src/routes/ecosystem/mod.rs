mod adahandle_resolve;

use axum::{routing::get, Router};

pub use adahandle_resolve::*;

pub fn get_ecosystem_routes() -> Router {
    Router::new().route("/adahandle/:handle", get(adahandle_resolve))
}
