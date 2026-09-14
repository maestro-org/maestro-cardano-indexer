use std::collections::HashSet;

use axum::{extract::Path, http::StatusCode, response::IntoResponse, Extension, Json};
use pallas::ledger::addresses::{Address, ShelleyDelegationPart};
use tikv_client::KvPair;
use timbre::encoding::{
    decode::{decode_utxos_by_policy_value, DecodedAddress},
    *,
};

use crate::{
    responses::{
        ErrorResponse, Holders, PolicyInfo, Script, TimestampedResponse, TimestampedTransaction,
    },
    utils::{self, internal_server_error, internal_server_error_str, not_found},
    MapiExtension,
};

#[utoipa::path(
    tag = "Asset Policy",
    get,
    path = "/policy/{policy}",
    params(
        ("policy" = String, Path, description = "Hex encoded policy ID"),
    ),
    responses(
        (
            status = 200,
            description = "Summary of asset minting policy",
            body = TimestampedPolicyInfo,
            example = json!({
                "data": {
                    "policy_id": "f0ff48bbb7bbe9d59a40f1ce90e9e9d0ff5002ec48f232b49ca0fb9a",
                    "script": {
                        "hash": "f0ff48bbb7bbe9d59a40f1ce90e9e9d0ff5002ec48f232b49ca0fb9a",
                        "type": "native",
                        "bytes": "8200581c4da965a049dfd15ed1ee19fba6e2974a0b79fc416dd1796a1f97f5e1",
                        "json": {
                            "keyHash": "4da965a049dfd15ed1ee19fba6e2974a0b79fc416dd1796a1f97f5e1",
                            "type": "sig"
                        }
                    },
                    "assets_of_policy": 11,
                    "total_supply": "11",
                    "unique_holders": {
                        "by_address": 21,
                        "by_account": 19
                    },
                    "first_mint_tx": {
                        "tx_hash": "257916e7ae112cf16f27218e41bfa37c018bab922354201b3b38c9a24c35ab33",
                        "slot": 55718892,
                        "timestamp": "2022-03-14 19:13:03"
                    },
                    "latest_update_tx": {
                        "tx_hash": "257916e7ae112cf16f27218e41bfa37c018bab922354201b3b38c9a24c35ab33",
                        "slot": 55718892,
                        "timestamp": "2022-03-14 19:13:03"
                    }
                },
                "last_updated": {
                    "timestamp": "2023-10-18 07:30:52",
                    "block_hash": "0e1e924710135acfe200ab13d290bd282a67584fd54456f0dcac0aeaa38bb2c2",
                    "block_slot": 106047961
                }
            })
        ),
        (status = 400, description = "Malformed query parameters"),
        (status = 404, description = "No results found"),
        (status = 500, description = "Internal server error"),
    )
)]
#[tracing::instrument(name = "POLICY_INFO", level = "info", skip(config))]
/// Information about a policy of native assets
///
/// Returns a summary of information about a native asset policy ID and assets minted under that policy
pub async fn policy_info(
    Path(policy): Path<String>,
    Extension(config): MapiExtension,
) -> Result<impl IntoResponse, ErrorResponse> {
    let chain = config.chain_info;
    let polyphony = config.polyphony_wrapper;
    let encoder = polyphony.supply_by_asset_encoder()?;

    // -- parse and try decode user params

    let policy = utils::decode_policy_id(&policy)?;

    // --- start db at most recent timestamp

    let mut txn = polyphony.begin_snapshot_latest().await?;

    // --- fetch cursor for last updated ---

    let last_updated = utils::get_last_updated(&mut txn, &encoder, &chain).await?;

    // -- fetch script which backs the policy ID

    let script = utils::fetch_script(&mut txn, &polyphony, &chain, *policy).await?;

    let script = Script {
        hash: script.hash,
        script_type: script.script_type,
        bytes: script.bytes,
        json: script.json,
    };

    // -- fetch all assets in the policy, sum all the kinds and all the supplies

    let assets_range = encoder.encode_supply_by_asset_range(&policy, None, None);

    let assets = utils::batch_scan_entire_range(&mut txn, assets_range).await?;

    let assets_of_policy = assets.len() as u64;

    let mut total_supply = 0u128;

    for KvPair(_, v) in assets {
        let supply: [u8; 16] = v[0..16]
            .try_into()
            .map_err(|_| internal_server_error_str("malformed block height"))?;

        total_supply += u128::from_be_bytes(supply);
    }

    // -- fetch all the utxos for the policy, sum unique addresses and accounts

    let utxos_range = polyphony
        .utxos_by_policy_encoder()?
        .encode_policy_utxos_range(&policy, None, None);

    let utxos = utils::batch_scan_entire_range(&mut txn, utxos_range).await?;

    let mut addresses = HashSet::new();
    let mut accounts = HashSet::new();

    for KvPair(_, v) in utxos {
        let (address, _) = decode_utxos_by_policy_value(&v);

        // addresses
        addresses.insert(address.clone());

        // accounts
        // omit byron + stake address
        if let DecodedAddress::Address(Address::Shelley(s)) = address {
            // omit null + pointer
            if let d @ (ShelleyDelegationPart::Key(_) | ShelleyDelegationPart::Script(_)) =
                s.delegation()
            {
                accounts.insert(d.clone());
            }
        }
    }

    let unique_holders = Holders {
        by_address: addresses.len() as u64,
        by_account: accounts.len() as u64,
    };

    // -- fetch the most recent metadata for empty asset for the policy

    // let metadata_range = polyphony.mint_metadata_by_asset().encode_mint_metadata_by_asset_range(&policy, vec![], None, None);

    // let metadata = txn
    //     .scan_reverse(metadata_range, 1)
    //     .await
    //     .map_err(internal_server_error)?
    //     .collect::<Vec<KvPair>>().first().map(|KvPair(_, v)| decode_mint_metadata_by_asset_value(v).2);

    // TODO: parse cip27 from metadata

    // -- first mint tx and last mint tx

    let updates_range = polyphony
        .updates_by_policy_encoder()?
        .encode_updates_by_policy_range(&policy, None, None);

    let KvPair(first_update_k, first_update_v) = txn
        .scan(updates_range.clone(), 1)
        .await
        .map_err(internal_server_error)?
        .next()
        .ok_or_else(not_found)?;

    let first_mint_tx = {
        let first_update_k = Into::<Vec<u8>>::into(first_update_k);

        let UpdatesByPolicyKey { slot, .. } = decode_updates_by_policy_key(&first_update_k);

        let tx_hash = decode_updates_by_policy_value(&first_update_v).0;

        TimestampedTransaction {
            tx_hash: hex::encode(tx_hash),
            slot,
            timestamp: chain.slot_to_utc(slot),
        }
    };

    let KvPair(last_update_k, last_update_v) = txn
        .scan_reverse(updates_range, 1)
        .await
        .map_err(internal_server_error)?
        .next()
        .ok_or_else(not_found)?;

    let latest_mint_tx = {
        let UpdatesByPolicyKey { slot, .. } =
            decode_updates_by_policy_key(&Into::<Vec<u8>>::into(last_update_k));

        let tx_hash = decode_updates_by_policy_value(&last_update_v).0;

        TimestampedTransaction {
            tx_hash: hex::encode(tx_hash),
            slot,
            timestamp: chain.slot_to_utc(slot),
        }
    };

    // --

    let info = PolicyInfo {
        policy_id: hex::encode(policy),
        script,
        assets_of_policy,
        total_supply: total_supply.to_string(),
        unique_holders,
        // asset_standards: PolicyStandards {cip27_metadata: None}, // TODO
        first_mint_tx,
        latest_mint_tx,
    };

    // ---

    let out = TimestampedResponse {
        data: info,
        last_updated,
    };

    Ok((StatusCode::OK, Json(out)))
}
