mod filters;

mod balance_by_payment_cred;
mod decode_address;
mod tx_count_by_address;
mod txs_by_address;
mod txs_by_payment_cred;
mod txs_by_payment_creds;
mod utxo_refs_at_address;
mod utxos_by_address;
mod utxos_by_addresses;
mod utxos_by_payment_cred;
mod utxos_by_payment_creds;

use axum::{
    routing::{get, post},
    Router,
};

pub use filters::*;

pub use balance_by_payment_cred::*;
pub use decode_address::*;
pub use tx_count_by_address::*;
pub use txs_by_address::*;
pub use txs_by_payment_cred::*;
pub use txs_by_payment_creds::*;
pub use utxo_refs_at_address::*;
pub use utxos_by_address::*;
pub use utxos_by_addresses::*;
pub use utxos_by_payment_cred::*;
pub use utxos_by_payment_creds::*;

pub fn get_addresses_router() -> Router {
    Router::new()
        .route("/:address/utxos", get(utxos_by_address))
        .route("/:address/utxo_refs", get(utxo_refs_at_address))
        .route("/:address/transactions", get(txs_by_address))
        .route("/:address/transactions/count", get(tx_count_by_address))
        .route("/utxos", post(utxos_by_addresses))
        .route("/:address/decode", get(decode_address))
        .route("/cred/utxos", post(utxos_by_payment_creds))
        .route("/cred/transactions", post(txs_by_payment_creds))
        .route("/cred/:credential/balance", get(balance_by_payment_cred))
        .route("/cred/:credential/utxos", get(utxos_by_payment_cred))
        .route("/cred/:credential/transactions", get(txs_by_payment_cred))
}
