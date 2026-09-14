use axum::{extract::Path, http::StatusCode, response::IntoResponse, Extension, Json};
use pallas::codec::minicbor;
use pallas::ledger::primitives::{babbage::PlutusData, ToCanonicalJson};

use crate::{
    responses::{Datum, ErrorResponse, TimestampedResponse},
    utils::{self, bad_request, internal_server_error, not_found},
    MapiExtension,
};

#[utoipa::path(
    tag = "Datums",
    get,
    path = "/datums/{datum_hash}",
    params(
        ("datum_hash" = String, Path, description = "Hex encoded datum hash"),
    ),
    responses(
        (
            status = 200,
            description = "Datum corresponding to the datum hash",
            body = TimestampedDatum,
            example = json!({
                "data": {
                    "json": {
                        "constructor": 0,
                        "fields": [
                            {
                                "bytes": "76140b3e82b58a9439ce10473d0d7c40ec9165fb03662c499a9162f95b9ec34641adbb83203c5276eaa35fc95c3776e00779ca412777cdc5201b53184d365306"
                            },
                            {
                                "constructor": 0,
                                "fields": [
                                    {
                                        "constructor": 0,
                                        "fields": [
                                            {
                                                "int": 100000
                                            },
                                            {
                                                "int": 36069
                                            }
                                        ]
                                    },
                                    {
                                        "constructor": 0,
                                        "fields": [
                                            {
                                                "constructor": 0,
                                                "fields": [
                                                    {
                                                        "constructor": 1,
                                                        "fields": [
                                                            {
                                                                "int": 1676063825000i64
                                                            }
                                                        ]
                                                    },
                                                    {
                                                        "constructor": 1,
                                                        "fields": []
                                                    }
                                                ]
                                            },
                                            {
                                                "constructor": 0,
                                                "fields": [
                                                    {
                                                        "constructor": 1,
                                                        "fields": [
                                                            {
                                                                "int": 1676065625000i64
                                                            }
                                                        ]
                                                    },
                                                    {
                                                        "constructor": 1,
                                                        "fields": []
                                                    }
                                                ]
                                            }
                                        ]
                                    },
                                    {
                                        "bytes": "555344"
                                    }
                                ]
                            },
                            {
                                "bytes": "0c64d6d0371d11185aae649cf3a169040e94214137137b531ebb16c2"
                            }
                        ]
                    },
                    "bytes": "d8799f584076140b3e82b58a9439ce10473d0d7c40ec9165fb03662c499a9162f95b9ec34641adbb83203c5276eaa35fc95c3776e00779ca412777cdc5201b53184d365306d8799fd8799f1a000186a0198ce5ffd8799fd8799fd87a9f1b000001863d305c68ffd87a80ffd8799fd87a9f1b000001863d4bd3a8ffd87a80ffff43555344ff581c0c64d6d0371d11185aae649cf3a169040e94214137137b531ebb16c2ff"
                },
                "last_updated": {
                    "timestamp": "2022-10-10 20:25:28",
                    "block_hash": "391e8974a46a05addd54593722380fdea42f29d7de4acc5729a6ec0663955293",
                    "block_slot": 32284048
                }
            })
        ),
        (status = 400, description = "Malformed query parameters"),
        (status = 404, description = "No results found"),
        (status = 500, description = "Internal server error"),
    )
)]
#[tracing::instrument(name = "DATUM_BY_HASH", level = "info", skip(config))]
/// Datum by datum hash
///
/// Returns the datum corresponding to the specified datum hash, if the datum has been seen on-chain
pub async fn datum_by_hash(
    Path(datum_hash): Path<String>,
    Extension(config): MapiExtension,
) -> Result<impl IntoResponse, ErrorResponse> {
    let chain = config.chain_info;
    let polyphony = config.polyphony_wrapper;
    let encoder = polyphony.datum_by_hash_encoder()?;

    let datum_hash: [u8; 32] = match hex::decode(datum_hash) {
        Ok(b) => b
            .try_into()
            .map_err(|_| bad_request("Malformed datum hash"))?,
        Err(_) => return Err(bad_request("Datum hash must be hex encoded")),
    };

    let key = encoder.encode_datum_by_hash_key(&datum_hash);

    let mut txn = polyphony.begin_snapshot_latest().await?;

    // --- fetch cursor for last updated ---

    let last_updated = utils::get_last_updated(&mut txn, &encoder, &chain).await?;

    // ---

    let bytes = txn
        .get(key)
        .await
        .map_err(internal_server_error)?
        .ok_or_else(not_found)?;

    if bytes.is_empty() {
        return Err(not_found());
    }

    let plutus_data: PlutusData = minicbor::decode(&bytes).map_err(internal_server_error)?;

    let datum = Datum {
        json: plutus_data.to_json(),
        bytes: hex::encode(bytes),
    };

    // ---

    let out = TimestampedResponse {
        data: datum,
        last_updated,
    };

    Ok((StatusCode::OK, Json(out)))
}
