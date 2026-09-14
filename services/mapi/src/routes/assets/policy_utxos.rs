use crate::{
    responses::{AssetInPolicy, ErrorResponse, PaginatedResponse, PolicyUtxo},
    utils::{self, internal_server_error, internal_server_error_str, ParsedSlotPageParams},
    CountParam, MapiExtension, OrderParam, SlotPagination,
};
use axum::{
    extract::{Path, Query},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Extension, Json,
};
use tikv_client::KvPair;
use timbre::{
    encoding::{
        decode::{
            decode_key, decode_utxos_by_address_cursor, decode_utxos_by_policy_value,
            DecodedAddress,
        },
        encode::encode_address_utxos_cursor,
    },
    Key, ReducerKey, UtxosByPolicyKey,
};

#[utoipa::path(
    tag = "Asset Policy",
    get,
    path = "/policy/{policy}/utxos",
    params(
        ("policy" = String, Path, description = "Hex encoded policy ID"),

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
            description = "Returns UTxOs which contain assets of the given policy ID, with the asset names and amounts",
            body = PaginatedPolicyUtxo,
            example = json!({
                "data": [
                    {
                        "tx_hash": "eeb9fb2af9c6da4a06c27c1021c4f138ec6e02857c6181cee79599dec6c96404",
                        "index": 0,
                        "slot": 12722572,
                        "address": "addr_test1vpxa84r650wsmcm887kkwar5y7ek5yyrjvgfqd8a608s2vsdnutyy",
                        "assets": [
                            {
                                "name": "6362544843",
                                "amount": 100000000000000i64
                            }
                        ]
                    },
                    {
                        "tx_hash": "eeb9fb2af9c6da4a06c27c1021c4f138ec6e02857c6181cee79599dec6c96404",
                        "index": 1,
                        "slot": 12722572,
                        "address": "addr_test1vrjpwhg7vlhck0ph5q0dpnkqferx3ltf0aqvy4qzn9m5v2gcvqhm8",
                        "assets": [
                            {
                                "name": "6362544843",
                                "amount": 320000000000000i64
                            }
                        ]
                    }
                ],
                "last_updated": {
                    "timestamp": "2022-10-10 20:25:28",
                    "block_hash": "0e0cb28820d245261e693d950c19f782fd90c63d03df5cdfde4efbfd2ac1365a",
                    "block_slot": 32283585
                },
                "next_cursor": null
            })
        ),
        (status = 400, description = "Malformed query parameters"),
        (status = 500, description = "Internal server error"),
    )
)]
#[tracing::instrument(
    name = "POLICY_UTXOS",
    level = "info",
    skip(page_params, config, headers)
)]
/// UTxOs containing assets of specific policy
///
/// Returns UTxO references of UTxOs which contain some of at least one asset of the specified policy ID, each paired with a list of assets of the policy contained in the UTxO and the corresponding amounts
pub async fn policy_utxos(
    headers: HeaderMap,
    page_params: Query<SlotPagination>,
    Path(policy): Path<String>,
    Extension(config): MapiExtension,
) -> Result<impl IntoResponse, ErrorResponse> {
    let chain = config.chain_info;
    let polyphony = config.polyphony_wrapper;
    let encoder = polyphony.utxos_by_policy_encoder()?;

    // -- parse and try decode user params

    let policy = utils::decode_policy_id(&policy)?;

    let ParsedSlotPageParams {
        count,
        order,
        lower,
        upper,
    } = utils::parse_slot_page_params(page_params.0, decode_utxos_by_address_cursor)?; // TODO RENAME

    // --- start db at most recent timestamp

    let mut txn = polyphony.begin_snapshot_latest().await?;

    // --- fetch cursor for last updated ---

    let last_updated = utils::get_last_updated(&mut txn, &encoder, &chain).await?;

    // --- initialise `next_cursor`

    let mut next_cursor = None;

    // --- scan keys for this page (max count plus one to see if there is another page)

    let range = encoder.encode_policy_utxos_range(&policy, lower, upper);

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

    // --- process fetched keys into response data

    let mut kvs = kvs.into_iter().enumerate();

    let mut utxos = Vec::new();

    while let Some((i, KvPair(k, v))) = kvs.next() {
        // TODO try_decode_X_keyxw or something
        let decoded_key = decode_key(&Into::<Vec<u8>>::into(k));

        let (tx_hash, index, slot) =
            if let Key::Reducer((_, _, ReducerKey::PolicyUtxos(key @ UtxosByPolicyKey { .. }))) =
                decoded_key
            {
                (key.utxo_hash, key.utxo_index, key.slot)
            } else {
                return Err(internal_server_error_str(
                    "unexpected key scanned: {decoded:?}",
                ));
            };

        // this is the last result of the page, check if there is a subsequent
        // result (and therefore we need to return a cursor for next page)
        if i == (count - 1) && kvs.next().is_some() {
            next_cursor = Some(encode_address_utxos_cursor(slot, tx_hash, index));
        }

        let (address, assets_list) = decode_utxos_by_policy_value(&v);

        // TODO Behaviour around the one or two giant addresses that were seen?
        let address = match address {
            DecodedAddress::Address(a) => a.to_string(),
            _ => "".to_string(),
        };

        let mut assets = Vec::new();

        for (name, quantity) in assets_list {
            assets.push(AssetInPolicy {
                name: hex::encode(name),
                amount: utils::u64_str_conv(quantity, &headers),
            })
        }

        utxos.push(PolicyUtxo {
            tx_hash: hex::encode(tx_hash),
            index: index as usize,
            slot,
            address,
            assets,
        })
    }

    let out = PaginatedResponse {
        data: utxos,
        last_updated,
        next_cursor,
    };

    Ok((StatusCode::OK, Json(out)))
}
