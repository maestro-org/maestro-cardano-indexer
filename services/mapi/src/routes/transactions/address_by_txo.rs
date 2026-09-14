use axum::{extract::Path, http::StatusCode, response::IntoResponse, Extension, Json};
use pallas::ledger::traverse::MultiEraTx;

use crate::{
    responses::{self, ErrorResponse, TimestampedResponse},
    utils::{self, internal_server_error, not_found},
    MapiExtension,
};

#[utoipa::path(
    tag = "Transactions",
    get,
    path = "/transactions/{tx_hash}/outputs/{index}/address",
    params(
        ("tx_hash" = String, Path, description = "Transaction Hash"),
        ("index" = u32, Path, description = "Output Index")
    ),
    responses(
        (
            status = 200,
            description = "Get an address via a transaction output reference",
            body = TimestampedAddress,
            example = json!({
                "data": "addr_test1wzdtu0djc76qyqak9cj239udezj2544nyk3ksmfqvaksv7c9xanpg",
                "last_updated": {
                    "timestamp": "2022-10-10 20:25:28",
                    "block_hash": "8cd56aae0fc966903b2b215a477d57e7b3f4a225af3eeea4b62be724c9fa1195",
                    "block_slot": 32295075
                }
            })
        ),
        (status = 400, description = "Malformed query parameters"),
        (status = 404, description = "No results found"),
        (status = 500, description = "Internal server error"),
    )
)]
#[tracing::instrument(name = "ADDRESS_BY_TXO", level = "info", skip(config))]
/// Address by transaction output reference
///
/// Returns the address which was specified in the given transaction output.
///
/// Note that if the transaction is invalid this will only return a result for the collateral return output, should one be present in the transaction. If the transaction is valid it will not return a result for the collateral return output.
pub async fn address_by_txo(
    Path((tx_hash, index)): Path<(String, usize)>,
    Extension(config): MapiExtension,
) -> Result<impl IntoResponse, ErrorResponse> {
    let chain = config.chain_info;
    let polyphony = config.polyphony_wrapper;

    let (cbor, mut snap) = polyphony.get_tx_bytes(&tx_hash).await?;

    // --- fetch cursor for last updated ---

    let last_updated =
        utils::get_last_updated(&mut snap, &polyphony.tx_by_hash_encoder()?, &chain).await?;

    // ---

    let tx = MultiEraTx::decode(&cbor).map_err(internal_server_error)?;

    let txout = tx.produces_at(index).ok_or_else(not_found)?;

    let address = txout.address().unwrap().to_string();

    // ---

    let out = TimestampedResponse {
        data: responses::Address(address),
        last_updated,
    };

    Ok((StatusCode::OK, Json(out)))
}
