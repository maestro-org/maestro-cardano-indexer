use crate::responses::{AccountAction, AccountUpdate, ErrorResponse, PaginatedResponse};
use crate::utils::{self, bad_request, internal_server_error, ParsedCursorPageParams};
use crate::{CountParam, CursorPagination, MapiExtension, OrderParam};
use axum::{
    extract::{Path, Query},
    http::StatusCode,
    response::IntoResponse,
    Extension, Json,
};
use base64::{engine::general_purpose as b64, Engine};
use serde::Deserialize;
use sqlx::types::BigDecimal;

#[derive(Debug, Deserialize)]
pub struct Params {
    pub order: Option<OrderParam>,
}

#[utoipa::path(
    tag = "Accounts",
    get,
    path = "/accounts/{stake_addr}/updates",
    params(
        ("stake_addr" = String, Path, description = "Bech32 encoded stake/reward address ('stake1...')"),

        ("count" = inline(Option<CountParam>), Query, description = "The max number of results per page"),
        ("order" = inline(Option<OrderParam>), Query, description = "The order in which the results are sorted (by absolute slot)"),
        ("cursor" = inline(Option<String>), Query, description = "Pagination cursor string, use the cursor included in a page of results to fetch the next page"),
    ),
    responses(
        (
            status = 200,
            description = "Updates relating to the specified stake key (registration, deregistration, delegation, withdrawal)",
            body = PaginatedAccountUpdate,
            example = json!({
                "data": [{
                    "action": "registration",
                    "tx_hash": "3058ba8de662ea6bd6572aeea69047c74de1eb4989c75e818d0d2ea4cb3f0a10",
                    "epoch_no": 162,
                    "abs_slot": 68770093,
                    "deposit": "2000000"
                }, {
                    "action": "withdrawal",
                    "tx_hash": "c90b48bcb8ec6b2ab1c622f55b34e488cc235d01907072014f9aaa58d883aba2",
                    "epoch_no": 162,
                    "abs_slot": 68770469,
                    "deposit": null
                }, {
                    "action": "withdrawal",
                    "tx_hash": "4e1bea9b85be4fa9ca2c88e941b7e58da84e026b5d95088e4e387f1bbccbfa40",
                    "epoch_no": 162,
                    "abs_slot": 68771196,
                    "deposit": null
                }],
                "last_updated": {
                    "timestamp": "2022-08-16 09:01:44",
                    "block_hash": "b740737679995380c05b41fc82a9e383c56c931770c46d04967532844792dfec",
                    "block_slot": 69074213
                },
                "next_cursor": null
            })
        ),
        (status = 400, description = "Malformed query parameters"),
        (status = 500, description = "Internal server error"),
    )
)]
#[tracing::instrument(name = "ACCOUNT_UPDATES", level = "info", skip(config))]
/// Stake account updates
///
/// Returns a list of updates relating to the specified stake key (`registration`, `deregistration`, `delegation`, `withdrawal`)
pub async fn account_updates(
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

    let row: (sqlx::types::Json<Vec<KoiosAccountUpdate>>,) =
        sqlx::query_as("SELECT updates FROM grest.account_updates($1)")
            .bind(vec![stake_addr])
            .fetch_optional(&dbsync)
            .await
            .map_err(internal_server_error)?
            .unwrap_or_default();

    let mut updates = row.0.to_vec();

    // --- cursor pagination

    match order {
        OrderParam::Asc => updates.sort_by_key(|a| a.abs_slot),
        OrderParam::Desc => updates.sort_by_key(|b| std::cmp::Reverse(b.abs_slot)),
    };

    let page_start_idx = if let Some(a) = cursor {
        if a > updates.len() {
            return Err(bad_request("Malformed cursor"));
        }

        updates = updates.split_off(a);

        a
    } else {
        0
    };

    let next_cursor = if updates.len() > count {
        let last_item_idx = page_start_idx + count;

        Some(encode_cursor(last_item_idx))
    } else {
        None
    };

    updates.truncate(count);

    let mut out = vec![];

    for update in updates.into_iter() {
        let deposit = match update.action {
            AccountAction::Registration => Some(
                sqlx::query_as::<_, (BigDecimal,)>(
                    "SELECT sr.deposit
                    FROM stake_registration sr
                    JOIN tx t ON sr.tx_id = t.id
                    WHERE t.hash = $1;",
                )
                .bind(hex::decode(&update.tx_hash).unwrap())
                .fetch_one(&dbsync)
                .await
                .map_err(internal_server_error)?,
            ),
            _ => None,
        };

        out.push(AccountUpdate {
            action: update.action,
            tx_hash: update.tx_hash,
            epoch_no: update.epoch_no,
            abs_slot: update.abs_slot,
            deposit: deposit.map(|x: (BigDecimal,)| x.0.to_string()),
        })
    }

    // ---

    let out = PaginatedResponse {
        data: out,
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

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
/// Stake account related update
pub struct KoiosAccountUpdate {
    #[serde(alias = "action_type")]
    pub action: AccountAction,
    /// Transaction hash of the transaction which performed the action
    pub tx_hash: String,
    /// Epoch number in which the transaction occured
    pub epoch_no: i32,
    #[serde(alias = "absolute_slot")]
    /// Absolute slot of the block which contained the transaction
    pub abs_slot: i32,
}
