use crate::responses::{AccountHistory, ErrorResponse, PaginatedResponse};
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
    pub epoch_no: Option<i32>,
    pub order: Option<OrderParam>,
}

#[utoipa::path(
    tag = "Accounts",
    get,
    path = "/accounts/{stake_addr}/history",
    params(
        ("stake_addr" = String, Path, description = "Bech32 encoded stake/reward address ('stake1...')"),

        ("epoch_no" = Option<u64>, Query, description = "Fetch result for only a specific epoch"),

        ("count" = inline(Option<CountParam>), Query, description = "The max number of results per page"),
        ("order" = inline(Option<OrderParam>), Query, description = "The order in which the results are sorted (by epoch number)"),
        ("cursor" = inline(Option<String>), Query, description = "Pagination cursor string, use the cursor included in a page of results to fetch the next page"),

        ("amounts-as-strings" = Option<String>, Header, description = "Large numbers returned as strings if set to `true`")
    ),
    responses(
        (
            status = 200,
            description = "Per-epoch history of the specified stake key",
            body = PaginatedAccountHistory,
            example = json!({
                "data": [
                    {
                        "epoch_no": 73,
                        "pool_id": "pool1egfg26w0syqly9qc65hz33gqv2qrzyka8tfue3ccsk3c73a56jp",
                        "active_stake": 270921649109i64
                    },
                    {
                        "epoch_no": 74,
                        "pool_id": "pool1egfg26w0syqly9qc65hz33gqv2qrzyka8tfue3ccsk3c73a56jp",
                        "active_stake": 271442847622i64
                    },
                    {
                        "epoch_no": 75,
                        "pool_id": "pool1egfg26w0syqly9qc65hz33gqv2qrzyka8tfue3ccsk3c73a56jp",
                        "active_stake": 271984526388i64
                    },
                    {
                        "epoch_no": 76,
                        "pool_id": "pool1egfg26w0syqly9qc65hz33gqv2qrzyka8tfue3ccsk3c73a56jp",
                        "active_stake": 272520020301i64
                    },
                    {
                        "epoch_no": 77,
                        "pool_id": "pool1egfg26w0syqly9qc65hz33gqv2qrzyka8tfue3ccsk3c73a56jp",
                        "active_stake": 273058079938i64
                    },
                    {
                        "epoch_no": 78,
                        "pool_id": "pool1egfg26w0syqly9qc65hz33gqv2qrzyka8tfue3ccsk3c73a56jp",
                        "active_stake": 273611950245i64
                    }
                ],
                "last_updated": {
                    "timestamp": "2022-10-10 20:25:28",
                    "block_hash": "b48416b482bdfda2a3c1ccbe220f84f74256d3e457bd064edea8dc45dc7e22e1",
                    "block_slot": 32209052
                },
                "next_cursor": null
            })
        ),
        (status = 400, description = "Malformed query parameters"),
        (status = 500, description = "Internal server error"),
    )
)]
#[tracing::instrument(name = "ACCOUNT_HISTORY", level = "info", skip(config, headers))]
/// Stake account history
///
/// Returns per-epoch history for the specified stake key
pub async fn account_history(
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

    let row: (sqlx::types::Json<Vec<DbsyncAccountHistory>>,) =
        sqlx::query_as("SELECT history FROM grest.account_history($1, $2)")
            .bind(vec![stake_addr])
            .bind(params.epoch_no)
            .fetch_optional(&dbsync)
            .await
            .map_err(internal_server_error)?
            .unwrap_or_default();

    let mut epochs = row.0.to_vec();

    // --- cursor pagination

    match order {
        OrderParam::Asc => {
            if let Some(a) = cursor {
                epochs.retain(|x| x.epoch_no > a);
            }
            epochs.sort_by_key(|a| a.epoch_no);
        }
        OrderParam::Desc => {
            if let Some(a) = cursor {
                epochs.retain(|x| x.epoch_no < a);
            }
            epochs.sort_by_key(|b| std::cmp::Reverse(b.epoch_no));
        }
    }

    let next_cursor = if epochs.len() > count {
        let last_epoch = epochs[count - 1].epoch_no;

        Some(encode_cursor(last_epoch))
    } else {
        None
    };

    epochs.truncate(count);

    let epochs = epochs
        .into_iter()
        .map(|x| AccountHistory {
            epoch_no: x.epoch_no,
            pool_id: x.pool_id,
            active_stake: utils::u64_str_conv(x.active_stake, &headers),
        })
        .collect();

    // ---

    let out = PaginatedResponse {
        data: epochs,
        last_updated,
        next_cursor,
    };

    Ok((StatusCode::OK, Json(out)))
}

fn decode_cursor(b64_cursor: &String) -> Result<i32, ErrorResponse> {
    let bytes: [u8; 4] = b64::URL_SAFE_NO_PAD
        .decode(b64_cursor)
        .map_err(|_| bad_request("Malformed cursor"))?
        .try_into()
        .map_err(|_| bad_request("Malformed cursor"))?;

    Ok(i32::from_be_bytes(bytes))
}

fn encode_cursor(item: i32) -> String {
    b64::URL_SAFE_NO_PAD.encode(i32::to_be_bytes(item))
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
/// Per-epoch information about a stake account
pub struct DbsyncAccountHistory {
    /// Epoch number
    pub epoch_no: i32,
    /// Bech32 encoded pool ID the account was delegated to
    pub pool_id: Option<String>,
    #[serde(deserialize_with = "de_u64_from_str")]
    /// Active stake of the account in the epoch
    pub active_stake: u64,
}
