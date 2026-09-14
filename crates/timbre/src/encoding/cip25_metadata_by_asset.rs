use crate::encoding::{
    decode::decode_short_bytestring,
    encode::{encode_short_bytestring, KeyEncoder, PREFIX_DATA, REDUCER_CIP25_METADATA_BY_ASSET},
};

#[derive(Hash)]
pub struct Cip25MetadataByAssetKey {
    pub policy: [u8; 28],
    pub name: Vec<u8>,
}

// <DATAPLANE><INSTANCE><DATA><CIP25_METADATA_BY_ASSET_TAG><policy><sbs(assetname)>
pub fn encode_cip25_metadata_by_asset_key(
    encoder: &KeyEncoder,
    policy: &[u8; 28],
    name: Vec<u8>,
) -> Vec<u8> {
    let expected_len = 4 + 28 + (1 + name.len());
    let mut key = Vec::with_capacity(expected_len);

    key.push(encoder.dataplane_id());
    key.push(encoder.instance_id());
    key.push(PREFIX_DATA);

    key.push(REDUCER_CIP25_METADATA_BY_ASSET);

    key.extend_from_slice(policy);
    key.extend(encode_short_bytestring(&name));

    assert_eq!(key.len(), expected_len);

    key
}

pub fn decode_cip25_metadata_by_asset_key(bytes: &[u8]) -> Cip25MetadataByAssetKey {
    assert_eq!(bytes[3], REDUCER_CIP25_METADATA_BY_ASSET);

    let mut c = 4; // cursor

    let policy: [u8; 28] = bytes[c..c + 28].try_into().unwrap();
    c += 28;

    let (name, _name_len) = decode_short_bytestring(&bytes[c..]);
    // c += name_len;

    Cip25MetadataByAssetKey {
        policy,
        name: name.to_vec(),
    }
}
