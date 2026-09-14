use std::{collections::HashMap, str::FromStr};

use crate::{
    responses::{AssetInPolicy, ErrorResponse, PaginatedResponse, PolicyHolderAccount},
    utils::{self, ParsedCursorPageParams},
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
    encoding::decode::{
        decode_policy_holders_cursor, decode_utxos_by_policy_value, DecodedAddress,
    },
    encoding::encode::encode_policy_holders_cursor,
};

#[utoipa::path(
    tag = "Asset Policy",
    get,
    path = "/policy/{policy}/accounts",
    params(
        ("policy" = String, Path, description = "Hex encoded Policy ID"),
        ("count" = inline(Option<CountParam>), Query, description = "The max number of results per page"),
        ("cursor" = inline(Option<String>), Query, description = "Pagination cursor string, use the cursor included in a page of results to fetch the next page"),

        ("amounts-as-strings" = Option<String>, Header, description = "Large numbers returned as strings if set to `true`")
    ),
    responses(
        (
            status = 200,
            description = "Returns stake addresses for accounts which hold assets of the given policy ID with the asset names and amounts",
            body = PaginatedPolicyHolderAccount,
            example = json!({
                "data": [{
                    "account": "stake1u83s6geern5tjrqgy6h7r99sspdnvayfjcd20k654qh737sfj90vm",
                    "assets": [{
                        "name": "647261676f6e6d",
                        "amount": 1
                    }]
                }, {
                    "account": "stake1u89727xfn46muezyut6ntj2cgmr0cy28xmjlvlec4w0qnmgyh3hy4",
                    "assets": [{
                        "name": "7468657265616c746f656b6e656573",
                        "amount": 1
                    }]
                }, {
                    "account": "stake1u8awcypj0uexy9al9msgeg5jqgsrypfcewq4hmz758mdsxsf0egkq",
                    "assets": [{
                        "name": "677579677579",
                        "amount": 1
                    }]
                }, {
                    "account": "stake1u96e4xmq9877s43meetz0tnwz37m4clv4vvrznj834dzszgkh2mj9",
                    "assets": [{
                        "name": "2e323233",
                        "amount": 1
                    }, {
                        "name": "6972732e616461",
                        "amount": 1
                    }]
                }],
                "last_updated": {
                    "block_hash": "bdf4630098ac6923e0ef1ca0b0d6e00dca96790a8c3dd670948fdfe1456798d2",
                    "block_slot": 73867237
                },
                "next_cursor": "4UJEXH0Ew5ACnrzRLfjDDQOeyeB3vhccUwWwWto"
            })
        ),
        (status = 400, description = "Malformed query parameters"),
        (status = 500, description = "Internal server error"),
    )
)]
#[tracing::instrument(
    name = "POLICY_ACCOUNTS",
    level = "info",
    skip(page_params, config, headers)
)]
/// Accounts of addresses holding assets of specific policy
///
/// Returns a list of accounts (as stake/reward addresses) associated with addresses which control some of an asset of the specified policy; in other words, instead of returning the addresses which hold the assets, the addresses are merged by their delegation part/account. Assets controlled by Byron, enterprise, or pointer addresses are omitted.
///
/// CAUTION: An asset being associated with a particular stake account does not necessarily mean the owner of that account controls the asset; use "asset addresses" unless you are sure you want to work with stake keys. Read more [here]( https://medium.com/adamant-security/multi-sig-concerns-mangled-addresses-and-the-dangers-of-using-stake-keys-in-your-cardano-project-94894319b1d8).
pub async fn policy_accounts(
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

    // --- scan all utxos for this policy ---

    let range = encoder.encode_policy_utxos_range(&policy, None, None);

    let kvs = utils::batch_scan_entire_range(&mut txn, range).await?;

    // --- build map of addresses to assets, where assets is another map of
    // asset name to amount (only assets in policy)

    let mut acc_asset_amt: HashMap<String, HashMap<Vec<u8>, u64>> = HashMap::new();

    for KvPair(_, v) in kvs {
        let (address, assets_list) = decode_utxos_by_policy_value(&v);

        let stake_addr: Option<StakeAddress> = match address {
            DecodedAddress::Address(Address::Shelley(s)) => s.try_into().ok(),
            _ => None, // omit byron + stake address
        };

        if let Some(stake) = stake_addr {
            // get the asset map for this address
            let asset_map = acc_asset_amt.entry(stake.to_bech32().unwrap()).or_default();

            // for each asset of the policy in the utxo, increment the relevant field
            // of the asset map for the address
            for (name, amount) in assets_list {
                asset_map
                    .entry(name)
                    .and_modify(|amt| *amt += amount)
                    .or_insert(amount);
            }
        }
    }

    // -- convert map into Vec of PolicyHolderAccount

    let mut holders = Vec::new();

    for (address, assets_map) in acc_asset_amt.iter() {
        let mut assets = Vec::new();

        for (asset_name, amount) in assets_map.iter() {
            assets.push(AssetInPolicy {
                name: hex::encode(asset_name),
                amount: utils::u64_str_conv(*amount, &headers),
            })
        }

        holders.push(PolicyHolderAccount {
            account: address.to_string(),
            assets,
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
