use std::collections::HashMap;

use axum::{http::StatusCode, response::IntoResponse, Extension, Json};
use pallas::codec::minicbor;
use pallas::ledger::primitives::babbage::TransactionInput;
use pallas::ledger::primitives::conway;
use pallas::ledger::primitives::conway::CostModels;
use pallas::ledger::primitives::Fragment;
use pallas::ledger::traverse::{Era, MultiEraInput, MultiEraOutput, MultiEraTx};
use serde::{Deserialize, Serialize};
use uplc::tx::eval_phase_two_raw;
use utoipa::ToSchema;

use crate::responses::{
    ErrorResponse, EvaluatedRedeemer, ExUnits, ProtocolParametersV6, RedeemerTag,
};
use crate::utils::{bad_request, internal_server_error, map_ogmios_error};
use crate::{ogmios_v6, MapiConfig, MapiExtension};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
/// UTxO which may not exist on-chain yet but is used in the transaction inputs or reference inputs
pub struct AdditionalUtxo {
    /// UTxO transaction hash
    pub tx_hash: String,
    /// UTxO transaction index
    pub index: u64,
    /// CBOR encoding of the UTxO
    pub txout_cbor: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct EvaluateRequest {
    /// Transaction CBOR
    pub cbor: String,
    pub additional_utxos: Option<Vec<AdditionalUtxo>>,
}

static PLUTUS_V3_COST_MODEL: [i64; 251] = [
    100788, 420, 1, 1, 1000, 173, 0, 1, 1000, 59957, 4, 1, 11183, 32, 201305, 8356, 4, 16000, 100,
    16000, 100, 16000, 100, 16000, 100, 16000, 100, 16000, 100, 100, 100, 16000, 100, 94375, 32,
    132994, 32, 61462, 4, 72010, 178, 0, 1, 22151, 32, 91189, 769, 4, 2, 85848, 123203, 7305, -900,
    1716, 549, 57, 85848, 0, 1, 1, 1000, 42921, 4, 2, 24548, 29498, 38, 1, 898148, 27279, 1, 51775,
    558, 1, 39184, 1000, 60594, 1, 141895, 32, 83150, 32, 15299, 32, 76049, 1, 13169, 4, 22100, 10,
    28999, 74, 1, 28999, 74, 1, 43285, 552, 1, 44749, 541, 1, 33852, 32, 68246, 32, 72362, 32,
    7243, 32, 7391, 32, 11546, 32, 85848, 123203, 7305, -900, 1716, 549, 57, 85848, 0, 1, 90434,
    519, 0, 1, 74433, 32, 85848, 123203, 7305, -900, 1716, 549, 57, 85848, 0, 1, 1, 85848, 123203,
    7305, -900, 1716, 549, 57, 85848, 0, 1, 955506, 213312, 0, 2, 270652, 22588, 4, 1457325, 64566,
    4, 20467, 1, 4, 0, 141992, 32, 100788, 420, 1, 1, 81663, 32, 59498, 32, 20142, 32, 24588, 32,
    20744, 32, 25933, 32, 24623, 32, 43053543, 10, 53384111, 14333, 10, 43574283, 26308, 10, 16000,
    100, 16000, 100, 962335, 18, 2780678, 6, 442008, 1, 52538055, 3756, 18, 267929, 18, 76433006,
    8868, 18, 52948122, 18, 1995836, 36, 3227919, 12, 901022, 1, 166917843, 4307, 36, 284546, 36,
    158221314, 26549, 36, 74698472, 36, 333849714, 1, 254006273, 72, 2174038, 72, 2261318, 64571,
    4, 207616, 8310, 4, 1293828, 28716, 63, 0, 1, 1006041, 43623, 251, 0, 1,
];

// (steps, mem)
static MAX_TX_EX_UNITS: (u64, u64) = (10000000000, 14000000);

#[utoipa::path(
    tag = "Transactions",
    post,
    path = "/transactions/evaluate",
    request_body(
        content = EvaluateRequest,
        examples(
            ("Without Additional UTxOs" = (
                summary = "Transaction evaluation request without additional UTxOs",
                value = json!({
                    "cbor": "84a80084825820649748a242393deac81d6f88a2b...8176567946693a204c50204f726465722050726f63657373"
                }))
            ),
            ("With Additional UTxOs" = (
                summary = "Transaction evaluation request with additional input UTxOs",
                value = json!({
                    "cbor": "84a80084825820649748a242393deac81d6f88a2b7edfec28b...946693a204c50204f726465722050726f63657373",
                    "additional_utxos": [
                        {
                            "tx_hash": "649748a242393deac81d6f88a2b7edfec28b1966d1d88b7f0a36e2d8c60ef9e9",
                            "index": 0,
                            "txout_cbor": "83581d713422c13d5a97e68fe725d3d740b7f96cb315e3b44d03a2be28863beb821a0ae72e72a1581c804f5544c1962a40546827cab750a88404dc7108c0f588b72964754fa144565946491a0e9b3eda58205b90f5d273e2d7f7db45dc4f0efdcfe7536250a9abf18dee57abee9c798944c7"
                        }
                    ]
                }))
            ),
        )
    ),
    responses(
        (
            status = 200,
            description = "Details of executed redeemers",
            body = [EvaluatedRedeemer],
            example = json!([
                {
                    "redeemer_tag": "spend",
                    "redeemer_index": 0,
                    "ex_units": {
                        "mem": 426418,
                        "steps": 125361025
                    }
                },
                {
                    "redeemer_tag": "spend",
                    "redeemer_index": 1,
                    "ex_units": {
                        "mem": 385832,
                        "steps": 113607371
                    }
                },
                {
                    "redeemer_tag": "mint",
                    "redeemer_index": 0,
                    "ex_units": {
                        "mem": 388722,
                        "steps": 111814261
                    }
                }
            ])
        ),
        (status = 400, description = "Malformed query parameters"),
        (status = 404, description = "No results found"),
        (status = 500, description = "Internal server error"),
        (status = 503, description = "ogmios temporarily unavailable, retry"),
        (status = 504, description = "ogmios query timed out"),
    )
)]
#[tracing::instrument(name = "EVALUATE_REDEEMERS", level = "info", skip(config, payload))]
/// Evaluate redeemers of a transaction
///
/// Executes the redeemers of a transaction in order to compute how many execution units are needed for each, without submitting the transaction to the chain and without requiring the transaction to be fully-valid.
///
/// Useful during transaction building to compute what budget should be used for each redeemer.
///
/// Note that all transaction inputs and reference inputs must be able to be resolved (we must be able to find the contents of that UTxO by finding the UTxO on-chain) in order to evaluate the transaction. If your transaction contains any inputs which may not be found on-chain at the time of evaluation, for example if you are transaction chaining, then you must pass in these inputs and their corresponding transaction output bytes as additional UTxOs.
pub async fn evaluate_redeemers(
    Extension(config): MapiExtension,
    Json(payload): Json<EvaluateRequest>,
) -> Result<impl IntoResponse, ErrorResponse> {
    let cost_models = get_ogmios_cost_models(&config).await?;

    let chain = config.chain_info;
    let polyphony = config.polyphony_wrapper;

    // --- parse user provided args

    let cbor = hex::decode(payload.cbor)
        .map_err(|_| bad_request("Transaction must be hex-encoded CBOR"))?;

    let tx = MultiEraTx::decode(&cbor).map_err(internal_server_error)?;

    // ---

    let mut txn = polyphony.begin_snapshot_latest().await?;

    // --- resolve input utxos

    let mut resolved = HashMap::new();
    let mut not_resolved = vec![];

    // try resolve the inputs and reference inputs by fetching the relevant txs
    for txo_ref in tx.inputs().into_iter().chain(tx.reference_inputs()) {
        let key = polyphony
            .tx_by_hash_encoder()?
            .encode_tx_by_hash_key(txo_ref.hash());

        let cbor = if let Some(b) = txn.get(key).await.map_err(internal_server_error)? {
            b
        } else {
            not_resolved.push(txo_ref.output_ref());
            continue;
        };

        let producing_tx = MultiEraTx::decode(&cbor).map_err(internal_server_error)?;

        if let Some(txo) = producing_tx.output_at(txo_ref.index() as usize) {
            let input_bytes = match txo_ref {
                MultiEraInput::AlonzoCompatible(x) => {
                    minicbor::to_vec(x).map_err(internal_server_error)?
                }
                MultiEraInput::Byron(x) => minicbor::to_vec(x).map_err(internal_server_error)?,
                _ => return Err(bad_request("unsupported transaction input era")),
            };

            resolved.insert(input_bytes, txo.clone().encode());
        }
    }

    // insert any additional utxos into the resolved inputs map
    for additional_utxo in payload.additional_utxos.unwrap_or_default() {
        let tx_hash: [u8; 32] = match hex::decode(additional_utxo.tx_hash) {
            Ok(b) => b
                .try_into()
                .map_err(|_| bad_request("Malformed transaction hash"))?,
            Err(_) => return Err(bad_request("Transaction hash must be hex encoded")),
        };

        let input = TransactionInput {
            transaction_id: tx_hash.into(),
            index: additional_utxo.index,
        };

        let txo_cbor = hex::decode(additional_utxo.txout_cbor)
            .map_err(|_| bad_request("Additional UTxO CBOR must be hex-encoded"))?;

        let _ = MultiEraOutput::decode(Era::Conway, &txo_cbor)
            .or_else(|_| MultiEraOutput::decode(Era::Babbage, &txo_cbor))
            .or_else(|_| MultiEraOutput::decode(Era::Alonzo, &txo_cbor))
            .or_else(|_| MultiEraOutput::decode(Era::Byron, &txo_cbor))
            .map_err(|_| bad_request("Malformed additional UTxO"))?;

        not_resolved.retain(|x| *x.hash() != input.transaction_id || x.index() != input.index);

        resolved.insert(minicbor::to_vec(input).unwrap(), txo_cbor);
    }

    if !not_resolved.is_empty() {
        return Err(bad_request(
            format!("Could not resolve input {}", not_resolved[0]).as_str(),
        ));
    }

    // --- evaluate

    let resolved_inputs = resolved.into_iter().collect::<Vec<(Vec<u8>, Vec<u8>)>>();

    let slot_config = (
        chain.shelley_known_time * 1000,
        chain.shelley_known_slot,
        chain.shelley_slot_length * 1000,
    );

    let result = eval_phase_two_raw(
        &cbor,
        &resolved_inputs,
        Some(&cost_models.encode_fragment().unwrap()),
        MAX_TX_EX_UNITS,
        slot_config,
        false,
        |_| (),
    );

    let rdmrs = match result {
        Ok(rdmr_bytes) => rdmr_bytes
            .into_iter()
            .map(|b| conway::Redeemer::decode_fragment(&b).unwrap())
            .collect::<Vec<_>>(),
        Err(uplc::tx::error::Error::RedeemerError { tag, index, err }) => {
            if let uplc::tx::error::Error::Machine(
                uplc::machine::Error::EvaluationFailure,
                _,
                strs,
            ) = *err
            {
                return Err(bad_request(format!("A validator threw an error while executing redeemer {}:{index}. The following information was returned: '{}'", tag.to_ascii_lowercase(), strs.join(", ")).as_str()));
            } else {
                return Err(bad_request(
                    format!("Transaction evaluation failed: {:?}", err).as_str(),
                ));
            }
        }
        Err(e) => {
            return Err(bad_request(
                format!("Transaction evaluation failed: {:?}", e).as_str(),
            ))
        }
    };

    // --- build response data

    let mut out = vec![];

    for rdmr in rdmrs {
        let redeemer_tag = match rdmr.tag {
            conway::RedeemerTag::Spend => RedeemerTag::Spend,
            conway::RedeemerTag::Mint => RedeemerTag::Mint,
            conway::RedeemerTag::Cert => RedeemerTag::Cert,
            conway::RedeemerTag::Reward => RedeemerTag::Wdrl,
            conway::RedeemerTag::Vote => RedeemerTag::Vote,
            conway::RedeemerTag::Propose => RedeemerTag::Propose,
        };

        out.push(EvaluatedRedeemer {
            redeemer_tag,
            redeemer_index: rdmr.index as usize,
            ex_units: ExUnits {
                mem: rdmr.ex_units.mem,
                steps: rdmr.ex_units.steps,
            },
        })
    }

    Ok((StatusCode::OK, Json(out)))
}

async fn get_ogmios_cost_models(config: &MapiConfig) -> Result<CostModels, ErrorResponse> {
    // Evaluate is the hottest cost-models consumer, so it gets its own single-flight cache to
    // close the stampede vector — but fed straight from Ogmios. Cost models come from the
    // ledger, not dbsync; reusing `protocol_parameters_cache` would have dragged in its
    // `last_updated` dbsync query (which evaluate never reads), making evaluation fail
    // whenever dbsync is down.
    config
        .cost_models_cache
        .get_or_refresh(|| async {
            let params: ProtocolParametersV6 = ogmios_v6::query(
                &config.ogmios_v6_url,
                "QueryLedgerStateProtocolParameters",
                "queryLedgerState/protocolParameters",
                &config.ogmios_query_opts,
            )
            .await
            .map_err(map_ogmios_error)?;

            Ok::<_, ErrorResponse>(CostModels {
                plutus_v1: Some(params.plutus_cost_models.plutus_v1),
                plutus_v2: Some(params.plutus_cost_models.plutus_v2),
                plutus_v3: params
                    .plutus_cost_models
                    .plutus_v3
                    .or(Some(PLUTUS_V3_COST_MODEL.to_vec())),
                unknown: Default::default(),
            })
        })
        .await
}
