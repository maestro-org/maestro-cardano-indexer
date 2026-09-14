use crate::{
    responses::{ErrorResponse, TimestampedResponse},
    utils::{self, internal_server_error, internal_server_error_str},
};
use axum::{http::StatusCode, response::IntoResponse, Extension, Json};
use pallas::network::miniprotocols::Point;
use timbre::encoding::decode::decode_cursor_value;

use crate::{responses::ChainTip, MapiExtension};

#[utoipa::path(
    tag = "General",
    get,
    path = "/chain-tip",
    responses(
        (
            status = 200,
            description = "Get details of the chain-tip (lastest block)",
            body = TimestampedChainTip,
            example = json!({
                "data": {
                    "block_hash": "4c0916eea22254efc2a35f72b4fa8ec5caaf3cd0b193a4df8efb4c0f5ff2be4a",
                    "slot": 32284578,
                    "height": 1102039
                },
                "last_updated": {
                    "timestamp": "2022-10-10 20:25:28",
                    "block_hash": "4c0916eea22254efc2a35f72b4fa8ec5caaf3cd0b193a4df8efb4c0f5ff2be4a",
                    "block_slot": 32284578
                }
            })
        ),
        (status = 500, description = "Internal server error"),
    )
)]
#[tracing::instrument(name = "CHAIN_TIP", level = "info", skip(config))]
/// Chain-tip
///
/// Returns the identifier of the most recently processed block on the network
pub async fn chain_tip(
    Extension(config): MapiExtension,
) -> Result<impl IntoResponse, ErrorResponse> {
    let polyphony = config.polyphony_wrapper;
    let chain = config.chain_info;
    // just use the tip for the tx by hash reducer for chain tip
    let encoder = polyphony.tx_by_hash_encoder()?;

    let key = encoder.encode_cursor_key();

    let mut txn = polyphony.begin_snapshot_latest().await?;

    // --- fetch cursor for last updated ---

    let last_updated = utils::get_last_updated(&mut txn, &encoder, &chain).await?;

    // ---

    let value_bytes = txn
        .get(key)
        .await
        .map_err(internal_server_error)?
        .ok_or_else(|| internal_server_error_str("cursor key not found in db"))?;

    let (point, height) = decode_cursor_value(&value_bytes);

    let (slot, block_hash) = match point {
        Point::Origin => (0, hex::encode([0; 32])),
        Point::Specific(s, b) => (s, hex::encode(b)),
    };

    let tip = ChainTip {
        block_hash,
        slot,
        height,
    };

    // ---

    let out = TimestampedResponse {
        data: tip,
        last_updated,
    };

    Ok((StatusCode::OK, Json(out)))
}
