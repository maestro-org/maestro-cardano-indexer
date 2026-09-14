use axum::{http::StatusCode, response::IntoResponse, Extension, Json};

use crate::{
    ogmios_v6,
    responses::{Eras, ErrorResponse, TimestampedResponse},
    utils::{self},
    MapiExtension,
};

#[utoipa::path(
    tag = "General",
    get,
    path = "/era-summaries",
    responses(
        (
            status = 200,
            description = "Get era summaries",
            body = TimestampedEraSummaries,
            example = json!({
              "data": [{
                  "start": {
                      "time": {
                          "seconds": 0
                      },
                      "slot": 0,
                      "epoch": 0
                  },
                  "end": {
                      "time": {
                          "seconds": 1728000
                      },
                      "slot": 86400,
                      "epoch": 4
                  },
                  "parameters": {
                      "epoch_length": 21600,
                      "slot_length": {
                          "milliseconds": 20000
                      },
                      "safe_zone": 4320
                  }
              }, {
                  "start": {
                      "time": {
                          "seconds": 1728000
                      },
                      "slot": 86400,
                      "epoch": 4
                  },
                  "end": {
                      "time": {
                          "seconds": 2160000
                      },
                      "slot": 518400,
                      "epoch": 5
                  },
                  "parameters": {
                      "epoch_length": 432000,
                      "slot_length": {
                          "milliseconds": 1000
                      },
                      "safe_zone": 129600
                  }
              }, {
                  "start": {
                      "time": {
                          "seconds": 2160000
                      },
                      "slot": 518400,
                      "epoch": 5
                  },
                  "end": {
                      "time": {
                          "seconds": 2592000
                      },
                      "slot": 950400,
                      "epoch": 6
                  },
                  "parameters": {
                      "epoch_length": 432000,
                      "slot_length": {
                          "milliseconds": 1000
                      },
                      "safe_zone": 129600
                  }
              }, {
                  "start": {
                      "time": {
                          "seconds": 2592000
                      },
                      "slot": 950400,
                      "epoch": 6
                  },
                  "end": {
                      "time": {
                          "seconds": 3024000
                      },
                      "slot": 1382400,
                      "epoch": 7
                  },
                  "parameters": {
                      "epoch_length": 432000,
                      "slot_length": {
                          "milliseconds": 1000
                      },
                      "safe_zone": 129600
                  }
              }, {
                  "start": {
                      "time": {
                          "seconds": 3024000
                      },
                      "slot": 1382400,
                      "epoch": 7
                  },
                  "end": {
                      "time": {
                          "seconds": 5184000
                      },
                      "slot": 3542400,
                      "epoch": 12
                  },
                  "parameters": {
                      "epoch_length": 432000,
                      "slot_length": {
                          "milliseconds": 1000
                      },
                      "safe_zone": 129600
                  }
              }, {
                  "start": {
                      "time": {
                          "seconds": 5184000
                      },
                      "slot": 3542400,
                      "epoch": 12
                  },
                  "end": {
                      "time": {
                          "seconds": 70416000
                      },
                      "slot": 68774400,
                      "epoch": 163
                  },
                  "parameters": {
                      "epoch_length": 432000,
                      "slot_length": {
                          "milliseconds": 1000
                      },
                      "safe_zone": 129600
                  }
              }, {
                  "start": {
                      "time": {
                          "seconds": 70416000
                      },
                      "slot": 68774400,
                      "epoch": 163
                  },
                  "end": {
                      "time": {
                          "seconds": 71280000
                      },
                      "slot": 69638400,
                      "epoch": 165
                  },
                  "parameters": {
                      "epoch_length": 432000,
                      "slot_length": {
                          "milliseconds": 1000
                      },
                      "safe_zone": 129600
                  }
              }],
              "last_updated": {
                  "timestamp": "2024-08-30 15:37:05",
                  "block_hash": "cffcdbe42399a72449325dbb8f207d84c86b9609ec36e45b67e9bc538ecdc6d6",
                  "block_slot": 69349025
              }
          })
        ),
        (status = 500, description = "Internal server error"),
        (status = 503, description = "ogmios temporarily unavailable, retry"),
        (status = 504, description = "ogmios query timed out"),
    )
)]
#[tracing::instrument(name = "ERA_SUMMARY", level = "info", skip(config))]
/// Era summary
///
/// Returns the blockchain era summaries. May include a future Era before hard forks. You should not assume the last item is the current epoch. Check "slot" against the current /chain-tip.
pub async fn era_summaries(
    Extension(config): MapiExtension,
) -> Result<impl IntoResponse, ErrorResponse> {
    let cache = config.era_summaries_cache.clone();

    let response = cache
        .get_or_refresh(|| async {
            let last_updated =
                utils::get_last_updated_dbsync(&config.chain_info, &config.dbsync).await?;

            let data: Eras = ogmios_v6::query(
                &config.ogmios_v6_url,
                "QueryLedgerStateEraSummaries",
                "queryLedgerState/eraSummaries",
                &config.ogmios_query_opts,
            )
            .await
            .map_err(utils::map_ogmios_error)?;

            Ok::<_, ErrorResponse>(TimestampedResponse { data, last_updated })
        })
        .await?;

    Ok((StatusCode::OK, Json(response)))
}
