use axum::{extract::Path, http::StatusCode, response::IntoResponse, Extension, Json};

use crate::{
    responses::{self, ErrorResponse, TimestampedResponse},
    utils, MapiExtension,
};

#[utoipa::path(
    tag = "Transactions",
    get,
    path = "/transactions/{tx_hash}/cbor",
    params(
        ("tx_hash" = String, Path, description = "Transaction Hash"),
    ),
    responses(
        (
            status = 200,
            description = "Get a transaction's hex encoded cbor via a transaction hash",
            body = TimestampedTxCbor,
            example = json!({
                "data": "84a8008282582085b5faa0ea5e....0821a0008743d1a0c1c6598f5f6",
                "last_updated": {
                    "timestamp": "2022-10-10 20:25:28",
                    "block_hash": "9f729f5404f6a4efcd65f5306e8c1784a9277f7408369cb92c567ccbfb6460b6",
                    "block_slot": 32295201
                }
            })
        ),
        (status = 400, description = "Malformed query parameters"),
        (status = 404, description = "No results found"),
        (status = 500, description = "Internal server error"),
    )
)]
#[tracing::instrument(name = "TX_CBOR_BY_TX_HASH", level = "info", skip(config))]
/// CBOR bytes of a transaction
///
/// Returns hex-encoded CBOR bytes of a transaction
pub async fn tx_cbor_by_tx_hash(
    Path(tx_hash): Path<String>,
    Extension(config): MapiExtension,
) -> Result<impl IntoResponse, ErrorResponse> {
    let chain = config.chain_info;
    let polyphony = config.polyphony_wrapper;

    let (cbor, mut txn) = polyphony.get_tx_bytes(&tx_hash).await?;

    // --- fetch cursor for last updated ---

    let last_updated =
        utils::get_last_updated(&mut txn, &polyphony.tx_by_hash_encoder()?, &chain).await?;

    // ---

    let out = TimestampedResponse {
        data: responses::TxCbor(hex::encode(cbor)),
        last_updated,
    };

    Ok((StatusCode::OK, Json(out)))
}
