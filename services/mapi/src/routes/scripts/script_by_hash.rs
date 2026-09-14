use axum::{extract::Path, http::StatusCode, response::IntoResponse, Extension, Json};

use crate::{
    responses::{ErrorResponse, TimestampedResponse},
    utils::{self, bad_request},
    MapiExtension,
};

#[utoipa::path(
    tag = "Scripts",
    get,
    path = "/scripts/{script_hash}",
    params(
        ("script_hash" = String, Path, description = "Hex encoded script hash"),
    ),
    responses(
        (
            status = 200,
            description = "Script corresponding to the provided script hash",
            body = TimestampedScriptFirstSeen,
            examples(
                ("Plutus" = (
                    summary = "An example response for a Plutus script",
                    value = json!({
                        "data": {
                            "hash": "a65ca58a4e9c755fa830173d2a5caed458ac0c73f97db7faae2e7e3b",
                            "type": "plutusv1",
                            "bytes": "59014f59014c0100003232323232323232222...8c0088cc00800800555cf2ba15573e6e1d200201",
                            "json": null,
                            "first_seen": {
                                "tx_hash": "27836bbea6fba67a9209de16c81d426e92ac218e5007fbf86d3a6786ca1db316",
                                "slot": 106047961,
                                "timestamp": "2023-10-18 07:30:52"
                            }
                        },
                        "last_updated": {
                            "timestamp": "2023-10-18 07:30:52",
                            "block_hash": "0e1e924710135acfe200ab13d290bd282a67584fd54456f0dcac0aeaa38bb2c2",
                            "block_slot": 106047961
                        }
                    }))
                ),
                ("Native" = (
                    summary = "An example response for a Native script",
                    value = json!({
                        "data": {
                            "hash": "f0ff48bbb7bbe9d59a40f1ce90e9e9d0ff5002ec48f232b49ca0fb9a",
                            "type": "native",
                            "bytes": "8200581c4da965a049dfd15ed1ee19fba6e2974a0b79fc416dd1796a1f97f5e1",
                            "json": {
                                "keyHash": "4da965a049dfd15ed1ee19fba6e2974a0b79fc416dd1796a1f97f5e1",
                                "type": "sig"
                            },
                            "first_seen": {
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
                    }))
                ),
            )
        ),
        (status = 400, description = "Malformed query parameters"),
        (status = 404, description = "No results found"),
        (status = 500, description = "Internal server error"),
    )
)]
#[tracing::instrument(name = "SCRIPT_BY_HASH", level = "info", skip(config))]
/// Script by script hash
///
/// Returns the script corresponding to the specified script hash, if the script has been seen on-chain
pub async fn script_by_hash(
    Path(script_hash): Path<String>,
    Extension(config): MapiExtension,
) -> Result<impl IntoResponse, ErrorResponse> {
    let chain = config.chain_info;
    let polyphony = config.polyphony_wrapper;
    let encoder = polyphony.script_by_hash_encoder()?;

    // -- parse and try decode user params

    let script_hash: [u8; 28] = match hex::decode(script_hash) {
        Ok(b) => b
            .try_into()
            .map_err(|_| bad_request("Malformed datum hash"))?,
        Err(_) => return Err(bad_request("Datum hash must be hex encoded")),
    };

    // --- get dbsync tip for last updated

    let mut txn = polyphony.begin_snapshot_latest().await?;

    // --- fetch cursor for last updated ---

    let last_updated = utils::get_last_updated(&mut txn, &encoder, &chain).await?;

    // --- query data

    let script = utils::fetch_script(&mut txn, &polyphony, &chain, script_hash).await?;

    // ---

    let out = TimestampedResponse {
        data: script,
        last_updated,
    };

    Ok((StatusCode::OK, Json(out)))
}
