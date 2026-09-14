use crate::responses::{Asset, ErrorResponse, PaginatedResponse};
use crate::utils::{self, bad_request, internal_server_error, ParsedCursorPageParams};
use crate::{CountParam, CursorPagination, MapiExtension};
use axum::http::HeaderMap;
use axum::{
    extract::{Path, Query},
    http::StatusCode,
    response::IntoResponse,
    Extension, Json,
};
use base64::{engine::general_purpose as b64, Engine};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Deserialize)]
pub struct Params {
    policy: Option<String>,
}

#[derive(Clone, Deserialize, Serialize, FromRow)]
struct KoiosAssetListEntry {
    pub policy_id: String,
    pub asset_name: String,
    #[sqlx(rename = "quantity")]
    pub amount: String,
}

#[utoipa::path(
    tag = "Accounts",
    get,
    path = "/accounts/{stake_addr}/assets",
    params(
        ("stake_addr" = String, Path, description = "Bech32 encoded reward/stake address ('stake1...')"),

        ("policy" = Option<String>, Query, description = "Filter results to only show assets of the specified policy"),

        ("count" = inline(Option<CountParam>), Query, description = "The max number of results per page"),
        ("cursor" = inline(Option<String>), Query, description = "Pagination cursor string, use the cursor included in a page of results to fetch the next page"),

        ("amounts-as-strings" = Option<String>, Header, description = "Large numbers returned as strings if set to `true`")
    ),
    responses(
        (
            status = 200,
            description = "A list of assets which are owned by addresses that use the specified stake key",
            body = PaginatedAsset,
            example = json!({
                "data": [
                    {
                        "unit": "01a67bf1c1b29912b17d53a31fa4b650da2dd136ff24ecc84f452ff047616d654368616e6765724e4654202331",
                        "amount": 5
                    },
                    {
                        "unit": "01a67bf1c1b29912b17d53a31fa4b650da2dd136ff24ecc84f452ff047616d654368616e6765724e4654202332",
                        "amount": 6
                    },
                    {
                        "unit": "01a67bf1c1b29912b17d53a31fa4b650da2dd136ff24ecc84f452ff047616d654368616e6765724e4654202333",
                        "amount": 7
                    },
                    {
                        "unit": "2f7e0f588efe4e2836c829c50619a498ddf5521255bf5e1e9f9f5a1346616b65555344",
                        "amount": 864
                    },
                    {
                        "unit": "833a41706977b6c54edba264a7ea9dc4390728a05c69cb53d8081df147616d654368616e676572546f6b656ef09f9a80",
                        "amount": 2
                    },
                    {
                        "unit": "cc5827d01be77807a48d7e3b70a7c32667722bce7957a61ca52f4e0d47616d654368616e676572546f6b656ef09f9a80",
                        "amount": 70
                    }
                ],
                "last_updated": {
                    "timestamp": "2022-10-10 20:25:28",
                    "block_hash": "27b0866a073a5068bd6a24a009beba68f8951c6bf2ae4c72a734c8566ecabfc2",
                    "block_slot": 32208940
                },
                "next_cursor": null
            })
        ),
        (status = 400, description = "Malformed query parameters"),
        (status = 500, description = "Internal server error"),
    )
)]
/// Stake account assets
///
/// Returns a list of native assets which are owned by addresses with the specified stake key
#[tracing::instrument(name = "ACCOUNT_ASSETS", level = "info", skip(config, headers))]
pub async fn account_assets(
    headers: HeaderMap,
    page_params: Query<CursorPagination>,
    params: Query<Params>,
    Path(stake_addr): Path<String>,
    Extension(config): MapiExtension,
) -> Result<impl IntoResponse, ErrorResponse> {
    let dbsync = config.dbsync;
    let chain = config.chain_info;

    // -- parse and try decode user params

    if let Some(p) = &params.policy {
        utils::decode_policy_id(p)?;
    }

    let ParsedCursorPageParams { count, cursor } =
        utils::parse_cursor_page_params(page_params.0, decode_cursor)?;

    // --- get dbsync tip for last updated

    let last_updated = utils::get_last_updated_dbsync(&chain, &dbsync).await?;

    // --- fetch all data from db

    let rows: Vec<KoiosAssetListEntry> = sqlx::query_as("SELECT * FROM grest.account_assets($1)")
        .bind(vec![stake_addr])
        .fetch_all(&dbsync)
        .await
        .map_err(internal_server_error)?;

    // --- transform data in mapi structure

    let mut assets = Vec::new();

    if let Some(p) = params.0.policy {
        for asset in rows {
            if asset.policy_id == p {
                assets.push(Asset {
                    unit: format!("{}{}", asset.policy_id, asset.asset_name),
                    amount: utils::u128_str_conv(asset.amount.parse().unwrap(), &headers),
                })
            }
        }
    } else {
        for asset in rows {
            assets.push(Asset {
                unit: format!("{}{}", asset.policy_id, asset.asset_name),
                amount: utils::u128_str_conv(asset.amount.parse().unwrap(), &headers),
            })
        }
    }

    // --- cursor pagination

    assets.sort_by(|a, b| a.unit.cmp(&b.unit));

    if let Some(a) = cursor {
        assets.retain(|x| x.unit > a);
    }

    let next_cursor = if assets.len() > count {
        let last_asset = &assets[count - 1].unit;

        Some(encode_cursor(last_asset))
    } else {
        None
    };

    assets.truncate(count);

    // ---

    let out = PaginatedResponse {
        data: assets,
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
