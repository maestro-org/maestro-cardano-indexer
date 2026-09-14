mod account_addresses;
mod account_assets;
mod account_delegations;
mod account_history;
mod account_info;
mod account_rewards;
mod account_updates;

use axum::{routing::get, Router};

pub use account_addresses::*;
pub use account_assets::*;
pub use account_delegations::*;
pub use account_history::*;
pub use account_info::*;
pub use account_rewards::*;
pub use account_updates::*;

pub fn get_accounts_router() -> Router {
    Router::new()
        .route("/:stake_addr", get(account_info))
        .route("/:stake_addr/addresses", get(account_addresses))
        .route("/:stake_addr/assets", get(account_assets))
        .route("/:stake_addr/history", get(account_history))
        .route("/:stake_addr/updates", get(account_updates))
        .route("/:stake_addr/rewards", get(account_rewards))
        .route("/:stake_addr/delegations", get(account_delegations))
}
