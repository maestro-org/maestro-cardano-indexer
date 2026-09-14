use crate::{
    responses::{AssetUtxo, ErrorResponse, PaginatedResponse},
    utils::{
        self, empty_string_as_none, internal_server_error_str, scan_extended, ParsedSlotPageParams,
    },
    CountParam, MapiExtension, OrderParam, SlotPagination,
};
use axum::{
    extract::{Path, Query},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Extension, Json,
};
use pallas::ledger::addresses::Address;
use serde::Deserialize;
use tikv_client::KvPair;
use timbre::encoding::{
    decode::{
        decode_utxos_by_address_cursor, decode_utxos_by_asset_key, decode_utxos_by_asset_value,
        DecodedAddress,
    },
    encode::encode_address_utxos_cursor,
};

#[derive(Debug, Deserialize)]
pub struct Params {
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub address: Option<String>,
}

#[utoipa::path(
    tag = "Assets",
    get,
    path = "/assets/{asset}/utxos",
    params(
        ("asset" = String, Path, description = "Asset, encoded as concatenation of hex of policy ID and asset name"),

        ("address" = inline(Option<String>), Query, description = "Return only UTxOs controlled by a specific address (bech32 encoding)"),

        ("count" = inline(Option<CountParam>), Query, description = "The max number of results per page"),

        ("order" = inline(Option<OrderParam>), Query, description = "The order in which the results are sorted (by slot at which UTxO was produced)"),
        ("from" = inline(Option<u64>), Query, description = "Return only UTxOs created on or after a specific slot"),
        ("to" = inline(Option<u64>), Query, description = "Return only UTxOs created before a specific slot"),

        ("cursor" = inline(Option<String>), Query, description = "Pagination cursor string, use the cursor included in a page of results to fetch the next page"),

        ("amounts-as-strings" = Option<String>, Header, description = "Large numbers returned as strings if set to `true`")
    ),
    responses(
        (
            status = 200,
            description = "Returns UTxOs containing the specified asset, each paired with the amount of the asset",
            body = PaginatedAssetUtxo,
            example = json!({
                "data": [
                    {
                        "tx_hash": "eeb9fb2af9c6da4a06c27c1021c4f138ec6e02857c6181cee79599dec6c96404",
                        "index": 0,
                        "slot": 12722572,
                        "address": "addr_test1vpxa84r650wsmcm887kkwar5y7ek5yyrjvgfqd8a608s2vsdnutyy",
                        "amount": 100000000000000i64
                    },
                    {
                        "tx_hash": "eeb9fb2af9c6da4a06c27c1021c4f138ec6e02857c6181cee79599dec6c96404",
                        "index": 1,
                        "slot": 12722572,
                        "address": "addr_test1vrjpwhg7vlhck0ph5q0dpnkqferx3ltf0aqvy4qzn9m5v2gcvqhm8",
                        "amount": 320000000000000i64
                    }
                ],
                "last_updated": {
                    "timestamp": "2022-10-10 20:25:28",
                    "block_hash": "a819c4555f28885e8e9e90039b7dd8375bb88224414536adb2ee2615036d8d4c",
                    "block_slot": 32277176
                },
                "next_cursor": null
            })
        ),
        (status = 400, description = "Malformed query parameters"),
        (status = 500, description = "Internal server error"),
    )
)]
#[tracing::instrument(name = "ASSET_UTXOS", level = "info", skip(config, headers))]
/// Native asset UTxOs
///
/// Returns references for UTxOs which contain some of the specified asset, each paired with the amount of the asset contained in the UTxO
pub async fn asset_utxos(
    headers: HeaderMap,
    page_params: Query<SlotPagination>,
    params: Query<Params>,
    Path(asset): Path<String>,
    Extension(config): MapiExtension,
) -> Result<impl IntoResponse, ErrorResponse> {
    let chain = config.chain_info;
    let polyphony: crate::PolyphonyWrapper = config.polyphony_wrapper;
    let timeout = config.timeout_duration;

    let encoder = polyphony.utxos_by_asset_encoder()?;

    // -- parse and try decode user params

    let (policy, asset_name) = utils::decode_asset(&asset)?;

    let filter_address = params
        .0
        .address
        .map(|a| utils::decode_payment_address(&a))
        .transpose()?;

    let ParsedSlotPageParams {
        count,
        order,
        lower,
        upper,
    } = utils::parse_slot_page_params(page_params.0, decode_utxos_by_address_cursor)?;

    // --- start db at most recent timestamp

    let mut txn = polyphony.begin_snapshot_latest().await?;

    // --- fetch cursor for last updated

    let last_updated = utils::get_last_updated(&mut txn, &encoder, &chain).await?;

    // --- initialise `next_cursor`

    let mut next_cursor = None;

    // --- scan keys for this page (max count plus one to see if there is another page)

    let range = encoder.encode_asset_utxos_range(&policy, &asset_name, lower, upper);

    // TODO: cleaner way of creating filter closure
    // let f = filter_address.map(move |a| |KvPair(_, v)| asset_utxo_controlled_by_address(v, a.clone()));
    // let kvs = scan_extended(&mut txn, range, order, f, Some(count + 1)).await?;

    let kvs = if let Some(addr) = filter_address.clone() {
        scan_extended(
            &mut txn,
            range,
            order,
            Some(|KvPair(_, v)| asset_utxo_controlled_by_address(v, &addr)),
            Some(count + 1),
            Some(timeout),
        )
        .await?
    } else {
        scan_extended(
            &mut txn,
            range,
            order,
            None::<fn(_) -> bool>,
            Some(count + 1),
            None,
        )
        .await?
    };

    // --- process fetched kvs

    let mut kvs = kvs.iter().enumerate();

    let mut utxos: Vec<AssetUtxo> = Vec::new();

    while let Some((i, KvPair(k, v))) = kvs.next() {
        let key = decode_utxos_by_asset_key(k.into());

        let (slot, tx_hash, index) = (key.slot, key.utxo_hash, key.utxo_index);

        // if this is the last result of the page, check if there is a subsequent
        // result (and therefore we need to return a cursor for next page)
        if i == (count - 1) && kvs.next().is_some() {
            next_cursor = Some(encode_address_utxos_cursor(slot, tx_hash, index));
        }

        let (address, amount) = decode_utxos_by_asset_value(v);

        let address = match address {
            DecodedAddress::Address(a) => a.to_string(),
            // TODO
            DecodedAddress::Hash(h) => {
                return Err(internal_server_error_str(
                    format!("decoded address hash: {}", hex::encode(h)).as_str(),
                ))
            }
        };

        utxos.push(AssetUtxo {
            tx_hash: hex::encode(tx_hash),
            index,
            slot,
            address,
            amount: utils::u64_str_conv(amount, &headers),
        })
    }

    let out = PaginatedResponse {
        data: utxos,
        last_updated,
        next_cursor,
    };

    Ok((StatusCode::OK, Json(out)))
}

fn asset_utxo_controlled_by_address(v: Vec<u8>, filter_address: &Address) -> bool {
    let (address, _) = decode_utxos_by_asset_value(&v);

    if let DecodedAddress::Address(a) = address {
        a == filter_address.clone()
    } else {
        false
    }
}
