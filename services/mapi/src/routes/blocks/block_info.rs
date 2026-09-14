use crate::{
    responses::{
        BlockInfo, ErrorResponse, ExUnits, LedgerEra, OperationalCert, TimestampedResponse,
    },
    utils::{self, internal_server_error, internal_server_error_str, not_found, BlockId},
};
use axum::{
    extract::Path,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Extension, Json,
};
use bech32::{ToBase32, Variant};
use pallas::crypto::hash::Hasher;
use pallas::ledger::traverse::{Era, MultiEraHeader};
use timbre::encoding::decode::decode_block_by_height_value;

use crate::MapiExtension;

#[utoipa::path(
    tag = "Blocks",
    get,
    path = "/blocks/{hash_or_height}",
    params(
        ("hash_or_height" = String, Path, description = "Block height or hex encoded block hash"),

        ("amounts-as-strings" = Option<String>, Header, description = "Large numbers returned as strings if set to `true`")
    ),
    responses(
        (
            status = 200,
            description = "Summary of information regarding the specified block",
            body = TimestampedBlockInfo,
            example = json!({
                "data": {
                    "hash": "bdf4630098ac6923e0ef1ca0b0d6e00dca96790a8c3dd670948fdfe1456798d2",
                    "height": 7867166,
                    "absolute_slot": 73867237,
                    "timestamp": "2022-10-10 20:25:28",
                    "epoch": 368,
                    "epoch_slot": 254437,
                    "block_producer": "pool103w4na57zyunn2s7r8cgrnqgsn3tskj4w69yx3alx4sezphv53t",
                    "confirmations": 0,
                    "tx_hashes": ["31a84c3c6200bec2498b18c42f882fa690cd0d32a9c84a2019eb5cc42f5971d0", "a90e31b3de59452659617c351e5f746b819cb8b026bf945dd41b4cc199bcc8c9", "dbe30d4f6f42342d7cbfa950bb330cd111bea525b17c2ca115fcec19f1f2e55c", "836c720301e2f463fcaf3ffa746213c03616cc003a1802ddb1243e738140b109", "660c2a41c5a6618db2a514fe0d84bd2570e8f2e0ed8e3ac9b810f5f8230c171d", "357a18477c72d771df77736f83abba44fa826a3e36bc31bb81ca4e2f06475cc6", "a79137811a11914fa6b5543871496e2efbbfd4fb63e3ef808fbbf4247a194f7f", "f89ad271629e827e6722617cce03f7c966f37b9d9143ab356600f31591b902b0", "ae2cc8e8bf536899acb036c7c560c5658abfdffb84a35f18e9a8cf72fb160e35", "b84082a6c4fd731f2a4263d5e99f205798d4356ee5386633ef47727ba2a345fd", "6dc497eb7acf460a491cff25ca22dcdad1fa42bb133262afc20c828d7003b439", "ba7464e4d1c8610e3bef857ddaf7e52083291b5e30750e6bce7ebfeef07f4790", "0344e44a44c644f782088ca7e1143304b20f5cd3e5a4c77748bf4efe5aea06f8", "1eec3e58f7001c5b875456232e79cbfdb64e2be0e2b32c06f5c07cab38069634"],
                    "total_fees": 4195080,
                    "total_ex_units": {
                        "mem": 7191340,
                        "steps": 2947856708i64
                    },
                    "script_invocations": 3,
                    "size": 27430,
                    "previous_block": "3c1778df54aedc9e6226d31cc316a8b5b7912800f909b62f77a46ad61fa10a0c",
                    "total_output_lovelace": "102300616446",
                    "era": "vasil",
                    "protocol_version": [7, 0],
                    "vrf_key": "f913889ad4f696f436dfa19803ebee2219cee59daba879ab9591c75fc8ee33e4",
                    "operational_certificate": {
                        "hot_vkey": "278c4f65bdca2498882a47eb876f023a86bc75b9307bd80ac361587f1c0fc1f1",
                        "sequence_number": 4,
                        "kes_period": 527,
                        "kes_signature": "39a017fbd95a909a3bcca2f368fcc26e7fab64fb09dc48c7d4a99db5bd68b6280a61fe66da3f048b237754d48cfa1c3f954895f4fe41cb71c296997b53dd6501"
                    }
                },
                "last_updated": {
                    "timestamp": "2022-10-10 20:25:28",
                    "block_hash": "bdf4630098ac6923e0ef1ca0b0d6e00dca96790a8c3dd670948fdfe1456798d2",
                    "block_slot": 73867237
                }
            })
        ),
        (status = 400, description = "Malformed query parameters"),
        (status = 404, description = "No results found"),
        (status = 500, description = "Internal server error"),
    )
)]
#[tracing::instrument(name = "BLOCK_INFO", level = "info", skip(config, headers))]
/// Block information
///
/// Returns information about the specified block
pub async fn block_info(
    headers: HeaderMap,
    Path(hash_or_height): Path<String>,
    Extension(config): MapiExtension,
) -> Result<impl IntoResponse, ErrorResponse> {
    let chain = config.chain_info;
    let polyphony = config.polyphony_wrapper;
    let encoder = polyphony.block_by_height_encoder()?;

    // -- parse and try decode user params

    let block_id = utils::decode_block_hash_or_height(&hash_or_height)?;

    // --- start db at most recent timestamp

    let mut txn = polyphony.begin_snapshot_latest().await?;

    // --- fetch cursor for last updated ---

    let (last_updated, tip_height) =
        utils::get_last_updated_with_height(&mut txn, &encoder, &chain).await?;

    // ---

    // resolve block hash to height if required
    let height = match block_id {
        BlockId::Height(n) => n,
        BlockId::Hash(h) => {
            let height_be: [u8; 8] = txn
                .get(encoder.encode_height_by_block_hash_key(&h))
                .await
                .map_err(internal_server_error)?
                .ok_or_else(not_found)?
                .try_into()
                .map_err(|_| internal_server_error_str("malformed block height"))?;

            u64::from_be_bytes(height_be)
        }
    };

    let key = encoder.encode_block_by_height_key(height);

    let value_bytes = txn
        .get(key)
        .await
        .map_err(internal_server_error)?
        .ok_or_else(not_found)?;

    let info = decode_block_by_height_value(&value_bytes);

    let header = MultiEraHeader::decode(Era::Conway as u8, None, &info.header_bytes)
        .or_else(|_| MultiEraHeader::decode(Era::Babbage as u8, None, &info.header_bytes))
        .or_else(|_| MultiEraHeader::decode(Era::Alonzo as u8, None, &info.header_bytes))
        .or_else(|_| MultiEraHeader::decode(Era::Byron as u8, None, &info.header_bytes))
        .or_else(|_| MultiEraHeader::decode(Era::Byron as u8, Some(0), &info.header_bytes))
        .map_err(internal_server_error)?;

    let timestamp = chain.slot_to_utc(header.slot());

    let block_producer = header
        .issuer_vkey()
        .map(Hasher::<224>::hash)
        .map(|vkh| bech32::encode("pool", vkh.to_base32(), Variant::Bech32).unwrap());

    let vrf_key = header.vrf_vkey().map(hex::encode);

    let (era, major, minor) = get_era(&header);

    let (epoch, epoch_slot) = match header {
        MultiEraHeader::Byron(ref h) => (h.consensus_data.0.epoch, h.consensus_data.0.slot),
        MultiEraHeader::EpochBoundary(ref h) => (h.consensus_data.epoch_id, 0),
        _ => chain.genesis().absolute_slot_to_relative(header.slot()),
    };

    let total_ex_units = ExUnits {
        mem: info.mem,
        steps: info.steps,
    };

    let next_block_key = encoder.encode_block_by_height_key(height + 1);

    let next_block_value_bytes = txn
        .get(next_block_key)
        .await
        .map_err(internal_server_error)?;

    let next_block = if let Some(b) = next_block_value_bytes {
        let prev_info = decode_block_by_height_value(&b);

        Some(hex::encode(prev_info.hash))
    } else {
        None
    };

    // -- build response data

    let block_info = BlockInfo {
        hash: hex::encode(header.hash()),
        height,
        absolute_slot: header.slot(),
        timestamp,
        epoch,
        epoch_slot,
        block_producer,
        confirmations: tip_height - height,
        tx_hashes: info.tx_hashes.into_iter().map(hex::encode).collect(),
        total_fees: utils::u64_str_conv(info.fees, &headers),
        total_ex_units,
        script_invocations: info.invocations,
        size: info.size,
        previous_block: header.previous_hash().map(hex::encode),
        next_block,
        total_output_lovelace: info.output.to_string(),
        era,
        protocol_version: (major, minor),
        vrf_key,
        operational_certificate: get_operational_cert(&header),
    };

    // ---

    let out = TimestampedResponse {
        data: block_info,
        last_updated,
    };

    Ok((StatusCode::OK, Json(out)))
}

fn get_operational_cert(hdr: &MultiEraHeader) -> Option<OperationalCert> {
    match hdr {
        MultiEraHeader::EpochBoundary(_) => None,
        MultiEraHeader::ShelleyCompatible(x) => Some(OperationalCert {
            hot_vkey: hex::encode((*x.header_body.operational_cert_hot_vkey).clone()),
            sequence_number: x.header_body.operational_cert_sequence_number,
            kes_period: x.header_body.operational_cert_kes_period,
            kes_signature: hex::encode((*x.header_body.operational_cert_sigma).clone()),
        }),
        MultiEraHeader::BabbageCompatible(x) => Some(OperationalCert {
            hot_vkey: hex::encode(
                (*x.header_body.operational_cert.operational_cert_hot_vkey).clone(),
            ),
            sequence_number: x
                .header_body
                .operational_cert
                .operational_cert_sequence_number,
            kes_period: x.header_body.operational_cert.operational_cert_kes_period,
            kes_signature: hex::encode(
                (*x.header_body.operational_cert.operational_cert_sigma).clone(),
            ),
        }),
        MultiEraHeader::Byron(_) => None,
    }
}

fn get_era(hdr: &MultiEraHeader) -> (LedgerEra, u64, u64) {
    let (major, minor) = match hdr {
        MultiEraHeader::Byron(_) | MultiEraHeader::EpochBoundary(_) => (1, 0),
        MultiEraHeader::ShelleyCompatible(x) => {
            (x.header_body.protocol_major, x.header_body.protocol_minor)
        }
        MultiEraHeader::BabbageCompatible(x) => (
            x.header_body.protocol_version.0,
            x.header_body.protocol_version.1,
        ),
    };

    (LedgerEra::from_major_prot_ver(major), major, minor)
}
