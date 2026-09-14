use std::collections::HashMap;

use crate::{
    responses::{Balance, ErrorResponse, TimestampedResponse},
    utils::{self, internal_server_error, scan_extended},
    MapiExtension, OrderParam,
};
use axum::{extract::Path, http::StatusCode, response::IntoResponse, Extension, Json};
use pallas::ledger::traverse::{Era, MultiEraOutput};
use tikv_client::KvPair;

#[utoipa::path(
    tag = "Addresses",
    get,
    path = "/addresses/cred/{credential}/balance",
    params(
        ("credential" = String, Path, description = "Payment credential in bech32 format"),
    ),
    responses(
        (
            status = 200,
            description = "Lovelace and native asset balance of payment credential",
            body = Balance,
            example = json!({
                "data": {
                    "lovelace": "73408487",
                    "assets": {
                        "29d222ce763455e3d7a09a665ce554f00ac89d2e99a1a83d267170c6": {
                            "4d494e74": "27833420567",
                            "4d494e": "6958355141"
                        },
                        "2afb448ef716bfbed1dcb676102194c3009bee5399e93b90def9db6a": {
                            "4249534f4e": "5000000"
                        }
                    }
                },
                "last_updated": {
                    "timestamp": "2023-12-11 21:36:35",
                    "block_hash": "58df3617b77c9b8da958c118c3daf9cabae86e31aca761fe9bb8d57b40fe14be",
                    "block_slot": 110764304
                }
            })
        ),
        (status = 400, description = "Malformed query parameters"),
        (status = 500, description = "Internal server error"),
    )
)]
#[tracing::instrument(name = "BALANCE_BY_PAYMENT_CRED", level = "info", skip(config))]
/// Balance by payment credential
///
/// Return total amount of assets, including ADA, in UTxOs controlled by a specific payment credential
pub async fn balance_by_payment_cred(
    Path(credential): Path<String>,
    Extension(config): MapiExtension,
) -> Result<impl IntoResponse, ErrorResponse> {
    let polyphony = config.polyphony_wrapper;
    let chain = config.chain_info;
    let encoder = polyphony.utxo_cbor_by_address_encoder()?;

    // -- parse and try decode user params

    let payment_cred = utils::decode_payment_credential(&credential)?;

    // --- start db at most recent timestamp

    let mut txn = polyphony.begin_snapshot_latest().await?;

    // --- fetch cursor for last updated ---

    let last_updated = utils::get_last_updated(&mut txn, &encoder, &chain).await?;

    // --- scan keys for this page

    let range =
        encoder.encode_utxos_by_payment_cred_range(chain.network_id(), &payment_cred, None, None);

    let kvs = scan_extended(
        &mut txn,
        range,
        OrderParam::Asc,
        None::<fn(_) -> bool>,
        None,
        None,
    )
    .await?;

    // --- process fetched keys into response data

    let mut total_lovelace = 0;
    let mut total_assets: HashMap<[u8; 28], HashMap<Vec<u8>, u128>> = HashMap::new();

    for KvPair(_, txo_bytes) in kvs.iter() {
        let txo = MultiEraOutput::decode(Era::Conway, txo_bytes)
            .or_else(|_| MultiEraOutput::decode(Era::Babbage, txo_bytes))
            .or_else(|_| MultiEraOutput::decode(Era::Alonzo, txo_bytes))
            .or_else(|_| MultiEraOutput::decode(Era::Byron, txo_bytes))
            .map_err(internal_server_error)?;

        total_lovelace += txo.value().coin() as u128;

        for policy in txo.value().assets() {
            let asset_map = total_assets.entry(**policy.policy()).or_default();

            for asset in policy.assets() {
                let amount = asset.output_coin().unwrap();

                asset_map
                    .entry(asset.name().into())
                    .and_modify(|a| *a += amount as u128)
                    .or_insert(amount as u128);
            }
        }
    }

    // --- craft response data

    let total_assets = total_assets
        .into_iter()
        .map(|(p, am)| {
            (
                hex::encode(p),
                am.into_iter()
                    .map(|(a, x)| (hex::encode(a), x.to_string()))
                    .collect(),
            )
        })
        .collect();

    let balance = Balance {
        lovelace: total_lovelace.to_string(),
        assets: total_assets,
    };

    // ---

    let out = TimestampedResponse {
        data: balance,
        last_updated,
    };

    Ok((StatusCode::OK, Json(out)))
}
