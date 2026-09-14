use crate::responses::{self, Address, ErrorResponse, PaginatedResponse};
use crate::utils::{self, bad_request, internal_server_error, ParsedCursorPageParams};
use crate::{CountParam, CursorPagination, MapiExtension};
use axum::{
    extract::{Path, Query},
    http::StatusCode,
    response::IntoResponse,
    Extension, Json,
};
use base64::{engine::general_purpose as b64, Engine};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct Params {
    pub include_empty: Option<bool>,
}

#[utoipa::path(
    tag = "Accounts",
    get,
    path = "/accounts/{stake_addr}/addresses",
    params(
        ("stake_addr" = String, Path, description = "Bech32 encoded stake/reward address ('stake1...')"),

        ("include_empty" = Option<bool>, Query, description = "Include addresses that have been seen on-chain but have no balance"),

        ("count" = inline(Option<CountParam>), Query, description = "The max number of results per page"),
        ("cursor" = inline(Option<String>), Query, description = "Pagination cursor string, use the cursor included in a page of results to fetch the next page"),
    ),
    responses(
        (
            status = 200,
            description = "Addresses seen on-chain which contain the specified stake key",
            body = PaginatedAddress,
            example = json!({
                "data": [
                    "addr_test1qpshevet4xrh7h2rr7pm3r7ysn8x9hk5gek9xvm6du3yl6upahs8fhhy49jqqnfn6vvmtq6yeqypx645ex9fxexlh36q9scvx8"
                ],
                "last_updated": {
                    "timestamp": "2022-10-10 20:25:28",
                    "block_hash": "bbd15af995f81cf22e559679865d5a0fb1ba348559cf88e07686234e499859c4",
                    "block_slot": 32208435
                },
                "next_cursor": null
            })
        ),
        (status = 400, description = "Malformed query parameters"),
        (status = 500, description = "Internal server error"),
    )
)]
#[tracing::instrument(name = "ACCOUNT_ADDRESSES", level = "info", skip(params, config))]
/// Stake account addresses
///
/// Returns a list of addresses seen on-chain which use the specified stake key. By default returns addresses which currently control at least one UTxO, or use `include_empty` query parameter to also return addresses which don't currently control any UTxOs.
pub async fn account_addresses(
    page_params: Query<CursorPagination>,
    params: Query<Params>,
    Path(stake_addr): Path<String>,
    Extension(config): MapiExtension,
) -> Result<impl IntoResponse, ErrorResponse> {
    let dbsync = config.dbsync;
    let chain = config.chain_info;

    // -- parse endpoint specific params

    let include_empty = params.include_empty.unwrap_or(false);

    // -- parse and try decode user params

    let ParsedCursorPageParams { count, cursor } =
        utils::parse_cursor_page_params(page_params.0, decode_cursor)?;

    // --- get dbsync tip for last updated

    let last_updated = utils::get_last_updated_dbsync(&chain, &dbsync).await?;

    // --- fetch all data from db

    let row: (sqlx::types::Json<Vec<String>>,) =
        sqlx::query_as("SELECT addresses FROM grest.account_addresses($1, FALSE, $2)")
            .bind(vec![stake_addr])
            .bind(include_empty)
            .fetch_optional(&dbsync)
            .await
            .map_err(internal_server_error)?
            .unwrap_or_default();

    let mut addresses = row.0.to_vec();

    // --- cursor pagination

    addresses.sort();

    if let Some(a) = cursor {
        addresses.retain(|x| *x > a);
    }

    let next_cursor = if addresses.len() > count {
        let last_address = &addresses[count - 1];

        Some(encode_cursor(last_address))
    } else {
        None
    };

    addresses.truncate(count);

    let addresses = addresses
        .into_iter()
        .map(responses::Address)
        .collect::<Vec<Address>>();

    // ---

    let out = PaginatedResponse {
        data: addresses,
        last_updated,
        next_cursor,
    };

    Ok((StatusCode::OK, Json(out)))
}

fn decode_cursor(b64_cursor: &String) -> Result<String, ErrorResponse> {
    std::str::from_utf8(
        &b64::URL_SAFE_NO_PAD
            .decode(b64_cursor)
            .map_err(|_| bad_request("Malformed cursor"))?,
    )
    .map_err(|_| bad_request("Malformed cursor"))
    .map(|s| s.to_string())
}

fn encode_cursor(item: &String) -> String {
    b64::URL_SAFE_NO_PAD.encode(item)
}
