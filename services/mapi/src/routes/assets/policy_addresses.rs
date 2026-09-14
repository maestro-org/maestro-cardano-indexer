use std::{collections::HashMap, str::FromStr};

use crate::{
    responses::{AssetInPolicy, ErrorResponse, PaginatedResponse, PolicyHolder},
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
    encoding::decode::{
        decode_key, decode_policy_holders_cursor, decode_utxos_by_policy_value, DecodedAddress,
    },
    encoding::encode::encode_policy_holders_cursor,
    Key, ReducerKey,
};

#[utoipa::path(
    tag = "Asset Policy",
    get,
    path = "/policy/{policy}/addresses",
    params(
        ("policy" = String, Path, description = "Hex encoded Policy ID"),
        ("count" = inline(Option<CountParam>), Query, description = "The max number of results per page"),
        ("cursor" = inline(Option<String>), Query, description = "Pagination cursor string, use the cursor included in a page of results to fetch the next page"),

        ("amounts-as-strings" = Option<String>, Header, description = "Large numbers returned as strings if set to `true`")
    ),
    responses(
        (
            status = 200,
            description = "Returns addresses holding assets of the given policy ID with the asset names and amounts",
            body = PaginatedPolicyHolder,
            example = json!({
                "data": [
                    {
                        "address": "addr_test1vpxa84r650wsmcm887kkwar5y7ek5yyrjvgfqd8a608s2vsdnutyy",
                        "assets": [
                            {
                                "name": "6362544843",
                                "amount": 100000000000000i64
                            }
                        ]
                    },
                    {
                        "address": "addr_test1vrjpwhg7vlhck0ph5q0dpnkqferx3ltf0aqvy4qzn9m5v2gcvqhm8",
                        "assets": [
                            {
                                "name": "6362544843",
                                "amount": 320000000000000i64
                            }
                        ]
                    }
                ],
                "last_updated": {
                    "timestamp": "2022-10-10 20:25:28",
                    "block_hash": "2863d3805fa06b0b8062df9c1cb62773e0961f8333ef2cd7ea3faca0f3c738a7",
                    "block_slot": 32277823
                },
                "next_cursor": null
            })
        ),
        (status = 400, description = "Malformed query parameters"),
        (status = 500, description = "Internal server error"),
    )
)]
#[tracing::instrument(
    name = "POLICY_ADDRESSES",
    level = "info",
    skip(page_params, config, headers)
)]
/// Addresses holding assets of specific policy
///
/// Returns a list of addresses which hold some of an asset of the specified policy ID
pub async fn policy_addresses(
    headers: HeaderMap,
    page_params: Query<CursorPagination>,
    Path(policy): Path<String>,
    Extension(config): MapiExtension,
) -> Result<impl IntoResponse, ErrorResponse> {
    let chain = config.chain_info;
    let polyphony = config.polyphony_wrapper;
    let encoder = polyphony.utxos_by_policy_encoder()?;

    // -- parse and try decode user params

    let policy = utils::decode_policy_id(&policy)?;

    let ParsedCursorPageParams { count, cursor } =
        utils::parse_cursor_page_params(page_params.0, decode_policy_holders_cursor)?;

    // --- start db at most recent timestamp

    let mut txn = polyphony.begin_snapshot_latest().await?;

    // --- fetch cursor for last updated ---

    let last_updated = utils::get_last_updated(&mut txn, &encoder, &chain).await?;

    // --- scan all utxos for this policy

    let range = encoder.encode_policy_utxos_range(&policy, None, None);

    let kvs = utils::batch_scan_entire_range(&mut txn, range).await?;

    // --- build map of addresses to assets, where assets is another map of
    // asset name to amount (only assets in policy)

    let mut addr_asset_amt: HashMap<String, HashMap<Vec<u8>, u64>> = HashMap::new();

    for KvPair(k, v) in kvs {
        let decoded_key = decode_key(&Into::<Vec<u8>>::into(k));

        // TODO cleanup
        if let Key::Reducer((_, _, ReducerKey::PolicyUtxos(_))) = decoded_key {
        } else {
            return Err(internal_server_error_str(
                "unexpected key scanned: {decoded:?}",
            ));
        };

        let (address, assets_list) = decode_utxos_by_policy_value(&v);

        let address = match address {
            DecodedAddress::Address(a) => a,
            // TODO
            DecodedAddress::Hash(h) => {
                return Err(internal_server_error_str(
                    format!("decoded address hash: {}", hex::encode(h)).as_str(),
                ))
            }
        };

        // get the asset map for this address

        let asset_map = addr_asset_amt.entry(address.to_string()).or_default();

        // for each asset of the policy in the utxo, increment the relevant field
        // of the asset map for the address

        for (name, amount) in assets_list {
            asset_map
                .entry(name)
                .and_modify(|amt| *amt += amount)
                .or_insert(amount);
        }
    }

    // -- convert map into Vec of PolicyHolder

    let mut holders = Vec::new();

    for (address, assets_map) in addr_asset_amt.iter() {
        let mut assets = Vec::new();

        for (asset_name, amount) in assets_map.iter() {
            assets.push(AssetInPolicy {
                name: hex::encode(asset_name),
                amount: utils::u64_str_conv(*amount, &headers),
            })
        }

        holders.push(PolicyHolder {
            address: address.to_string(),
            assets,
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
