use crate::{
    responses::{ErrorResponse, PaginatedResponse, PaymentCredentialTransaction},
    utils::{self, internal_server_error, ParsedSlotPageParams},
    CountParam, OrderParam, SlotPagination,
};
use axum::{
    extract::{Path, Query},
    http::StatusCode,
    response::IntoResponse,
    Extension, Json,
};
use tikv_client::KvPair;
use timbre::encoding::{
    decode_txs_by_payment_cred_cursor, decode_txs_by_payment_cred_key,
    decode_txs_by_payment_cred_value, encode_txs_by_payment_cred_cursor, TxsByPayCredKey,
    TxsByPayCredValue,
};

use crate::MapiExtension;

#[utoipa::path(
    tag = "Addresses",
    get,
    path = "/addresses/cred/{credential}/transactions",
    params(
        ("credential" = String, Path, description = "Payment credential in bech32 format"),

        ("count" = inline(Option<CountParam>), Query, description = "The max number of results per page"),

        ("order" = inline(Option<OrderParam>), Query, description = "The order in which the results are sorted (by point in chain)"),
        ("from" = inline(Option<u64>), Query, description = "Return only transactions minted on or after a specific slot"),
        ("to" = inline(Option<u64>), Query, description = "Return only transactions minted on or before a specific slot"),

        ("cursor" = inline(Option<String>), Query, description = "Pagination cursor string, use the cursor included in a page of results to fetch the next page"),
    ),
    responses(
        (
            status = 200,
            description = "Get the transactions for an address",
            body = PaginatedPaymentCredentialTransaction,
            example = json!({
                "data": [{
                    "tx_hash": "31a84c3c6200bec2498b18c42f882fa690cd0d32a9c84a2019eb5cc42f5971d0",
                    "slot": 73867237,
                    "input": false,
                    "output": true,
                    "required_signer": false
                }, {
                    "tx_hash": "357a18477c72d771df77736f83abba44fa826a3e36bc31bb81ca4e2f06475cc6",
                    "slot": 73867237,
                    "input": false,
                    "output": true,
                    "required_signer": false
                }],
                "last_updated": {
                    "timestamp": "2022-10-10 20:25:28",
                    "block_hash": "bdf4630098ac6923e0ef1ca0b0d6e00dca96790a8c3dd670948fdfe1456798d2",
                    "block_slot": 73867237
                },
                "next_cursor": null
            })
        ),
        (status = 400, description = "Malformed query parameters"),
        (status = 500, description = "Internal server error"),
    )
)]
#[tracing::instrument(name = "TXS_BY_PAYMENT_CRED", level = "info", skip(config))]
/// Payment credential transactions
///
/// Returns transactions in which the specified payment credential spent or received funds, or was a required signer.
///
/// Specifically, "spent or received funds" meaning: the payment credential was used in an address which controlled at least one of the transaction inputs and/or receives one of the outputs AND the transaction is phase-2 valid, OR, the address controlled at least one of the collateral inputs and/or receives the collateral return output AND the transaction is phase-2 invalid. [Read more](https://docs.cardano.org/plutus/collateral-mechanism/).
pub async fn txs_by_payment_cred(
    page_params: Query<SlotPagination>,
    Path(credential): Path<String>,
    Extension(config): MapiExtension,
) -> Result<impl IntoResponse, ErrorResponse> {
    let polyphony = config.polyphony_wrapper;
    let chain = config.chain_info;

    let encoder = polyphony.txs_by_pay_cred_encoder()?;

    // -- parse and try decode user params

    let payment_cred = utils::decode_payment_credential(&credential)?;

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

    // --- initialise `next_cursor`

    let mut next_cursor = None;

    // --- scan keys for this page (max count plus one to see if there is another page)

    let range = encoder.encode_txs_by_payment_cred_range(&payment_cred, lower, upper);

    // TODO global tikv wrapper with helpers
    let kvs: Vec<KvPair> = match order {
        OrderParam::Asc => txn
            .scan(range, (count + 1) as u32)
            .await
            .map_err(internal_server_error)?
            .collect(),
        OrderParam::Desc => txn
            .scan_reverse(range, (count + 1) as u32)
            .await
            .map_err(internal_server_error)?
            .collect(),
    };

    // --- process fetched kvs

    let mut kvs = kvs.iter().enumerate();
    let mut txs = Vec::new();

    while let Some((i, KvPair(k, v))) = kvs.next() {
        let TxsByPayCredKey {
            slot,
            block_index,
            tx_hash,
            ..
        } = decode_txs_by_payment_cred_key(k.into());
        let TxsByPayCredValue {
            input,
            output,
            required_signer,
        } = decode_txs_by_payment_cred_value(v[0]);

        txs.push(PaymentCredentialTransaction {
            tx_hash: hex::encode(tx_hash),
            slot,
            input,
            output,
            required_signer,
        });

        // if this is the last result of the page, check if there is a subsequent
        // result (and therefore we need to return a cursor for next page)
        if i == (count - 1) && kvs.next().is_some() {
            next_cursor = Some(encode_txs_by_payment_cred_cursor(
                slot,
                block_index,
                tx_hash,
            ));
        }
    }

    // ---

    let out = PaginatedResponse {
        data: txs,
        last_updated,
        next_cursor,
    };

    Ok((StatusCode::OK, Json(out)))
}
