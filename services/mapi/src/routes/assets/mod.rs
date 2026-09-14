mod asset_accounts;
mod asset_addresses;
mod asset_info;
mod asset_mints;
mod asset_txs;
mod asset_utxos;

mod policy_accounts;
mod policy_addresses;
mod policy_assets;
mod policy_info;
mod policy_mints;
mod policy_txs;
mod policy_utxos;

use axum::{routing::get, Router};

pub use asset_accounts::*;
pub use asset_addresses::*;
pub use asset_info::*;
pub use asset_mints::*;
pub use asset_txs::*;
pub use asset_utxos::*;

pub use policy_accounts::*;
pub use policy_addresses::*;
pub use policy_assets::*;
pub use policy_info::*;
pub use policy_mints::*;
pub use policy_txs::*;
pub use policy_utxos::*;

pub fn get_assets_routes() -> Router {
    Router::new()
        .route("/:asset", get(asset_info))
        .route("/:asset/accounts", get(asset_accounts))
        .route("/:asset/addresses", get(asset_addresses))
        .route("/:asset/utxos", get(asset_utxos))
        .route("/:asset/transactions", get(asset_txs))
        .route("/:asset/mints", get(asset_mints))
}

pub fn get_policy_routes() -> Router {
    Router::new()
        .route("/:policy", get(policy_info))
        .route("/:policy/accounts", get(policy_accounts))
        .route("/:policy/addresses", get(policy_addresses))
        .route("/:policy/assets", get(policy_assets))
        .route("/:policy/utxos", get(policy_utxos))
        .route("/:policy/transactions", get(policy_txs))
        .route("/:policy/mints", get(policy_mints))
}
