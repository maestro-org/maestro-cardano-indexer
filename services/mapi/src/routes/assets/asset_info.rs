use axum::{extract::Path, http::StatusCode, response::IntoResponse, Extension, Json};
use itertools::Itertools;
use pallas::{
    codec::utils::KeyValuePairs,
    ledger::{
        addresses::{Address, ShelleyDelegationPart},
        primitives::{babbage::Metadatum, Fragment},
    },
};
use serde_json::{Map, Value};
use std::{cmp::Ordering, collections::HashSet};
use tikv_client::KvPair;
use timbre::encoding::{
    decode::{decode_utxos_by_asset_value, DecodedAddress},
    decode_mint_metadata_by_asset_value, decode_updates_by_policy_key,
    decode_updates_by_policy_value, UpdatesByPolicyKey,
};

use crate::{
    responses::{
        AssetInfo, AssetStandards, ErrorResponse, Holders, MintTransaction, TimestampedResponse,
        TokenRegistryMetadata,
    },
    utils::{self, internal_server_error, internal_server_error_str, not_found, scan_extended},
    MapiExtension, OrderParam,
};

#[utoipa::path(
    tag = "Assets",
    get,
    path = "/assets/{asset}",
    params(
        ("asset" = String, Path, description = "Asset, encoded as concatenation of hex of policy ID and asset name"),
    ),
    responses(
        (
            status = 200,
            description = "Information about the asset",
            body = TimestampedAssetInfo,
            examples(
                ("CIP-25" = (
                    summary = "Asset with CIP-25 metadata",
                    value = json!({
                        "data": {
                            "asset_name": "6164616d616e74",
                            "asset_name_ascii": "adamant",
                            "fingerprint": "asset105lxc60yjpqygjsnn29e5hjyafnfw75pqwwza2",
                            "total_supply": "1",
                            "unique_holders": {
                                "by_address": 1,
                                "by_account": 0
                            },
                            "first_mint_tx": {
                                "tx_hash": "257916e7ae112cf16f27218e41bfa37c018bab922354201b3b38c9a24c35ab33",
                                "slot": 55718892,
                                "timestamp": "2022-03-14 19:13:03",
                                "amount": "1"
                            },
                            "latest_mint_tx": {
                                "tx_hash": "257916e7ae112cf16f27218e41bfa37c018bab922354201b3b38c9a24c35ab33",
                                "slot": 55718892,
                                "timestamp": "2022-03-14 19:13:03",
                                "amount": "1"
                            },
                            "mint_tx_count": 1,
                            "burn_tx_count": 0,
                            "asset_standards": {
                                "cip25_metadata": {
                                    "name": "$adamant",
                                    "description": "The Handle Standard",
                                    "website": "https://adahandle.com",
                                    "image": "ipfs://Qmai8iwE1Diw5YZVYSpSPAbXM5AVprheve8AAXJzwXoftf",
                                    "core": {
                                        "og": 0,
                                        "termsofuse": "https://adahandle.com/tou",
                                        "handleEncoding": "utf-8",
                                        "prefix": "$",
                                        "version": 0
                                    },
                                    "augmentations": []
                                },
                                "cip68_metadata": null
                            },
                            "latest_mint_tx_metadata": {
                                "721": {
                                    "f0ff48bbb7bbe9d59a40f1ce90e9e9d0ff5002ec48f232b49ca0fb9a": {
                                        "cardanosweets": {
                                            "name": "$cardanosweets",
                                            "description": "The Handle Standard",
                                            "website": "https://adahandle.com",
                                            "image": "ipfs://QmaNoVpks3gAR4oayMkx9uHU2baDMGQrBAzpM1PZ3ebJBu",
                                            "core": {
                                                "og": 0,
                                                "termsofuse": "https://adahandle.com/tou",
                                                "handleEncoding": "utf-8",
                                                "prefix": "$",
                                                "version": 0
                                            },
                                            "augmentations": []
                                        },
                                        "adamant": {
                                            "name": "$adamant",
                                            "description": "The Handle Standard",
                                            "website": "https://adahandle.com",
                                            "image": "ipfs://Qmai8iwE1Diw5YZVYSpSPAbXM5AVprheve8AAXJzwXoftf",
                                            "core": {
                                                "og": 0,
                                                "termsofuse": "https://adahandle.com/tou",
                                                "handleEncoding": "utf-8",
                                                "prefix": "$",
                                                "version": 0
                                            },
                                            "augmentations": []
                                        }
                                    }
                                }
                            },
                            "token_registry_metadata": null
                        },
                        "last_updated": {
                            "timestamp": "2023-10-18 07:30:52",
                            "block_hash": "0e1e924710135acfe200ab13d290bd282a67584fd54456f0dcac0aeaa38bb2c2",
                            "block_slot": 106047961
                        }
                    })
                )),
                // TODO: CIP68 example
                // TODO: Token registry example
            )
        ),
        (status = 400, description = "Malformed query parameters"),
        (status = 404, description = "No results found"),
        (status = 500, description = "Internal server error"),
    )
)]
#[tracing::instrument(name = "ASSET_INFO", level = "info", skip(config))]
/// Native asset information
///
/// Return a summary of information about an asset
pub async fn asset_info(
    Path(asset): Path<String>,
    Extension(config): MapiExtension,
) -> Result<impl IntoResponse, ErrorResponse> {
    let chain = config.chain_info;
    let polyphony = config.polyphony_wrapper;
    let timeout = config.timeout_duration;

    // -- parse and try decode user params

    let (policy, name) = utils::decode_asset(&asset)?;

    let name_vec = name.to_vec();

    // --- start db at most recent timestamp

    let mut txn = polyphony.begin_snapshot_latest().await?;

    // --- fetch cursor for last updated ---

    let last_updated =
        utils::get_last_updated(&mut txn, &polyphony.supply_by_asset_encoder()?, &chain).await?;

    // ---

    let asset_name = hex::encode(&name_vec);
    let asset_name_ascii = String::from_utf8_lossy(&name).to_string();
    let fingerprint = utils::asset_fingerprint(*policy, &name);

    // --- supply

    let supply_key = polyphony
        .supply_by_asset_encoder()?
        .encode_supply_by_asset_key(&policy, name_vec.clone());

    let supply_bytes: [u8; 16] = txn
        .get(supply_key)
        .await
        .map_err(internal_server_error)?
        .ok_or_else(not_found)?
        .try_into()
        .map_err(|_| internal_server_error_str("malformed asset supply"))?;

    let total_supply = u128::from_be_bytes(supply_bytes).to_string();

    // --- holders

    let utxos_range = polyphony
        .utxos_by_asset_encoder()?
        .encode_asset_utxos_range(&policy, &name, None, None);

    let utxos = utils::batch_scan_entire_range(&mut txn, utxos_range).await?;

    let mut addresses = HashSet::new();
    let mut accounts = HashSet::new();

    for KvPair(_, v) in utxos {
        let (address, _) = decode_utxos_by_asset_value(&v);

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
        };
    }

    let unique_holders = Holders {
        by_address: addresses.len() as u64,
        by_account: accounts.len() as u64,
    };

    // --- updates

    let updates_range = polyphony
        .updates_by_policy_encoder()?
        .encode_updates_by_policy_range(&policy, None, None);

    let update_kvs = scan_extended(
        &mut txn,
        updates_range,
        OrderParam::Asc,
        Some(|KvPair(_, v)| policy_update_contains_asset(v, &name_vec)),
        None,
        Some(timeout),
    )
    .await?;

    let KvPair(first_update_k, first_update_v) = update_kvs.first().unwrap().clone();

    let first_mint_tx = {
        let UpdatesByPolicyKey { slot, .. } =
            decode_updates_by_policy_key(&Into::<Vec<u8>>::into(first_update_k));

        let (tx_hash, assets) = decode_updates_by_policy_value(&first_update_v);

        let amount = assets
            .into_iter()
            .find(|(n, _)| *n == name_vec)
            .map(|(_, x)| x)
            .ok_or_else(|| internal_server_error_str("mint tx contents error"))?;

        MintTransaction {
            tx_hash: hex::encode(tx_hash),
            slot,
            timestamp: chain.slot_to_utc(slot),
            amount: amount.to_string(),
        }
    };

    let KvPair(last_update_k, last_update_v) = update_kvs.last().unwrap().clone();

    let latest_mint_tx = {
        let UpdatesByPolicyKey { slot, .. } =
            decode_updates_by_policy_key(&Into::<Vec<u8>>::into(last_update_k));

        let (tx_hash, assets) = decode_updates_by_policy_value(&last_update_v);

        let amount = assets
            .into_iter()
            .find(|(n, _)| *n == name_vec)
            .map(|(_, x)| x)
            .ok_or_else(|| internal_server_error_str("mint tx contents error"))?;

        MintTransaction {
            tx_hash: hex::encode(tx_hash),
            slot,
            timestamp: chain.slot_to_utc(slot),
            amount: amount.to_string(),
        }
    };

    let mut mint_tx_count = 0;
    let mut burn_tx_count = 0;

    for KvPair(_, update_v) in update_kvs {
        for (policy_asset, amount) in decode_updates_by_policy_value(&update_v).1 {
            if policy_asset == *name {
                match amount.cmp(&0) {
                    Ordering::Greater => mint_tx_count += 1,
                    Ordering::Less => burn_tx_count += 1,
                    Ordering::Equal => (),
                }
                break;
            }
        }
    }

    // --- standards

    let cip25_metadata =
        utils::fetch_cip25_metadata(&mut txn, &polyphony, vec![(*policy, name.to_vec())])
            .await?
            .into_values()
            .next();

    let cip68_metadata =
        utils::try_fetch_cip68_metadata(&polyphony, *policy, name.to_vec()).await?;

    let asset_standards = AssetStandards {
        cip25_metadata,
        cip68_metadata,
    };

    // --- last mint metadata

    let metadata_range = polyphony
        .mint_metadata_by_asset_encoder()?
        .encode_mint_metadata_by_asset_range(&policy, name_vec.clone(), None, None);

    let metadata_cbor = txn
        .scan_reverse(metadata_range, 1)
        .await
        .map_err(internal_server_error)?
        .collect::<Vec<KvPair>>()
        .first()
        .map(|KvPair(_, v)| decode_mint_metadata_by_asset_value(v).2);

    let latest_mint_tx_metadata = match metadata_cbor {
        Some(b) => {
            let kvs = KeyValuePairs::<u64, Metadatum>::decode_fragment(&b)
                .map_err(|_| internal_server_error_str("malformed metadata cbor"))?;

            let mut out_json = Map::new();

            for (tag, metadatum) in kvs.to_vec() {
                let k_string = tag.to_string();

                if let Some(v_json) = utils::metadatum_to_json(metadatum) {
                    out_json.insert(k_string, v_json);
                }
            }

            Some(Value::Object(out_json))
        }
        None => None,
    };

    // --- registry

    // The token registry keyspace is populated by an external ingestor. When its
    // IDs are not configured, token-registry lookups are disabled and this field
    // is served as `null`.
    let token_registry_metadata = match polyphony.token_registry_encoder() {
        Some(encoder) => {
            let registry_key = encoder.encode_token_registry_by_asset_key(&policy, name_vec);

            match txn.get(registry_key).await.map_err(internal_server_error)? {
                Some(b) => {
                    let trm: TokenRegistryMetadata =
                        serde_json::from_slice(&b).map_err(internal_server_error)?;

                    Some(trm)
                }
                None => None,
            }
        }
        None => None,
    };

    // ---

    let info = AssetInfo {
        asset_name,
        asset_name_ascii,
        fingerprint,
        total_supply,
        unique_holders,
        first_mint_tx,
        latest_mint_tx,
        mint_tx_count,
        burn_tx_count,
        asset_standards,
        latest_mint_tx_metadata,
        token_registry_metadata,
    };

    // ---

    let out = TimestampedResponse {
        data: info,
        last_updated,
    };

    Ok((StatusCode::OK, Json(out)))
}

fn policy_update_contains_asset(v: Vec<u8>, asset_name: &Vec<u8>) -> bool {
    let (_, assets) = decode_updates_by_policy_value(&v);

    assets
        .into_iter()
        .map(|(name, _)| name)
        .contains(asset_name)
}
