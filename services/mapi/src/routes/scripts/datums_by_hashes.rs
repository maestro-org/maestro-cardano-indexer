use std::collections::HashMap;

use crate::{
    responses::{Datum, ErrorResponse, TimestampedResponse},
    utils::{self, bad_request, internal_server_error},
    MapiExtension,
};
use axum::{http::StatusCode, response::IntoResponse, Extension, Json};
use pallas::codec::minicbor;
use pallas::ledger::primitives::{babbage::PlutusData, ToCanonicalJson};

#[utoipa::path(
    tag = "Datums",
    post,
    path = "/datums",
    request_body(content = [String], example = json!(["f03819a4039003a8c6b65153351adbc61f983bd22787ed238a5b4f24a01aa5d6", "5f7f360208c46d739296ea3ff4f41d85240d94aa3f59e5b1c82c2bbeceb57f23"])),
    responses(
        (
            status = 200,
            description = "Map of datum hashes to datum objects",
            body = TimestampedDatumMap,
            example = json!({
                "data": {
                    "5f7f360208c46d739296ea3ff4f41d85240d94aa3f59e5b1c82c2bbeceb57f23": {
                        "json": {
                            "constructor": 0,
                            "fields": [
                                {
                                    "bytes": "e1d915c10c840017bd39088a82507b27150a438e8907784221491309"
                                },
                                {
                                    "bytes": "e1d915c10c840017bd39088a82507b27150a438e8907784221491309"
                                }
                            ]
                        },
                        "bytes": "d8799f581ce1d915c10c840017bd3908...1a009896801a9a7ec8001b00000183c392c9a1ff"
                    },
                    "f03819a4039003a8c6b65153351adbc61f983bd22787ed238a5b4f24a01aa5d6": {
                        "json": {
                            "constructor": 0,
                            "fields": [
                                {
                                    "constructor": 0,
                                    "fields": [
                                        {
                                            "constructor": 0,
                                            "fields": [
                                                {
                                                    "bytes": "fc6e1b47816bc4a4165a39f3c6dd65868e6b74a743dc2dcf54eb2d4c"
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
                                                            "constructor": 0,
                                                            "fields": [
                                                                {
                                                                    "bytes": "d756fb7ceee90ed5af3f193d73f9970ca640ab540eedac272cb28c29"
                                                                }
                                                            ]
                                                        }
                                                    ]
                                                }
                                            ]
                                        }
                                    ]
                                }
                            ]
                        },
                        "bytes": "d8799fd8799fd8799f581cfc6e1b47816bc...d8799fd8799f581cd756fb7ceee90ed5af3f193d73f9970ca640ab540eedac272cb28c29ffffffffd8799fd8799f581cfc6e1b47816bc4a4165a39f3c6dd65868e6b74a743dc2dcf54eb2d4cffd8799fd8799fd8799f581cd756fb7ceee90ed5af3f193d73f9970ca640ab540eedac272cb28c29ffffffffd87a80d87a9fd8799f581c25f0fc240e91bd95dcdaebd2ba7713fc5168ac77234a3d79449fc20c47534f4349455459ff1a0bebc200ff1a001e84801a001e8480ff"
                    }
                },
                "last_updated": {
                    "timestamp": "2023-10-18 07:30:52",
                    "block_hash": "0e1e924710135acfe200ab13d290bd282a67584fd54456f0dcac0aeaa38bb2c2",
                    "block_slot": 106047961
                }
            }) //
        ),
        (status = 400, description = "Malformed query parameters"),
        (status = 500, description = "Internal server error"),
    )
)]
#[tracing::instrument(name = "DATUMS_BY_HASHES", level = "info", skip(config))]
/// Datums by hashes
///
/// Returns the datums corresponding to the specified datum hashes, if the datums have been seen on-chain
pub async fn datums_by_hashes(
    Extension(config): MapiExtension,
    Json(payload): Json<Vec<String>>,
) -> Result<impl IntoResponse, ErrorResponse> {
    let chain = config.chain_info;
    let polyphony = config.polyphony_wrapper;

    // -- parse and try decode user params

    let datum_hash_strs: Vec<String> = payload;

    if datum_hash_strs.len() > 100 {
        return Err(bad_request("Payload size must not exceed 100 items"));
    }

    let mut datum_hashes = vec![];

    for datum_hash_str in datum_hash_strs {
        let datum_hash: [u8; 32] = match hex::decode(datum_hash_str) {
            Ok(b) => b
                .try_into()
                .map_err(|_| bad_request("Malformed datum hash"))?,
            Err(_) => return Err(bad_request("Datum hash must be hex encoded")),
        };

        datum_hashes.push(datum_hash)
    }

    // --- start db at most recent timestamp

    let mut txn = polyphony.begin_snapshot_latest().await?;

    // --- fetch cursor for last updated ---

    let last_updated =
        utils::get_last_updated(&mut txn, &polyphony.datum_by_hash_encoder()?, &chain).await?;

    // ---

    let resolved_dhs =
        utils::fetch_datums(&mut txn, &polyphony.datum_by_hash_encoder()?, datum_hashes).await?;

    // ---

    let datum_map = resolved_dhs
        .into_iter()
        .map(|(dh, h)| {
            let key = hex::encode(dh);

            let plutus_data: PlutusData = minicbor::decode(&h).map_err(internal_server_error)?;

            let value = Datum {
                json: plutus_data.to_json(),
                bytes: hex::encode(h),
            };

            Ok((key, value))
        })
        .collect::<Result<HashMap<_, _>, ErrorResponse>>()?;

    let out = TimestampedResponse {
        data: datum_map,
        last_updated,
    };

    Ok((StatusCode::OK, Json(out)))
}
