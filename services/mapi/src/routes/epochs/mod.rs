mod current_epoch;
mod epoch_info;

use axum::{routing::get, Router};

pub use current_epoch::*;
pub use epoch_info::*;

pub fn get_epochs_routes() -> Router {
    Router::new()
        .route("/:epoch_no", get(epoch_info))
        .route("/current", get(current_epoch))
}
