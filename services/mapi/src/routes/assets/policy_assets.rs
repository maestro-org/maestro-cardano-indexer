use crate::{
    responses::{AssetInfoConcise, AssetStandards, ErrorResponse, PaginatedResponse},
    utils::{self, internal_server_error, scan_extended, ParsedCursorPageParams},
    CountParam, CursorPagination, MapiExtension, OrderParam,
};
use axum::{
    extract::{Path, Query},
    http::StatusCode,
    response::IntoResponse,
    Extension, Json,
};
use tikv_client::KvPair;
use timbre::encoding::*;

#[utoipa::path(
    tag = "Asset Policy",
    get,
    path = "/policy/{policy}/assets",
    params(
        ("policy" = String, Path, description = "Hex encoded Policy ID"),
        ("count" = inline(Option<CountParam>), Query, description = "The max number of results per page"),
        ("cursor" = inline(Option<String>), Query, description = "Pagination cursor string, use the cursor included in a page of results to fetch the next page"),
    ),
    responses(
        (
            status = 200,
            description = "List of assets paired with short information summary",
            body = PaginatedAssetInfoConcise,
            example = json!({
                "data": [{
                    "asset_name": "6164616d616e74",
                    "asset_name_ascii": "adamant",
                    "fingerprint": "asset105lxc60yjpqygjsnn29e5hjyafnfw75pqwwza2",
                    "total_supply": "1",
                    "asset_standards": {
                        "cip25_metadata": {
                            "augmentations": [],
                            "core": {
                                "handleEncoding": "utf-8",
                                "og": 0,
                                "prefix": "$",
                                "termsofuse": "https://adahandle.com/tou",
                                "version": 0
                            },
                            "description": "The Handle Standard",
                            "image": "ipfs://Qmai8iwE1Diw5YZVYSpSPAbXM5AVprheve8AAXJzwXoftf",
                            "name": "$adamant",
                            "website": "https://adahandle.com"
                        },
                        "cip68_metadata": null
                    }
                }, {
                    "asset_name": "6264736d",
                    "asset_name_ascii": "bdsm",
                    "fingerprint": "asset1qyv6sdhq0fw7mxt0zl7gwd2h2hs4evpe0zl73r",
                    "total_supply": "1",
                    "asset_standards": {
                        "cip25_metadata": {
                            "augmentations": [],
                            "core": {
                                "handleEncoding": "utf-8",
                                "og": 0,
                                "prefix": "$",
                                "termsofuse": "https://adahandle.com/tou",
                                "version": 0
                            },
                            "description": "The Handle Standard",
                            "image": "ipfs://QmfTpy3ybWL1teCMVAieh7atHtYgpcCZ5Ew58eSUxHgZFb",
                            "name": "$bdsm",
                            "website": "https://adahandle.com"
                        },
                        "cip68_metadata": null
                    }
                }],
                "last_updated": {
                    "timestamp": "2023-10-18 07:30:52",
                    "block_hash": "0e1e924710135acfe200ab13d290bd282a67584fd54456f0dcac0aeaa38bb2c2",
                    "block_slot": 106047961
                },
                "next_cursor": "YmRzbQ"
            })
        ),
        (status = 400, description = "Malformed query parameters"),
        (status = 500, description = "Internal server error"),
    )
)]
#[tracing::instrument(name = "POLICY_ASSETS", level = "info", skip(page_params, config))]
/// List assets of a policy
///
/// Lists all assets which have existed under the specified policy ID with a short information summary for each
pub async fn policy_assets(
    page_params: Query<CursorPagination>,
    Path(policy): Path<String>,
    Extension(config): MapiExtension,
) -> Result<impl IntoResponse, ErrorResponse> {
    let chain = config.chain_info;
    let polyphony = config.polyphony_wrapper;
    let encoder = polyphony.supply_by_asset_encoder()?;

    // -- parse and try decode user params

    let policy = utils::decode_policy_id(&policy)?;

    let ParsedCursorPageParams { count, cursor } =
        utils::parse_cursor_page_params(page_params.0, decode_supply_by_asset_cursor)?;

    // --- initialise `next_cursor`

    let mut next_cursor = None;

    // --- start db at most recent timestamp

    let mut txn = polyphony.begin_snapshot_latest().await?;

    // --- fetch cursor for last updated ---

    let last_updated = utils::get_last_updated(&mut txn, &encoder, &chain).await?;

    // --- scan keys for this page (max count plus one to see if there is another page)

    let lower = cursor.map(RangeBound::Cursor);

    let range = encoder.encode_supply_by_asset_range(&policy, lower, None);

    let kvs = scan_extended(
        &mut txn,
        range,
        OrderParam::Asc,
        None::<fn(_) -> bool>,
        Some(count + 1),
        None,
    )
    .await?;

    // --- parse fetched kvs

    let mut kvs = kvs.into_iter().enumerate();

    let mut assets = Vec::new();

    while let Some((i, KvPair(k, v))) = kvs.next() {
        let k = Into::<Vec<u8>>::into(k);
        let key = decode_supply_by_asset_key(&k);

        // if this is the last result of the page, check if there is a subsequent
        // result (and therefore we need to return a cursor for next page)
        if i == (count - 1) && kvs.next().is_some() {
            next_cursor = Some(encode_supply_by_asset_cursor(key.asset_name.clone()));
        }

        let value_bytes: [u8; 16] = v[..16].try_into().map_err(internal_server_error)?;

        assets.push((
            (key.policy, key.asset_name),
            u128::from_be_bytes(value_bytes),
        ))
    }

    // --- cip25

    let mut cip25_resolver = utils::fetch_cip25_metadata(
        &mut txn,
        &polyphony,
        assets.clone().into_iter().map(|(x, _)| x).collect(),
    )
    .await?;

    // --- cip68

    let encoder = polyphony.utxos_by_policy_encoder()?;
    let mut txn = polyphony.begin_snapshot_latest().await?;
    let range = encoder.encode_policy_utxos_range(&policy, None, None);

    let utxo_kvs = utils::batch_scan_entire_range(&mut txn, range).await?;

    // --- build response data

    let mut asset_infos = vec![];

    for ((policy, name), total_supply) in assets {
        let asset_name = hex::encode(&name);
        let asset_name_ascii = String::from_utf8_lossy(&name).to_string();
        let fingerprint = utils::asset_fingerprint(policy, &name);

        let asset_standards = AssetStandards {
            cip25_metadata: cip25_resolver.remove(&(policy, name.clone())),
            cip68_metadata: utils::try_parse_cip68_metadata_from_policy_utxos(
                &polyphony, &utxo_kvs, name,
            )
            .await?,
        };

        asset_infos.push(AssetInfoConcise {
            asset_name,
            asset_name_ascii,
            fingerprint,
            total_supply: total_supply.to_string(),
            asset_standards,
        })
    }

    // ---

    let out = PaginatedResponse {
        data: asset_infos,
        last_updated,
        next_cursor,
    };

    Ok((StatusCode::OK, Json(out)))
}
