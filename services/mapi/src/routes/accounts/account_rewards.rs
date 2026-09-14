use crate::responses::{AccountReward, AccountRewardType, ErrorResponse, PaginatedResponse};
use crate::utils::{
    self, bad_request, de_u64_from_str, internal_server_error, ParsedCursorPageParams,
};
use crate::{CountParam, CursorPagination, MapiExtension, OrderParam};
use axum::http::HeaderMap;
use axum::{
    extract::{Path, Query},
    http::StatusCode,
    response::IntoResponse,
    Extension, Json,
};
use base64::{engine::general_purpose as b64, Engine};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct Params {
    pub order: Option<OrderParam>,
}

#[utoipa::path(
    tag = "Accounts",
    get,
    path = "/accounts/{stake_addr}/rewards",
    params(
        ("stake_addr" = String, Path, description = "Bech32 encoded stake/reward address ('stake1...')"),

        ("count" = inline(Option<CountParam>), Query, description = "The max number of results per page"),
        ("order" = inline(Option<OrderParam>), Query, description = "The order in which the results are sorted (by epoch number)"),
        ("cursor" = inline(Option<String>), Query, description = "Pagination cursor string, use the cursor included in a page of results to fetch the next page"),

        ("amounts-as-strings" = Option<String>, Header, description = "Large numbers returned as strings if set to `true`")
    ),
    responses(
        (
            status = 200,
            description = "Staking rewards for the specified stake key (pool-member and pool-leader rewards, deposit refunds)",
            body = PaginatedAccountReward,
            example = json!({
                "data": [
                    {
                        "type": "leader",
                        "earned_epoch": 68,
                        "spendable_epoch": 70,
                        "amount": 576671933,
                        "pool_id": "pool1egfg26w0syqly9qc65hz33gqv2qrzyka8tfue3ccsk3c73a56jp"
                    },
                    {
                        "type": "leader",
                        "earned_epoch": 69,
                        "spendable_epoch": 71,
                        "amount": 567061410,
                        "pool_id": "pool1egfg26w0syqly9qc65hz33gqv2qrzyka8tfue3ccsk3c73a56jp"
                    },
                    {
                        "type": "leader",
                        "earned_epoch": 70,
                        "spendable_epoch": 72,
                        "amount": 559723577,
                        "pool_id": "pool1egfg26w0syqly9qc65hz33gqv2qrzyka8tfue3ccsk3c73a56jp"
                    },
                    {
                        "type": "leader",
                        "earned_epoch": 71,
                        "spendable_epoch": 73,
                        "amount": 521198513,
                        "pool_id": "pool1egfg26w0syqly9qc65hz33gqv2qrzyka8tfue3ccsk3c73a56jp"
                    },
                    {
                        "type": "leader",
                        "earned_epoch": 72,
                        "spendable_epoch": 74,
                        "amount": 541678766,
                        "pool_id": "pool1egfg26w0syqly9qc65hz33gqv2qrzyka8tfue3ccsk3c73a56jp"
                    }
                ],
                "last_updated": {
                    "timestamp": "2022-10-10 20:25:28",
                    "block_hash": "82b6eee6d8bc3ca82e450d94bed5bfbb7859417840e3db8a3bf639a46bcbe379",
                    "block_slot": 32265228
                },
                "next_cursor": null
            })
        ),
        (status = 400, description = "Malformed query parameters"),
        (status = 500, description = "Internal server error"),
    )
)]
#[tracing::instrument(name = "ACCOUNT_REWARDS", level = "info", skip(config, headers))]
/// Stake account rewards
///
/// Returns a list of staking-related rewards for the specified stake key (pool `member` or `leader` rewards, deposit `refund`)
pub async fn account_rewards(
    headers: HeaderMap,
    page_params: Query<CursorPagination>,
    params: Query<Params>,
    Path(stake_addr): Path<String>,
    Extension(config): MapiExtension,
) -> Result<impl IntoResponse, ErrorResponse> {
    let dbsync = config.dbsync;
    let chain = config.chain_info;

    // -- parse and try decode user params

    let ParsedCursorPageParams { count, cursor } =
        utils::parse_cursor_page_params(page_params.0, decode_cursor)?;

    let order = params.order.unwrap_or(OrderParam::Asc);

    // --- get dbsync tip for last updated

    let last_updated = utils::get_last_updated_dbsync(&chain, &dbsync).await?;

    // --- fetch all data from db

    let row: (sqlx::types::Json<Vec<DbsyncAccountReward>>,) =
        sqlx::query_as("SELECT rewards FROM grest.account_rewards($1)")
            .bind(vec![stake_addr])
            .fetch_optional(&dbsync)
            .await
            .map_err(internal_server_error)?
            .unwrap_or_default();

    let mut rewards = Vec::new();

    // filter rewards for only staking-related rewards
    for reward in row.0.iter().cloned() {
        match reward.reward_type {
            AccountRewardType::Member => rewards.push(reward),
            AccountRewardType::Leader => rewards.push(reward),
            AccountRewardType::Refund => rewards.push(reward),
            _ => (),
        }
    }

    if order == OrderParam::Desc {
        rewards.reverse()
    }

    // --- cursor pagination

    let page_start_idx = if let Some(a) = cursor {
        if a > rewards.len() {
            return Err(bad_request("Malformed cursor"));
        }

        rewards = rewards.split_off(a);

        a
    } else {
        0
    };

    let next_cursor = if rewards.len() > count {
        let last_item_idx = page_start_idx + count;

        Some(encode_cursor(last_item_idx))
    } else {
        None
    };

    rewards.truncate(count);

    let rewards = rewards
        .into_iter()
        .map(|x| AccountReward {
            reward_type: x.reward_type,
            earned_epoch: x.earned_epoch,
            spendable_epoch: x.spendable_epoch,
            amount: utils::u64_str_conv(x.amount, &headers),
            pool_id: x.pool_id,
        })
        .collect();

    // ---

    let out = PaginatedResponse {
        data: rewards,
        last_updated,
        next_cursor,
    };

    Ok((StatusCode::OK, Json(out)))
}

fn decode_cursor(b64_cursor: &String) -> Result<usize, ErrorResponse> {
    let bytes: [u8; 8] = b64::URL_SAFE_NO_PAD
        .decode(b64_cursor)
        .map_err(|_| bad_request("Malformed cursor"))?
        .try_into()
        .map_err(|_| bad_request("Malformed cursor"))?;

    Ok(u64::from_be_bytes(bytes) as usize)
}

fn encode_cursor(item: usize) -> String {
    b64::URL_SAFE_NO_PAD.encode(u64::to_be_bytes(item as u64))
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
/// Stake account related reward
pub struct DbsyncAccountReward {
    #[serde(rename = "type")]
    pub reward_type: AccountRewardType,
    /// Epoch in which the reward was earned
    pub earned_epoch: i32,
    /// Epoch at which the reward is spendable
    pub spendable_epoch: i32,
    #[serde(deserialize_with = "de_u64_from_str")]
    /// Reward amount
    pub amount: u64,
    /// Bech32 encoded pool ID (if relevant to reward type)
    pub pool_id: Option<String>,
}
