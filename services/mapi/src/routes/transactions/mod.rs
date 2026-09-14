mod address_by_txo;
mod evaluate_redeemers;
mod tx_cbor_by_tx_hash;
mod tx_info;
mod txo_by_txo_ref;
mod txos_by_txo_refs;

use axum::{
    routing::{get, post},
    Router,
};

pub use address_by_txo::*;
pub use evaluate_redeemers::*;
pub use tx_cbor_by_tx_hash::*;
pub use tx_info::*;
pub use txo_by_txo_ref::*;
pub use txos_by_txo_refs::*;

pub fn get_transactions_routes() -> Router {
    Router::new()
        .route("/outputs", post(txos_by_txo_refs))
        .route("/evaluate", post(evaluate_redeemers))
        .route("/:tx_hash", get(tx_info))
        .route("/:tx_hash/cbor", get(tx_cbor_by_tx_hash))
        .route("/:tx_hash/outputs/:index/address", get(address_by_txo))
        .route("/:tx_hash/outputs/:index/txo", get(txo_by_txo_ref))
}
