use pallas::{
    codec::utils::Bytes,
    crypto::hash::Hash,
    ledger::traverse::{Era, MultiEraOutput},
};
use tracing::error;

pub fn address_utxo_contains_asset(
    v: Vec<u8>,
    target_policy: &Hash<28>,
    target_name: &Bytes,
) -> bool {
    let utxo = MultiEraOutput::decode(Era::Conway, &v)
        .or_else(|_| MultiEraOutput::decode(Era::Babbage, &v))
        .or_else(|_| MultiEraOutput::decode(Era::Alonzo, &v))
        .or_else(|_| MultiEraOutput::decode(Era::Byron, &v));

    let utxo = match utxo {
        Ok(u) => u,
        Err(e) => {
            error!("error decoding utxo bytes {e:?} [{}]", hex::encode(&v));
            return false;
        }
    };

    for policy in utxo.value().assets() {
        if policy.policy() == target_policy {
            for asset in policy.assets() {
                if asset.name().to_vec() == target_name.to_vec() && asset.output_coin().unwrap() > 0
                {
                    return true;
                }
            }
        }
    }

    false
}
