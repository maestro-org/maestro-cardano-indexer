use std::str::FromStr;

use crate::{
    responses::{
        ErrorResponse, NetworkId, PaymentCredKind, PaymentCredential, Pointer, StakingCredKind,
        StakingCredential,
    },
    utils::bad_request,
};
use axum::{extract::Path, http::StatusCode, response::IntoResponse, Extension, Json};
use pallas::ledger::addresses::{Address, Network, ShelleyDelegationPart, StakeAddress};

use crate::{responses::AddressInfo, MapiExtension};

#[utoipa::path(
    tag = "Addresses",
    get,
    path = "/addresses/{address}/decode",
    params(
        ("address" = String, Path, description = "Address in bech32/hex/base58 format"),
    ),
    responses(
        (
            status = 200,
            description = "Decode an address",
            body = AddressInfo,
            example = json!({
                "bech32": "addr_test1zp40k56s0pq085hyzyeclfh6l987el8xyze5v86h0pr7wzkvenlf6qfxx9s3re5y0pqh2ug89dynd8xsje5835f7sutslvktx9",
                "hex": "106afb53507840f3d2e411338fa6faf94fecfce620b3461f577847e70accccfe9d0126316111e68478417571072b49369cd0966878d13e8717",
                "network": "testnet",
                "payment_cred": {
                    "kind": "script",
                    "bech32": "addr_shared_vkh1dta4x5rcgrea9eq3xw86d7heflk0ee3qkdrp74mcglns58ucedk",
                    "hex": "6afb53507840f3d2e411338fa6faf94fecfce620b3461f577847e70a"
                },
                "staking_cred": {
                    "kind": "key",
                    "bech32": "stake_vkh1enx0a8gpycckzy0xs3uyzat3qu45jd5u6ztxs7x386r3wzy6dys",
                    "reward_address": "stake_test1urxvel5aqynrzcg3u6z8sst4wyrjkjfknngfv6rc6ylgw9cj72l60",
                    "hex": "ccccfe9d0126316111e68478417571072b49369cd0966878d13e8717",
                    "pointer": null
                }
            })
        ),
        (status = 400, description = "Malformed query parameters"),
        (status = 500, description = "Internal server error"),
    )
)]
#[tracing::instrument(name = "DECODE_ADDRESS", level = "info", skip(_extension))]
/// Decode address
///
/// Returns the different information encoded within a Cardano address, including details of the payment and delegation parts of the address
pub async fn decode_address(
    Path(address): Path<String>,
    Extension(_extension): MapiExtension,
) -> Result<impl IntoResponse, ErrorResponse> {
    let address =
        Address::from_str(&address).map_err(|_| bad_request("Could not decode address"))?;

    if matches!(address, Address::Stake(_)) {
        return Err(bad_request(
            "This endpoint does not support reward addresses",
        ));
    }

    let bech32 = address.to_bech32().ok();

    let hex = address.to_hex();

    let network = match address.network() {
        Some(Network::Mainnet) => Some(NetworkId::Mainnet),
        Some(Network::Testnet) => Some(NetworkId::Testnet),
        _ => None,
    };

    let (payment, deleg) = match address {
        Address::Shelley(a) => {
            let p = a.payment();

            let p_kind = if p.is_script() {
                PaymentCredKind::Script
            } else {
                PaymentCredKind::Key
            };

            let payment = PaymentCredential {
                kind: p_kind,
                bech32: p.to_bech32(),
                hex: p.to_hex(),
            };

            let deleg = match a.delegation() {
                p @ ShelleyDelegationPart::Key(_) => {
                    let b32 = p.to_bech32().unwrap();
                    let hex = p.to_hex();
                    let reward: StakeAddress = a.try_into().unwrap();

                    Some(StakingCredential {
                        kind: StakingCredKind::Key,
                        bech32: Some(b32),
                        reward_address: Some(reward.to_bech32().unwrap()),
                        hex: Some(hex),
                        pointer: None,
                    })
                }
                p @ ShelleyDelegationPart::Script(_) => {
                    let b32 = p.to_bech32().unwrap();
                    let hex = p.to_hex();
                    let reward: StakeAddress = a.try_into().unwrap();

                    Some(StakingCredential {
                        kind: StakingCredKind::Script,
                        bech32: Some(b32),
                        reward_address: Some(reward.to_bech32().unwrap()),
                        hex: Some(hex),
                        pointer: None,
                    })
                }
                ShelleyDelegationPart::Pointer(p) => Some(StakingCredential {
                    kind: StakingCredKind::Pointer,
                    bech32: None,
                    hex: None,
                    reward_address: None,
                    pointer: Some(Pointer {
                        slot: p.slot(),
                        tx_index: p.tx_idx(),
                        cert_index: p.cert_idx(),
                    }),
                }),
                ShelleyDelegationPart::Null => None,
            };

            (Some(payment), deleg)
        }
        _ => (None, None),
    };

    let info = AddressInfo {
        bech32,
        hex,
        network,
        payment_cred: payment,
        staking_cred: deleg,
    };

    Ok((StatusCode::OK, Json(info)))
}
