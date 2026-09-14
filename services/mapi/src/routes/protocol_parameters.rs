use axum::{http::StatusCode, response::IntoResponse, Extension, Json};

use crate::{
    ogmios_v6,
    responses::{ErrorResponse, ProtocolParametersV6, TimestampedResponse},
    utils::{self},
    MapiExtension,
};

#[utoipa::path(
    tag = "General",
    get,
    path = "/protocol-parameters",
    responses(
        (
            status = 200,
            description = "Get protocol params",
            body = TimestampedProtocolParameters,
            example = json!({
              "data": {
                  "min_fee_coefficient": 44,
                  "min_fee_constant": {
                      "ada": {
                          "lovelace": 155381
                      }
                  },
                  "min_fee_reference_scripts": {
                      "base": 15.0,
                      "range": 25600,
                      "multiplier": 1.2
                  },
                  "max_block_body_size": {
                      "bytes": 90112
                  },
                  "max_block_header_size": {
                      "bytes": 1100
                  },
                  "max_transaction_size": {
                      "bytes": 16384
                  },
                  "max_reference_scripts_size": {
                      "bytes": 204800
                  },
                  "stake_credential_deposit": {
                      "ada": {
                          "lovelace": 2000000
                      }
                  },
                  "stake_pool_deposit": {
                      "ada": {
                          "lovelace": 500000000
                      }
                  },
                  "stake_pool_retirement_epoch_bound": 18,
                  "desired_number_of_stake_pools": 500,
                  "stake_pool_pledge_influence": "3/10",
                  "monetary_expansion": "3/1000",
                  "treasury_expansion": "1/5",
                  "min_stake_pool_cost": {
                      "ada": {
                          "lovelace": 170000000
                      }
                  },
                  "min_utxo_deposit_constant": {
                      "ada": {
                          "lovelace": 0
                      }
                  },
                  "min_utxo_deposit_coefficient": 4310,
                  "plutus_cost_models": {
                      "plutus_v1": [100788, 420, 1, 1, 1000, 173, 0, 1, 1000, 59957, 4, 1, 11183, 32, 201305, 8356, 4, 16000, 100, 16000, 100, 16000, 100, 16000, 100, 16000, 100, 16000, 100, 100, 100, 16000, 100, 94375, 32, 132994, 32, 61462, 4, 72010, 178, 0, 1, 22151, 32, 91189, 769, 4, 2, 85848, 228465, 122, 0, 1, 1, 1000, 42921, 4, 2, 24548, 29498, 38, 1, 898148, 27279, 1, 51775, 558, 1, 39184, 1000, 60594, 1, 141895, 32, 83150, 32, 15299, 32, 76049, 1, 13169, 4, 22100, 10, 28999, 74, 1, 28999, 74, 1, 43285, 552, 1, 44749, 541, 1, 33852, 32, 68246, 32, 72362, 32, 7243, 32, 7391, 32, 11546, 32, 85848, 228465, 122, 0, 1, 1, 90434, 519, 0, 1, 74433, 32, 85848, 228465, 122, 0, 1, 1, 85848, 228465, 122, 0, 1, 1, 270652, 22588, 4, 1457325, 64566, 4, 20467, 1, 4, 0, 141992, 32, 100788, 420, 1, 1, 81663, 32, 59498, 32, 20142, 32, 24588, 32, 20744, 32, 25933, 32, 24623, 32, 53384111, 14333, 10],
                      "plutus_v2": [100788, 420, 1, 1, 1000, 173, 0, 1, 1000, 59957, 4, 1, 11183, 32, 201305, 8356, 4, 16000, 100, 16000, 100, 16000, 100, 16000, 100, 16000, 100, 16000, 100, 100, 100, 16000, 100, 94375, 32, 132994, 32, 61462, 4, 72010, 178, 0, 1, 22151, 32, 91189, 769, 4, 2, 85848, 228465, 122, 0, 1, 1, 1000, 42921, 4, 2, 24548, 29498, 38, 1, 898148, 27279, 1, 51775, 558, 1, 39184, 1000, 60594, 1, 141895, 32, 83150, 32, 15299, 32, 76049, 1, 13169, 4, 22100, 10, 28999, 74, 1, 28999, 74, 1, 43285, 552, 1, 44749, 541, 1, 33852, 32, 68246, 32, 72362, 32, 7243, 32, 7391, 32, 11546, 32, 85848, 228465, 122, 0, 1, 1, 90434, 519, 0, 1, 74433, 32, 85848, 228465, 122, 0, 1, 1, 85848, 228465, 122, 0, 1, 1, 955506, 213312, 0, 2, 270652, 22588, 4, 1457325, 64566, 4, 20467, 1, 4, 0, 141992, 32, 100788, 420, 1, 1, 81663, 32, 59498, 32, 20142, 32, 24588, 32, 20744, 32, 25933, 32, 24623, 32, 43053543, 10, 53384111, 14333, 10, 43574283, 26308, 10],
                      "plutus_v3": [100788, 420, 1, 1, 1000, 173, 0, 1, 1000, 59957, 4, 1, 11183, 32, 201305, 8356, 4, 16000, 100, 16000, 100, 16000, 100, 16000, 100, 16000, 100, 16000, 100, 100, 100, 16000, 100, 94375, 32, 132994, 32, 61462, 4, 72010, 178, 0, 1, 22151, 32, 91189, 769, 4, 2, 85848, 123203, 7305, -900, 1716, 549, 57, 85848, 0, 1, 1, 1000, 42921, 4, 2, 24548, 29498, 38, 1, 898148, 27279, 1, 51775, 558, 1, 39184, 1000, 60594, 1, 141895, 32, 83150, 32, 15299, 32, 76049, 1, 13169, 4, 22100, 10, 28999, 74, 1, 28999, 74, 1, 43285, 552, 1, 44749, 541, 1, 33852, 32, 68246, 32, 72362, 32, 7243, 32, 7391, 32, 11546, 32, 85848, 123203, 7305, -900, 1716, 549, 57, 85848, 0, 1, 90434, 519, 0, 1, 74433, 32, 85848, 123203, 7305, -900, 1716, 549, 57, 85848, 0, 1, 1, 85848, 123203, 7305, -900, 1716, 549, 57, 85848, 0, 1, 955506, 213312, 0, 2, 270652, 22588, 4, 1457325, 64566, 4, 20467, 1, 4, 0, 141992, 32, 100788, 420, 1, 1, 81663, 32, 59498, 32, 20142, 32, 24588, 32, 20744, 32, 25933, 32, 24623, 32, 43053543, 10, 53384111, 14333, 10, 43574283, 26308, 10, 16000, 100, 16000, 100, 962335, 18, 2780678, 6, 442008, 1, 52538055, 3756, 18, 267929, 18, 76433006, 8868, 18, 52948122, 18, 1995836, 36, 3227919, 12, 901022, 1, 166917843, 4307, 36, 284546, 36, 158221314, 26549, 36, 74698472, 36, 333849714, 1, 254006273, 72, 2174038, 72, 2261318, 64571, 4, 207616, 8310, 4, 1293828, 28716, 63, 0, 1, 1006041, 43623, 251, 0, 1]
                  },
                  "script_execution_prices": {
                      "memory": "577/10000",
                      "cpu": "721/10000000"
                  },
                  "max_execution_units_per_transaction": {
                      "memory": 14000000,
                      "cpu": 10000000000i64
                  },
                  "max_execution_units_per_block": {
                      "memory": 62000000,
                      "cpu": 20000000000i64
                  },
                  "max_value_size": {
                      "bytes": 5000
                  },
                  "collateral_percentage": 150,
                  "max_collateral_inputs": 3,
                  "version": {
                      "major": 9,
                      "minor": 0
                  },
                  "stake_pool_voting_thresholds": {
                      "no_confidence": "51/100",
                      "constitutional_committee": {
                          "default": "51/100",
                          "state_of_no_confidence": "51/100"
                      },
                      "hard_fork_initiation": "51/100",
                      "protocol_parameters_update": {
                          "security": "51/100"
                      }
                  },
                  "delegate_representative_voting_thresholds": {
                      "no_confidence": "67/100",
                      "constitutional_committee": {
                          "default": "67/100",
                          "state_of_no_confidence": "3/5"
                      },
                      "constitution": "3/4",
                      "hard_fork_initiation": "3/5",
                      "protocol_parameters_update": {
                          "network": "67/100",
                          "economic": "67/100",
                          "technical": "67/100",
                          "governance": "3/4"
                      },
                      "treasury_withdrawals": "67/100"
                  },
                  "constitutional_committee_min_size": 7,
                  "constitutional_committee_max_term_length": 146,
                  "governance_action_lifetime": 6,
                  "governance_action_deposit": {
                      "ada": {
                          "lovelace": 100000000000i64
                      }
                  },
                  "delegate_representative_deposit": {
                      "ada": {
                          "lovelace": 500000000
                      }
                  },
                  "delegate_representative_max_idle_time": 20
              },
              "last_updated": {
                  "timestamp": "2024-08-29 16:03:26",
                  "block_hash": "ebe27467ad34f4ecb80a3a537aae2e084e34d751426d484f87a5a6a9db4c216f",
                  "block_slot": 69264206
              }
          }
        )
        ),
        (status = 500, description = "Internal server error"),
        (status = 503, description = "ogmios temporarily unavailable, retry"),
        (status = 504, description = "ogmios query timed out"),
    )
)]
#[tracing::instrument(name = "PROTOCOL_PARAMETERS", level = "info", skip(config))]
/// Protocol parameters
///
/// Returns the current blockchain protocol parameters
pub async fn protocol_parameters(
    Extension(config): MapiExtension,
) -> Result<impl IntoResponse, ErrorResponse> {
    let cache = config.protocol_parameters_cache.clone();

    let response = cache
        .get_or_refresh(|| async {
            let last_updated =
                utils::get_last_updated_dbsync(&config.chain_info, &config.dbsync).await?;

            let data: ProtocolParametersV6 = ogmios_v6::query(
                &config.ogmios_v6_url,
                "QueryLedgerStateProtocolParameters",
                "queryLedgerState/protocolParameters",
                &config.ogmios_query_opts,
            )
            .await
            .map_err(utils::map_ogmios_error)?;

            Ok::<_, ErrorResponse>(TimestampedResponse { data, last_updated })
        })
        .await?;

    Ok((StatusCode::OK, Json(response)))
}
