use axum::{extract::Query, http::StatusCode, response::IntoResponse, Extension, Json};
use pallas::ledger::addresses::ShelleyPaymentPart;
use tikv_client::KvPair;
use timbre::encoding::{
    decode_txs_by_payment_cred_cursor, decode_txs_by_payment_cred_key,
    encode_txs_by_payment_cred_cursor,
};

use crate::responses::PaymentCredentialsTransaction;
use crate::utils::{scan_extended, ParsedSlotPageParams};
use crate::{
    responses::{ErrorResponse, PaginatedResponse},
    utils::{self, bad_request},
    CountParam, MapiExtension,
};
use crate::{OrderParam, SlotPagination};

#[utoipa::path(
    tag = "Addresses",
    post,
    path = "/addresses/cred/transactions",
    params(
        ("count" = inline(Option<CountParam>), Query, description = "The max number of results per page"),

        ("order" = inline(Option<OrderParam>), Query, description = "The order in which the results are sorted (by point in chain)"),
        ("from" = inline(Option<u64>), Query, description = "Return only transactions minted on or after a specific slot"),
        ("to" = inline(Option<u64>), Query, description = "Return only transactions minted on or before a specific slot"),

        ("cursor" = inline(Option<String>), Query, description = "Pagination cursor string, use the cursor included in a page of results to fetch the next page"),
    ),
    request_body(content = [String], example = json!(["addr_vkh1wdkle2sprqsuklt34474g6n4ps7k6pv6zwe4644uxmg7xj54y87", "addr_shared_vkh1ewj7sycvy5y234m3uhudn5dggqk3djr0jheacr3utna5gcnmwp2"])),
    responses(
        (
            status = 200,
            description = "Transactions involving payment credentials",
            body = PaginatedPaymentCredentialsTransaction,
            example = json!({
                "data": [
                    {
                        "tx_hash": "6029360e8236d06d81a31e9d8c6748ada97817f227204be092c93ad04fd04e4c",
                        "slot": 55718892
                    },
                    {
                        "tx_hash": "21c895e07df0af0d4bfad175f823b047a61542352890ffdd23bd23d56f567d2d",
                        "slot": 55718892
                    },
                    {
                        "tx_hash": "257916e7ae112cf16f27218e41bfa37c018bab922354201b3b38c9a24c35ab33",
                        "slot": 55718892
                    },
                    {
                        "tx_hash": "c140d6590778da752bf9b33780efdb9dc6307aa0bfc107f95f503016b58aa218",
                        "slot": 106047961
                    }
                ],
                "last_updated": {
                    "timestamp": "2023-10-18 07:30:52",
                    "block_hash": "0e1e924710135acfe200ab13d290bd282a67584fd54456f0dcac0aeaa38bb2c2",
                    "block_slot": 106047961
                },
                "next_cursor": null
            })
        ),
        (status = 400, description = "Malformed query parameters"),
        (status = 500, description = "Internal server error"),
    )
)]
#[tracing::instrument(name = "TXS_BY_PAYMENT_CREDS", level = "info", skip(config))]
/// Transactions by multiple payment credentials
///
/// Returns transactions in which at least one of a list specified payment credentials spent or received funds, or was a required signer.
///
/// Specifically, "spent or received funds" meaning: the payment credential was used in an address which controlled at least one of the transaction inputs and/or receives one of the outputs AND the transaction is phase-2 valid, OR, the address controlled at least one of the collateral inputs and/or receives the collateral return output AND the transaction is phase-2 invalid. [Read more](https://docs.cardano.org/plutus/collateral-mechanism/).
pub async fn txs_by_payment_creds(
    page_params: Query<SlotPagination>,
    Extension(config): MapiExtension,
    Json(payload): Json<Vec<String>>,
) -> Result<impl IntoResponse, ErrorResponse> {
    let polyphony = config.polyphony_wrapper;
    let chain = config.chain_info;

    let encoder = polyphony.txs_by_pay_cred_encoder()?;

    // -- parse and try decode user params

    let mut creds: Vec<String> = payload;

    if creds.len() > 100 {
        return Err(bad_request("Payload size must not exceed 100 items"));
    }

    creds.sort();
    creds.dedup();

    let creds = creds
        .iter()
        .map(|cred: &std::string::String| utils::decode_payment_credential(cred))
        .collect::<Result<Vec<ShelleyPaymentPart>, _>>()?;

    let ParsedSlotPageParams {
        count,
        order,
        lower,
        upper,
    } = utils::parse_slot_page_params(page_params.0, decode_txs_by_payment_cred_cursor)?;

    // --- start db snapshot at most recent timestamp

    let mut txn = polyphony.begin_snapshot_latest().await?;

    // --- fetch cursor for last updated ---

    let last_updated = utils::get_last_updated(&mut txn, &encoder, &chain).await?;

    // --- first collect all the required kvs

    let mut combined = Vec::new();

    for cred in creds {
        let range = encoder.encode_txs_by_payment_cred_range(&cred, lower.clone(), upper.clone());

        for KvPair(k, _) in scan_extended(
            &mut txn,
            range,
            order,
            None::<fn(_) -> bool>,
            Some(count + 1),
            None,
        )
        .await?
        {
            let key = decode_txs_by_payment_cred_key((&k).into());

            combined.push(((key.slot, key.block_index), key.tx_hash))
        }
    }

    combined.sort_unstable_by_key(|(x, _)| *x);
    combined.dedup();

    if order == OrderParam::Desc {
        combined.reverse()
    }

    let next_cursor = if combined.len() > count {
        let last = combined[count - 1];

        Some(encode_txs_by_payment_cred_cursor(
            last.0 .0, last.0 .1, last.1,
        ))
    } else {
        None
    };

    combined.truncate(count);

    // --- craft response data

    let txs = combined
        .into_iter()
        .map(|((slot, _), tx_hash)| PaymentCredentialsTransaction {
            slot,
            tx_hash: hex::encode(tx_hash),
        })
        .collect::<Vec<_>>();

    // ---

    let out = PaginatedResponse {
        data: txs,
        last_updated,
        next_cursor,
    };

    Ok((StatusCode::OK, Json(out)))
}
