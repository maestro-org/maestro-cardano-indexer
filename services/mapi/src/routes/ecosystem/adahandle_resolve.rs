use axum::{extract::Path, http::StatusCode, response::IntoResponse, Extension, Json};
use lazy_static::lazy_static;
use pallas::ledger::addresses::Address;
use pallas::ledger::primitives::babbage::PolicyId;
use tikv_client::Snapshot;
use timbre::encoding::encode::KeyEncoder;

use crate::{
    responses::{self, ErrorResponse, TimestampedResponse},
    utils::{self, bad_request, fetch_single_utxo_containing_asset, not_found, UniqueAssetUtxo},
    MapiExtension,
};

lazy_static! {
    pub static ref ADAHANDLE_POLICY: [u8; 28] =
        hex::decode("f0ff48bbb7bbe9d59a40f1ce90e9e9d0ff5002ec48f232b49ca0fb9a").unwrap()[..28]
            .try_into()
            .unwrap();
}

#[utoipa::path(
    tag = "Ecosystem",
    get,
    path = "/ecosystem/adahandle/{handle}",
    params(
        ("handle" = String, Path, description = "Ada Handle to resolve"),
    ),
    responses(
        (
            status = 200,
            description = "Return the address which the Ada Handle resolves to",
            body = TimestampedAddress,
            example = json!({
                "data": "addr1vyn6t3yqfy5awnc5pekjxww4cn89y3g4pt5rmuykg4y2urc56c56e",
                "last_updated": {
                    "timestamp": "2022-10-10 20:25:28",
                    "block_hash": "bdf4630098ac6923e0ef1ca0b0d6e00dca96790a8c3dd670948fdfe1456798d2",
                    "block_slot": 73867237
                }
            })
        ),
        (status = 400, description = "Malformed query parameters"),
        (status = 404, description = "No results found"),
        (status = 500, description = "Internal server error"),
    )
)]
#[tracing::instrument(name = "ADAHANDLE_RESOLVE", level = "info", skip(config))]
/// Resolve ADA Handle
///
/// Returns the Cardano address corresponding to an ADA Handle
pub async fn adahandle_resolve(
    Path(handle): Path<String>,
    Extension(config): MapiExtension,
) -> Result<impl IntoResponse, ErrorResponse> {
    let chain = config.chain_info;
    let polyphony = config.polyphony_wrapper;
    let encoder = polyphony.utxos_by_asset_encoder()?;

    // -- parse and try decode user params

    if !valid_handle(&handle) {
        return Err(bad_request("Ada Handle is invalid"));
    };

    // --- start db at most recent timestamp

    let mut txn = polyphony.begin_snapshot_latest().await?;

    // --- fetch cursor for last updated ---

    let last_updated = utils::get_last_updated(&mut txn, &encoder, &chain).await?;

    // -- endpoint logic

    let policy: PolicyId = (*ADAHANDLE_POLICY).into();

    // first try resolve as new cip68 handle standard
    let mut resolved = try_fetch_handle_cip68(&mut txn, &encoder, &policy, &handle).await?;

    // TODO: then try resolve as cip67 virtual subhandle standard (not live)

    // finally try resolve as classic handle standard
    if resolved.is_none() {
        resolved = try_fetch_handle_simple(&mut txn, &encoder, &policy, handle).await?
    }

    if let Some(address) = resolved {
        Ok((
            StatusCode::OK,
            Json(TimestampedResponse {
                data: responses::Address(address.to_string()),
                last_updated,
            }),
        ))
    } else {
        Err(not_found())
    }
}

/// Try parse an Ada Handle from user supplied string, checking the characters
/// and length are valid, and return the valid handle in lowercase which will
/// strip a single '$' from the front of the handle if one is present.
fn valid_handle(string: &str) -> bool {
    let valid = "abcdefghijklmnopqrstuvwxyz0123456789-_.@$";

    if !string.chars().all(|c| valid.contains(c)) {
        return false;
    }

    // handles can be max 15 chars (we allow 16 for optional '$' at front)
    if string.is_empty() || string.len() > 16 {
        return false;
    }

    true
}

async fn try_fetch_handle_cip68(
    txn: &mut Snapshot,
    encoder: &KeyEncoder,
    policy: &PolicyId,
    handle: &String,
) -> Result<Option<Address>, ErrorResponse> {
    let mut handle_222 = vec![0x00, 0x0d, 0xe1, 0x40];
    handle_222.extend(handle.as_bytes());

    if let Some(UniqueAssetUtxo { address, .. }) =
        fetch_single_utxo_containing_asset(txn, encoder, policy, &handle_222.into()).await?
    {
        Ok(Some(address))
    } else {
        Ok(None)
    }
}

/// Find the UTxO of the adahandle policy where the asset name is precisely
/// the handle.
async fn try_fetch_handle_simple(
    txn: &mut Snapshot,
    encoder: &KeyEncoder,
    policy: &PolicyId,
    handle: String,
) -> Result<Option<Address>, ErrorResponse> {
    if let Some(UniqueAssetUtxo { address, .. }) =
        fetch_single_utxo_containing_asset(txn, encoder, policy, &handle.as_bytes().to_vec().into())
            .await?
    {
        Ok(Some(address))
    } else {
        Ok(None)
    }
}
