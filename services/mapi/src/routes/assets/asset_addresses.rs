use std::{collections::HashMap, str::FromStr};

use crate::{
    responses::{AssetHolder, ErrorResponse, PaginatedResponse},
    utils::{self, internal_server_error_str, ParsedCursorPageParams},
    CountParam, CursorPagination, MapiExtension,
};
use axum::{
    extract::{Path, Query},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Extension, Json,
};
use pallas::ledger::addresses::Address;
use tikv_client::KvPair;
use timbre::{
    encoding::decode::{decode_policy_holders_cursor, decode_utxos_by_asset_value, DecodedAddress},
    encoding::encode::encode_policy_holders_cursor,
};

#[utoipa::path(
    tag = "Assets",
    get,
    path = "/assets/{asset}/addresses",
    params(
        ("asset" = String, Path, description = "Asset, encoded as concatenation of hex of policy ID and asset name"),
        ("count" = inline(Option<CountParam>), Query, description = "The max number of results per page"),
        ("cursor" = inline(Option<String>), Query, description = "Pagination cursor string, use the cursor included in a page of results to fetch the next page"),

        ("amounts-as-strings" = Option<String>, Header, description = "Large numbers returned as strings if set to `true`")
    ),
    responses(
        (
            status = 200,
            description = "Returns addresses holding the specified asset, paired with the amounts of the asset owned",
            body = PaginatedAssetHolder,
            example = json!({
                "data": [
                    {
                        "address": "addr_test1vpxa84r650wsmcm887kkwar5y7ek5yyrjvgfqd8a608s2vsdnutyy",
                        "amount": 100000000000000i64
                    },
                    {
                        "address": "addr_test1vrjpwhg7vlhck0ph5q0dpnkqferx3ltf0aqvy4qzn9m5v2gcvqhm8",
                        "amount": 320000000000000i64
                    }
                ],
                "last_updated": {
                    "timestamp": "2022-10-10 20:25:28",
                    "block_hash": "c0d2cf4d7e0e9dfbfda2e09330a8aa2dd426a0d9c2d927ea28e43aa0b6550f8f",
                    "block_slot": 32266746
                },
                "next_cursor": null
            })
        ),
        (status = 400, description = "Malformed query parameters"),
        (status = 500, description = "Internal server error"),
    )
)]
#[tracing::instrument(
    name = "ASSET_ADDRESSES",
    level = "info",
    skip(page_params, config, headers)
)]
/// Native asset addresses
///
/// Returns a list of addresses which control some amount of the specified asset
pub async fn asset_addresses(
    page_params: Query<CursorPagination>,
    headers: HeaderMap,
    Path(asset): Path<String>,
    Extension(config): MapiExtension,
) -> Result<impl IntoResponse, ErrorResponse> {
    let chain = config.chain_info;
    let polyphony = config.polyphony_wrapper;

    let encoder = polyphony.utxos_by_asset_encoder()?;

    // -- parse and try decode user params

    let (policy, asset_name) = utils::decode_asset(&asset)?;

    let ParsedCursorPageParams { count, cursor } =
        utils::parse_cursor_page_params(page_params.0, decode_policy_holders_cursor)?;

    // --- start db at most recent timestamp

    let mut txn = polyphony.begin_snapshot_latest().await?;

    // --- fetch cursor for last updated ---

    let last_updated = utils::get_last_updated(&mut txn, &encoder, &chain).await?;

    // --- scan all utxos for this policy

    let range = encoder.encode_asset_utxos_range(&policy, &asset_name, None, None);

    let kvs = utils::batch_scan_entire_range(&mut txn, range).await?;

    // --- build map of addresses to amounts of the asset

    let mut addr_amt_map: HashMap<String, u64> = HashMap::new();

    for KvPair(_, v) in kvs {
        let (address, amount) = decode_utxos_by_asset_value(&v);

        let address = match address {
            DecodedAddress::Address(a) => a.to_string(),
            // TODO
            DecodedAddress::Hash(h) => {
                return Err(internal_server_error_str(
                    format!("decoded address hash: {}", hex::encode(h)).as_str(),
                ))
            }
        };

        addr_amt_map
            .entry(address.clone())
            .and_modify(|amt| *amt += amount)
            .or_insert(amount);
    }

    // -- convert map into Vec of PolicyHolder

    let mut holders = Vec::new();

    for (address, amount) in addr_amt_map.into_iter() {
        holders.push(AssetHolder {
            address: address.to_string(),
            amount: utils::u64_str_conv(amount, &headers),
        })
    }

    holders.sort_by(|a, b| a.address.cmp(&b.address));

    if let Some(a) = cursor {
        holders.retain(|x| x.address > a.to_string());
    }

    let next_cursor = if holders.len() > count {
        let last_address = Address::from_str(&holders[count - 1].address).unwrap();

        Some(encode_policy_holders_cursor(&last_address))
    } else {
        None
    };

    // only return only the page size of results
    holders.truncate(count);

    let out = PaginatedResponse {
        data: holders,
        last_updated,
        next_cursor,
    };

    Ok((StatusCode::OK, Json(out)))
}
