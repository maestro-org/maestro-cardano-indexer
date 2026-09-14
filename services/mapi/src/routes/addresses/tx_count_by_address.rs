use crate::{
    responses::{self, ErrorResponse, TimestampedResponse},
    utils::{self, internal_server_error},
};
use axum::{extract::Path, http::StatusCode, response::IntoResponse, Extension, Json};

use crate::MapiExtension;

#[utoipa::path(
    tag = "Addresses",
    get,
    path = "/addresses/{address}/transactions/count",
    params(
        ("address" = String, Path, description = "Address in bech32 format"),
    ),
    responses(
        (
            status = 200,
            description = "Get the transaction count for an address",
            body = TimestampedTxCount,
            example = json!({
                "data": 11043,
                "last_updated": {
                    "timestamp": "2022-10-10 20:25:28",
                    "block_hash": "2d9eda3c353ed1e18bd7219cdee9d4911df9e582d76adf7f85b1f464a78bd5af",
                    "block_slot": 32265939
                }
            })
        ),
        (status = 400, description = "Malformed query parameters"),
        (status = 500, description = "Internal server error"),
    )
)]
#[tracing::instrument(name = "TX_COUNT_BY_ADDRESS", level = "info", skip(config))]
/// Address transaction count
///
/// Returns the number of transactions in which the address spent or received some funds.
///
/// Specifically, the number of transactions where: the address controlled at least one of the transaction inputs and/or receives one of the outputs AND the transaction is phase-2 valid, OR, the address controlled at least one of the collateral inputs and/or receives the collateral return output AND the transaction is phase-2 invalid. [Read more](https://docs.cardano.org/plutus/collateral-mechanism/).
pub async fn tx_count_by_address(
    Path(address): Path<String>,
    Extension(config): MapiExtension,
) -> Result<impl IntoResponse, ErrorResponse> {
    let polyphony = config.polyphony_wrapper;
    let chain = config.chain_info;

    let encoder = polyphony.tx_count_by_address_encoder()?;

    // -- parse and try decode user params

    let address = utils::decode_payment_address(&address)?;

    // --- start db at most recent timestamp

    let mut txn = polyphony.begin_snapshot_latest().await?;

    // --- fetch cursor for last updated ---

    let last_updated = utils::get_last_updated(&mut txn, &encoder, &chain).await?;

    // ---

    let key = encoder.encode_tx_count_by_address_key(address);

    let mut txn = polyphony.begin_snapshot_latest().await?;

    let value_bytes: [u8; 8] = txn
        .get(key)
        .await
        .map_err(internal_server_error)?
        .unwrap_or(Vec::from([0; 8]))[..8]
        .try_into()
        .map_err(internal_server_error)?;

    let count = u64::from_be_bytes(value_bytes);

    // ---

    let out = TimestampedResponse {
        data: responses::TxCount(count),
        last_updated,
    };

    Ok((StatusCode::OK, Json(out)))
}
