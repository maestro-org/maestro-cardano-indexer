use std::{collections::HashMap, str::FromStr};

use crate::{
    responses::{AssetHolderAccount, ErrorResponse, PaginatedResponse},
    utils::{self, internal_server_error_str, ParsedCursorPageParams},
    CountParam, CursorPagination, MapiExtension,
};
use axum::{
    extract::{Path, Query},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Extension, Json,
};
use pallas::ledger::addresses::{Address, StakeAddress};
use tikv_client::KvPair;
use timbre::{
    encoding::decode::{decode_policy_holders_cursor, decode_utxos_by_asset_value, DecodedAddress},
    encoding::encode::encode_policy_holders_cursor,
};

#[utoipa::path(
    tag = "Assets",
    get,
    path = "/assets/{asset}/accounts",
    params(
        ("asset" = String, Path, description = "Asset, encoded as concatenation of hex of policy ID and asset name"),
        ("count" = inline(Option<CountParam>), Query, description = "The max number of results per page"),
        ("cursor" = inline(Option<String>), Query, description = "Pagination cursor string, use the cursor included in a page of results to fetch the next page"),

        ("amounts-as-strings" = Option<String>, Header, description = "Large numbers returned as strings if set to `true`")
    ),
    responses(
        (
            status = 200,
            description = "Returns stake addresses for accounts which control some of the specified asset",
            body = PaginatedAssetHolderAccount,
            example = json!({
                "data": [{
                    "account": "stake1uy3c9n7hlnvt2eucfpgd8ezqn424na7nhc6zca2atq0tqsg5ljq5v",
                    "amount": 1
                },
                {
                    "account": "stake1uylx7yct3smspkes9fzk3f8y8mmu90htxe75g923576d2zgpluxt7",
                    "amount": 3
                }],
                "last_updated": {
                    "block_hash": "bdf4630098ac6923e0ef1ca0b0d6e00dca96790a8c3dd670948fdfe1456798d2",
                    "block_slot": 73867237
                },
                "next_cursor": null
            })
        ),
        (status = 400, description = "Malformed query parameters"),
        (status = 500, description = "Internal server error"),
    )
)]
#[tracing::instrument(name = "ASSET_ACCOUNTS", level = "info", skip(config, headers))]
/// Accounts of addresses holding specific asset
///
/// Returns a list of accounts (as stake/reward addresses) associated with addresses which control some of the specified asset; in other words, instead of returning the addresses which hold some of the asset, the addresses are merged by their delegation part/account. Assets controlled by Byron, enterprise, or pointer addresses are omitted.
///
/// CAUTION: An asset being associated with a particular stake account does not necessarily mean the owner of that account controls the asset; use "asset addresses" unless you are sure you want to work with stake keys. Read more [here]( https://medium.com/adamant-security/multi-sig-concerns-mangled-addresses-and-the-dangers-of-using-stake-keys-in-your-cardano-project-94894319b1d8).
pub async fn asset_accounts(
    headers: HeaderMap,
    page_params: Query<CursorPagination>,
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

    // --- build map of stake addresses to amounts of the asset

    let mut acc_amt_map: HashMap<String, u64> = HashMap::new();

    for KvPair(_, v) in kvs {
        let (address, amount) = decode_utxos_by_asset_value(&v);

        let stake_addr: Option<StakeAddress> = match address {
            DecodedAddress::Address(Address::Shelley(s)) => s.try_into().ok(),
            DecodedAddress::Hash(h) => {
                return Err(internal_server_error_str(
                    format!("decoded address hash: {}", hex::encode(h)).as_str(),
                ))
            }
            _ => None, // omit byron + stake address
        };

        if let Some(stake) = stake_addr {
            acc_amt_map
                .entry(stake.to_bech32().unwrap())
                .and_modify(|amt| *amt += amount)
                .or_insert(amount);
        }
    }

    // -- convert map into Vec of PolicyHolder

    let mut holders = Vec::new();

    for (account, amount) in acc_amt_map.into_iter() {
        holders.push(AssetHolderAccount {
            account,
            amount: utils::u64_str_conv(amount, &headers),
        })
    }

    holders.sort_by(|a, b| a.account.cmp(&b.account));

    if let Some(a) = cursor {
        holders.retain(|x| x.account > a.to_string());
    }

    let next_cursor = if holders.len() > count {
        let last_address = Address::from_str(&holders[count - 1].account).unwrap();

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
