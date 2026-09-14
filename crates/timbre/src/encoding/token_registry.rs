use std::ops::Deref;

use pallas::ledger::primitives::babbage::PolicyId;

use crate::encoding::encode::PREFIX_TOKEN_REGISTRY;

use super::encode::KeyEncoder;

// <DATAPLANE><INSTANCE><TOKEN_REGISTRY><policy><assetname>
pub fn encode_token_registry_by_asset_key(
    encoder: &KeyEncoder,
    policy: &PolicyId,
    name: Vec<u8>,
) -> Vec<u8> {
    let expected_len = 3 + 28 + name.len();
    let mut key = Vec::with_capacity(expected_len);

    key.push(encoder.dataplane_id());
    key.push(encoder.instance_id());
    key.push(PREFIX_TOKEN_REGISTRY);

    key.extend_from_slice(policy.deref());
    key.extend(name);

    assert_eq!(key.len(), expected_len);

    key
}
