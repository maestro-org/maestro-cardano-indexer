#![recursion_limit = "256"]
mod common;

use std::collections::HashMap;

use common::*;
use ctor::ctor;
use mapi::{
    responses::*,
    routes::{AdditionalUtxo, EvaluateRequest},
};
use reqwest::StatusCode;
use serde_json::json;
use std::env;
use tracing_test::traced_test;

#[ctor]
fn test_init() {
    // This code will run before each test
    env::set_var("TOKEN_REGISTRY_DATAPLANE_ID", "123");
    env::set_var("TOKEN_REGISTRY_INSTANCE_ID", "1");
    env::set_var("REDIS", "redis://localhost:6379");
}

// tx_count_by_address

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn tx_count_by_address_route_works() {
    // Arrange
    let TestApp { api_address } = common::spawn_app().await;
    let api_route = format!("{api_address}/addresses/{TEST_ADDRESS}/transactions/count",);
    let expected_tx_count = 1;

    // Act
    let address_tx_count: TimestampedResponse<TxCount> = common::get_route(&api_route).await;

    // Assert
    assert_eq!(
        address_tx_count.data.0, expected_tx_count,
        "Expected tx count at address to be {expected_tx_count}"
    )
}

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn tx_count_by_address_no_data() {
    // Arrange
    let TestApp { api_address } = common::spawn_app().await;
    let api_route = format!("{api_address}/addresses/{TEST_NULL_ADDRESS}/transactions/count");

    let expected_tx_count = 0;

    // Act
    let address_tx_count: TimestampedResponse<TxCount> = common::get_route(&api_route).await;

    // Assert
    assert_eq!(
        address_tx_count.data.0, expected_tx_count,
        "Expected tx count at address to be {expected_tx_count}"
    )
}

// txs_by_address

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn txs_by_address_route_works() {
    // Arrange
    let TestApp { api_address } = common::spawn_app().await;
    let api_route = format!("{api_address}/addresses/{TEST_ADDRESS}/transactions",);
    let expected_length = 1;
    let expected_item = AddressTransaction {
        tx_hash: "257916e7ae112cf16f27218e41bfa37c018bab922354201b3b38c9a24c35ab33".into(),
        slot: 55718892,
        input: false,
        output: true,
    };

    // Act
    let txs: PaginatedResponse<AddressTransaction> = common::get_route(&api_route).await;

    // Assert
    assert_eq!(
        txs.data.len(),
        expected_length,
        "Expected length of UTxO refs to be {expected_length}"
    );

    assert_eq!(
        txs.data[0], expected_item,
        "Expected UTxO ref to be {expected_item:?}"
    );
}

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn txs_by_address_pagination() {
    // Arrange
    let TestApp { api_address } = common::spawn_app().await;
    let api_route = format!("{api_address}/addresses/addr1wxn9efv2f6w82hagxqtn62ju4m293tqvw0uhmdl64ch8uwc0h43gt/transactions?count=1&to=73867238",);
    let expected_len = 1;

    // Act
    let txs: PaginatedResponse<AddressTransaction> = common::get_route(&api_route).await;

    // Assert
    assert_eq!(txs.data.len(), expected_len);

    // Arrange
    let api_route = format!(
        "{api_address}/addresses/addr1wxn9efv2f6w82hagxqtn62ju4m293tqvw0uhmdl64ch8uwc0h43gt/transactions?count=1&cursor={}&to=73867238",
        txs.next_cursor.unwrap()
    );
    let expected_len = 1;

    // Act
    let txs: PaginatedResponse<AddressTransaction> = common::get_route(&api_route).await;

    // Assert
    assert_eq!(txs.data.len(), expected_len);
    assert!(txs.next_cursor.is_none())
}

// txs_by_payment_cred

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn txs_by_payment_cred_route_works() {
    // Arrange
    let TestApp { api_address } = common::spawn_app().await;
    let api_route = format!("{api_address}/addresses/cred/{TEST_KEY_PAYMENT_CRED}/transactions",);
    let expected_length = 1;
    let expected_item = PaymentCredentialTransaction {
        tx_hash: "257916e7ae112cf16f27218e41bfa37c018bab922354201b3b38c9a24c35ab33".into(),
        slot: 55718892,
        input: false,
        output: true,
        required_signer: false,
    };

    // Act
    let txs: PaginatedResponse<PaymentCredentialTransaction> = common::get_route(&api_route).await;

    // Assert
    assert_eq!(
        txs.data.len(),
        expected_length,
        "Expected length of UTxO refs to be {expected_length}"
    );

    assert_eq!(
        txs.data[0], expected_item,
        "Expected UTxO ref to be {expected_item:?}"
    );
}

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn txs_by_payment_pagination() {
    // Arrange
    let TestApp { api_address } = common::spawn_app().await;
    let api_route = format!("{api_address}/addresses/cred/addr_shared_vkh15ew2tzjwn364l2pszu7j5h9w63v2crrnl97m074w9elrk2t0m8l/transactions?count=1&to=105028979",);
    let expected_len = 1;

    // Act
    let txs: PaginatedResponse<PaymentCredentialTransaction> = common::get_route(&api_route).await;

    // Assert
    assert_eq!(txs.data.len(), expected_len);

    // Arrange
    let api_route = format!(
        "{api_address}/addresses/cred/addr_shared_vkh15ew2tzjwn364l2pszu7j5h9w63v2crrnl97m074w9elrk2t0m8l/transactions?count=1&cursor={}&to=105028979",
        txs.next_cursor.unwrap()
    );
    let expected_len = 1;

    // Act
    let txs: PaginatedResponse<PaymentCredentialTransaction> = common::get_route(&api_route).await;

    // Assert
    assert_eq!(txs.data.len(), expected_len);
    assert!(txs.next_cursor.is_none())
}

// utxos_by_payment_creds

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn txs_by_payment_creds_route_works() {
    // Arrange
    let TestApp { api_address } = common::spawn_app().await;
    let api_route = format!("{api_address}/addresses/cred/transactions");
    let expected_length = 4;
    let request_body = vec![
        "addr_vkh1wdkle2sprqsuklt34474g6n4ps7k6pv6zwe4644uxmg7xj54y87",
        "addr_vkh1ndctd7pzttqwud2e94p4yj7ennp4r9guzdzvd8da45j3u6gmzpa",
        "addr_shared_vkh1ffv7hkf75573h0mlsg3jc7cpyuq2pn6tk7xc08dtkx3q53tvmac",
        "addr_vkh19mfem468j7hnzq59wh7t0lck650f88faxkp3f3wjeu8ruklqs5n",
    ];

    // Act
    let txs: PaginatedResponse<PaymentCredentialsTransaction> =
        common::post_json_route(&api_route, request_body).await;

    // Assert
    assert_eq!(txs.data.len(), expected_length);
}

// utxo_refs_at_address

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn utxo_refs_at_address_route_works() {
    // Arrange
    let TestApp { api_address } = common::spawn_app().await;
    let api_route = format!("{api_address}/addresses/{TEST_ADDRESS}/utxo_refs",);
    let expected_length = 1;
    let expected_item = UtxoRef {
        tx_hash: "257916e7ae112cf16f27218e41bfa37c018bab922354201b3b38c9a24c35ab33".into(),
        index: 1,
    };

    // Act
    let utxo_refs: PaginatedResponse<UtxoRef> = common::get_route(&api_route).await;

    // Assert
    assert_eq!(
        utxo_refs.data.len(),
        expected_length,
        "Expected length of UTxO refs to be {expected_length}"
    );

    assert_eq!(
        utxo_refs.data[0], expected_item,
        "Expected UTxO ref to be {expected_item:?}"
    );
}

// TODO pagination

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn utxos_refs_at_address_no_data() {
    // Arrange
    let TestApp { api_address } = common::spawn_app().await;
    let api_route = format!("{api_address}/addresses/{TEST_NULL_ADDRESS}/utxo_refs");
    let expected_length = 0;

    // Act
    let utxo_refs: PaginatedResponse<UtxoRef> = common::get_route(&api_route).await;

    // Assert
    assert_eq!(
        utxo_refs.data.len(),
        expected_length,
        "Expected length of UTxO refs to be {expected_length}"
    );
}

// utxos_by_address

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn utxos_by_address_route_works() {
    // Arrange
    let TestApp { api_address } = common::spawn_app().await;
    let api_route = format!("{api_address}/addresses/{TEST_ADDRESS}/utxos");
    let expected_item = UtxoWithSlot {
        tx_hash: "257916e7ae112cf16f27218e41bfa37c018bab922354201b3b38c9a24c35ab33".into(),
        index: 1,
        slot: 55718892,
        assets: vec![
            Asset {
                unit: "lovelace".into(),
                amount: NumOrString::U64(1444443),
            },
            Asset {
                unit: "f0ff48bbb7bbe9d59a40f1ce90e9e9d0ff5002ec48f232b49ca0fb9a6164616d616e74"
                    .into(),
                amount: NumOrString::U64(1),
            },
        ],
        address: TEST_ADDRESS.into(),
        datum: None,
        reference_script: None,
        txout_cbor: None,
    };

    // Act
    let utxo_refs: PaginatedResponse<UtxoWithSlot> = common::get_route(&api_route).await;

    // Assert
    assert_eq!(
        utxo_refs.data,
        vec![expected_item.clone()],
        "Expected UTxO to be {expected_item:?}"
    );
}

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn utxos_by_address_asset_filter() {
    // Arrange
    let TestApp { api_address } = common::spawn_app().await;
    let api_route = format!("{api_address}/addresses/addr1zxj47sy4qxlktqzmkrw8dahe46gtv8seakrshsqz26qnvzypw288a4x0xf8pxgcntelxmyclq83s0ykeehchz2wtspksr3q9nx/utxos?asset=ca5fc915496a771109b98c4a2b76e32c21a8229f3332398cb8babcd75261747344616f3035343633");
    let expected_txhash = "836c720301e2f463fcaf3ffa746213c03616cc003a1802ddb1243e738140b109";

    // Act
    let utxo_refs: PaginatedResponse<UtxoWithSlot> = common::get_route(&api_route).await;

    // Assert
    assert_eq!(utxo_refs.data.len(), 1);
    assert_eq!(
        utxo_refs.data[0].tx_hash, expected_txhash,
        "Expected UTxO to be {expected_txhash}"
    );
}

// TODO pagination

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn utxos_by_address_no_data() {
    // Arrange
    let TestApp { api_address } = common::spawn_app().await;
    let api_route = format!("{api_address}/addresses/{TEST_NULL_ADDRESS}/utxos");
    let expected_length = 0;

    // Act
    let utxos: PaginatedResponse<Utxo> = common::get_route(&api_route).await;

    // Assert
    assert_eq!(utxos.data.len(), expected_length);
}

// utxos_by_addresses

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn utxos_by_addresses_route_works() {
    // Arrange
    let TestApp { api_address } = common::spawn_app().await;
    let api_route = format!("{api_address}/addresses/utxos");
    let expected_length = 2;
    let request_body = vec![TEST_ADDRESS, TEST_ADDRESS_2];

    // Act
    let utxos: PaginatedResponse<UtxoWithSlot> =
        common::post_json_route(&api_route, request_body).await;

    // Assert
    assert_eq!(utxos.data.len(), expected_length);
}

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn utxos_by_addresses_pagination_works() {
    // Arrange
    let TestApp { api_address } = common::spawn_app().await;

    let request_body = vec![TEST_ADDRESS_2, TEST_ADDRESS];
    let expected_length = 1;

    // Arrange
    let api_route = format!("{api_address}/addresses/utxos?count=1");

    // Act
    let utxos: PaginatedResponse<Utxo> =
        common::post_json_route(&api_route, request_body.clone()).await;

    // Assert
    // TEST_ADDRESS < TEST_ADDRESS_2, so its UTxO (X#1) is returned first
    assert_eq!(utxos.data.len(), expected_length);
    assert_eq!(
        (utxos.data[0].clone().tx_hash, utxos.data[0].index),
        (
            "257916e7ae112cf16f27218e41bfa37c018bab922354201b3b38c9a24c35ab33".into(),
            1
        )
    );

    // Arrange
    let api_route = format!(
        "{api_address}/addresses/utxos?count=1&cursor={}",
        utxos.next_cursor.unwrap()
    );
    let expected_length = 1;

    // Act
    let utxos: PaginatedResponse<Utxo> =
        common::post_json_route(&api_route, request_body.clone()).await;

    // Assert
    assert_eq!(utxos.data.len(), expected_length);
    assert_eq!(
        (utxos.data[0].clone().tx_hash, utxos.data[0].index),
        (
            "257916e7ae112cf16f27218e41bfa37c018bab922354201b3b38c9a24c35ab33".into(),
            0
        )
    );
    assert_eq!(utxos.next_cursor, None);
}

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn utxos_by_addresses_route_no_data() {
    // Arrange
    let TestApp { api_address } = common::spawn_app().await;
    let api_route = format!("{api_address}/addresses/utxos");
    let expected_length = 0;
    let request_body = vec![TEST_NULL_ADDRESS];

    // Act
    let utxos: PaginatedResponse<Utxo> = common::post_json_route(&api_route, request_body).await;

    // Assert
    assert_eq!(utxos.data.len(), expected_length);
}

// utxos_by_payment_cred

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn utxos_by_key_payment_cred_route_works() {
    // Arrange
    let TestApp { api_address } = common::spawn_app().await;
    let api_route = format!("{api_address}/addresses/cred/{TEST_KEY_PAYMENT_CRED}/utxos",);
    let expected_item = UtxoWithSlot {
        tx_hash: "257916e7ae112cf16f27218e41bfa37c018bab922354201b3b38c9a24c35ab33".into(),
        index: 1,
        slot: 55718892,
        assets: vec![
            Asset {
                unit: "lovelace".into(),
                amount: NumOrString::U64(1444443),
            },
            Asset {
                unit: "f0ff48bbb7bbe9d59a40f1ce90e9e9d0ff5002ec48f232b49ca0fb9a6164616d616e74"
                    .into(),
                amount: NumOrString::U64(1),
            },
        ],
        address: TEST_ADDRESS.into(),
        datum: None,
        reference_script: None,
        txout_cbor: None,
    };

    // Act
    let utxos: PaginatedResponse<UtxoWithSlot> = common::get_route(&api_route).await;

    // Assert
    assert_eq!(utxos.data, vec![expected_item]);
}

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn utxos_by_script_payment_cred_route_works() {
    // Arrange
    let TestApp { api_address } = common::spawn_app().await;
    let api_route = format!("{api_address}/addresses/cred/{TEST_SCRIPT_PAYMENT_CRED}/utxos",);
    let expected_length = 1;
    let expected_item = UtxoWithSlot {
        tx_hash: "a7e1f137a1d4befa15128286fd08dad67aebe2c11995aa2c04cc48159ccef082".into(),
        index: 0,
        slot: 55718892,
        assets: vec![
            Asset {
                unit: "lovelace".into(),
                amount: NumOrString::U64(1724100),
            },
            Asset {
                unit: "29760f3fb40b3670144594df635e5bb5d144a1173bff0d96fcad155c43727970746f426173686f2330303037"
                    .into(),
                amount: NumOrString::U64(1),
            },
        ],
        address: "addr1w896t6qnpsjs32xhw8jl3kw34pqz69kgd72l8hqw83w0k3qahx2sv".into(),
        datum: Some(DatumOption { datum_type: DatumOptionType::Hash, hash: "950c3e20ab32ba38c75cac5b1a7451800e514326a953c64e2dced7b4772ffcbe".into(), bytes: None, json: None }),
        reference_script: None,
        txout_cbor: None,
    };

    // Act
    let utxos: PaginatedResponse<UtxoWithSlot> = common::get_route(&api_route).await;

    // Assert
    assert_eq!(
        utxos.data.len(),
        expected_length,
        "Expected length of UTxOs to be {expected_length}"
    );

    assert_eq!(
        utxos.data[0], expected_item,
        "Expected UTxO to be {expected_item:?}"
    );
}

// TODO pagination

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn utxos_by_payment_cred_route_no_data() {
    // Arrange
    let TestApp { api_address } = common::spawn_app().await;
    let api_route = format!("{api_address}/addresses/cred/{TEST_NULL_KEY_PAYMENT_CRED}/utxos",);
    let expected_length = 0;

    // Act
    let utxos: PaginatedResponse<UtxoWithSlot> = common::get_route(&api_route).await;

    // Assert
    assert_eq!(utxos.data.len(), expected_length);
}

// utxos_by_payment_creds

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn utxos_by_payment_creds_route_works() {
    // Arrange
    let TestApp { api_address } = common::spawn_app().await;
    let api_route = format!("{api_address}/addresses/cred/utxos");
    let expected_length = 2;
    let request_body = vec![TEST_KEY_PAYMENT_CRED, TEST_SCRIPT_PAYMENT_CRED];

    // Act
    let utxos: PaginatedResponse<UtxoWithSlot> =
        common::post_json_route(&api_route, request_body).await;

    // Assert
    assert_eq!(utxos.data.len(), expected_length);
}

// asset_accounts

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn asset_accounts_route_works() {
    // Arrange
    let TestApp { api_address } = common::spawn_app().await;
    let api_route = format!(
        "{api_address}/assets/{TEST_POLICY_HASH}{}/accounts",
        "7573656d65"
    );
    let expected_item = AssetHolderAccount {
        account: "stake1uylx7yct3smspkes9fzk3f8y8mmu90htxe75g923576d2zgpluxt7".into(),
        amount: NumOrString::U64(1),
    };

    // Act
    let holders: PaginatedResponse<AssetHolderAccount> = common::get_route(&api_route).await;

    // Assert
    assert_eq!(holders.data, vec![expected_item]);
}

// asset_addresses

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn asset_addresses_route_works() {
    // Arrange
    let TestApp { api_address } = common::spawn_app().await;
    let api_route = format!("{api_address}/assets/{TEST_POLICY_HASH}{TEST_ASSET_NAME}/addresses");
    let expected_item = AssetHolder {
        address: TEST_ADDRESS.into(),
        amount: NumOrString::U64(1),
    };

    // Act
    let holders: PaginatedResponse<AssetHolder> = common::get_route(&api_route).await;

    // Assert
    assert_eq!(holders.data, vec![expected_item]);
}

// TODO pagination

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn asset_addresses_no_data() {
    // Arrange
    let TestApp { api_address } = common::spawn_app().await;
    let api_route = format!(
        "{api_address}/assets/{TEST_POLICY_HASH}{}/addresses",
        "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
    );
    let expected_len = 0;

    // Act
    let holders: PaginatedResponse<AssetHolder> = common::get_route(&api_route).await;

    // Assert
    assert_eq!(holders.data.len(), expected_len)
}

// asset_info

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn asset_info_route_works() {
    // Arrange
    let TestApp { api_address } = spawn_app().await;
    let api_route =
        format!("{api_address}/assets/f0ff48bbb7bbe9d59a40f1ce90e9e9d0ff5002ec48f232b49ca0fb9a6164616d616e74",);
    let expected: TimestampedResponse<AssetInfo> = serde_json::from_value(json!({"data":{"asset_name":"6164616d616e74","asset_name_ascii":"adamant","fingerprint":"asset105lxc60yjpqygjsnn29e5hjyafnfw75pqwwza2","total_supply":"1","unique_holders":{"by_address":1,"by_account":0},"first_mint_tx":{"tx_hash":"257916e7ae112cf16f27218e41bfa37c018bab922354201b3b38c9a24c35ab33","slot":55718892,"timestamp":"2022-03-14 19:13:03","amount":"1"},"latest_mint_tx":{"tx_hash":"257916e7ae112cf16f27218e41bfa37c018bab922354201b3b38c9a24c35ab33","slot":55718892,"timestamp":"2022-03-14 19:13:03","amount":"1"},"mint_tx_count":1,"burn_tx_count":0,"asset_standards":{"cip25_metadata":{"name":"$adamant","description":"The Handle Standard","website":"https://adahandle.com","image":"ipfs://Qmai8iwE1Diw5YZVYSpSPAbXM5AVprheve8AAXJzwXoftf","core":{"og":0,"termsofuse":"https://adahandle.com/tou","handleEncoding":"utf-8","prefix":"$","version":0},"augmentations":[]},"cip68_metadata":null},"latest_mint_tx_metadata":{"721":{"f0ff48bbb7bbe9d59a40f1ce90e9e9d0ff5002ec48f232b49ca0fb9a":{"cardanosweets":{"name":"$cardanosweets","description":"The Handle Standard","website":"https://adahandle.com","image":"ipfs://QmaNoVpks3gAR4oayMkx9uHU2baDMGQrBAzpM1PZ3ebJBu","core":{"og":0,"termsofuse":"https://adahandle.com/tou","handleEncoding":"utf-8","prefix":"$","version":0},"augmentations":[]},"adamant":{"name":"$adamant","description":"The Handle Standard","website":"https://adahandle.com","image":"ipfs://Qmai8iwE1Diw5YZVYSpSPAbXM5AVprheve8AAXJzwXoftf","core":{"og":0,"termsofuse":"https://adahandle.com/tou","handleEncoding":"utf-8","prefix":"$","version":0},"augmentations":[]},"pancake_swap":{"name":"$pancake_swap","description":"The Handle Standard","website":"https://adahandle.com","image":"ipfs://QmbFeBtq8cvMqiN8ns8v2ajwZjWe1tbkhADdYusCHEzkjE","core":{"og":0,"termsofuse":"https://adahandle.com/tou","handleEncoding":"utf-8","prefix":"$","version":0},"augmentations":[]},"dragonm":{"name":"$dragonm","description":"The Handle Standard","website":"https://adahandle.com","image":"ipfs://QmTTkmXrAXaGh5hcfZ2y3jWT7krrcxjkGeXBWu1ikh7djZ","core":{"og":0,"termsofuse":"https://adahandle.com/tou","handleEncoding":"utf-8","prefix":"$","version":0},"augmentations":[]},"pay.dylan":{"name":"$pay.dylan","description":"The Handle Standard","website":"https://adahandle.com","image":"ipfs://QmQ645UxEQfx82ZU3sm6M5sM62gfXSGcbpqDtVZnjasyyo","core":{"og":0,"termsofuse":"https://adahandle.com/tou","handleEncoding":"utf-8","prefix":"$","version":0},"augmentations":[]},"bdsm":{"name":"$bdsm","description":"The Handle Standard","website":"https://adahandle.com","image":"ipfs://QmfTpy3ybWL1teCMVAieh7atHtYgpcCZ5Ew58eSUxHgZFb","core":{"og":0,"termsofuse":"https://adahandle.com/tou","handleEncoding":"utf-8","prefix":"$","version":0},"augmentations":[]},"yoroi.ada":{"name":"$yoroi.ada","description":"The Handle Standard","website":"https://adahandle.com","image":"ipfs://QmfSMNMhVuDdT27iY15qUSP2zFzXA6cAsvzNYcKWheX6Dr","core":{"og":0,"termsofuse":"https://adahandle.com/tou","handleEncoding":"utf-8","prefix":"$","version":0},"augmentations":[]},"guyguy":{"name":"$guyguy","description":"The Handle Standard","website":"https://adahandle.com","image":"ipfs://QmRq2QQbdnUhsdU6S5HyCk7Ktx9uW5gAStu1szHzUk7voc","core":{"og":0,"termsofuse":"https://adahandle.com/tou","handleEncoding":"utf-8","prefix":"$","version":0},"augmentations":[]},"texans":{"name":"$texans","description":"The Handle Standard","website":"https://adahandle.com","image":"ipfs://QmU5uGHqc7sLPiaccAnyK2grGjcDM8AWXBtH6zRV9Dme83","core":{"og":0,"termsofuse":"https://adahandle.com/tou","handleEncoding":"utf-8","prefix":"$","version":0},"augmentations":[]},"monsanto":{"name":"$monsanto","description":"The Handle Standard","website":"https://adahandle.com","image":"ipfs://QmUFspnZQMWQyZ9ny6NBkn6ymRvo96oi9V7egXRrKGQGL2","core":{"og":0,"termsofuse":"https://adahandle.com/tou","handleEncoding":"utf-8","prefix":"$","version":0},"augmentations":[]},"dionaeatrap":{"name":"$dionaeatrap","description":"The Handle Standard","website":"https://adahandle.com","image":"ipfs://QmPhZzDhiC5b6DCEFXHHv7qnyvZYy94C4svVHi417CY8bX","core":{"og":0,"termsofuse":"https://adahandle.com/tou","handleEncoding":"utf-8","prefix":"$","version":0},"augmentations":[]}}}},"token_registry_metadata":null},"last_updated":{"timestamp":"2023-10-18 07:30:52","block_hash":"0e1e924710135acfe200ab13d290bd282a67584fd54456f0dcac0aeaa38bb2c2","block_slot":106047961}})).unwrap();

    // Act
    let info: TimestampedResponse<AssetInfo> = get_route(&api_route).await;

    // Assert
    assert_eq!(info.data, expected.data)
}

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn asset_info_no_data() {
    // Arrange
    let TestApp { api_address } = spawn_app().await;
    let api_route =
        format!("{api_address}/assets/{TEST_DUMMY_HEX_28_BYTES}{TEST_DUMMY_HEX_32_BYTES}",);

    // Act
    let response = get_route_any_status(&api_route).await;

    // Assert
    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "Expected 404 for invalid request, got: {} {:?}",
        response.status(),
        response.text().await
    )
}

// asset_mints

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn asset_mints_route_works() {
    // Arrange
    let TestApp { api_address } = spawn_app().await;
    let api_route =
        format!("{api_address}/assets/f0ff48bbb7bbe9d59a40f1ce90e9e9d0ff5002ec48f232b49ca0fb9a6164616d616e74/mints",);
    let expected: PaginatedResponse<MintTransaction> = serde_json::from_value(json!({"data":[{"tx_hash":"257916e7ae112cf16f27218e41bfa37c018bab922354201b3b38c9a24c35ab33","slot":55718892,"timestamp":"2022-03-14 19:13:03","amount":"1"}],"last_updated":{"timestamp":"2023-10-18 07:30:52","block_hash":"0e1e924710135acfe200ab13d290bd282a67584fd54456f0dcac0aeaa38bb2c2","block_slot":106047961},"next_cursor":null})).unwrap();

    // Act
    let txs: PaginatedResponse<MintTransaction> = get_route(&api_route).await;

    // Assert
    assert_eq!(txs.data, expected.data)
}

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn asset_mints_no_data() {
    // Arrange
    let TestApp { api_address } = common::spawn_app().await;
    let api_route = format!(
        "{api_address}/assets/{TEST_POLICY_HASH}{}/utxos",
        "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
    );

    // Act
    let txs: PaginatedResponse<MintTransaction> = common::get_route(&api_route).await;

    // Assert
    assert_eq!(txs.data.len(), 0)
}

// asset txs

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn asset_txs_route_works() {
    // Arrange
    let TestApp { api_address } = spawn_app().await;
    let api_route =
        format!("{api_address}/assets/f0ff48bbb7bbe9d59a40f1ce90e9e9d0ff5002ec48f232b49ca0fb9a6164616d616e74/transactions",);
    let expected: PaginatedResponse<TimestampedTransaction> = serde_json::from_value(json!({"data":[{"tx_hash":"257916e7ae112cf16f27218e41bfa37c018bab922354201b3b38c9a24c35ab33","slot":55718892,"timestamp":"2022-03-14 19:13:03"}],"last_updated":{"timestamp":"2023-10-18 07:30:52","block_hash":"0e1e924710135acfe200ab13d290bd282a67584fd54456f0dcac0aeaa38bb2c2","block_slot":106047961},"next_cursor":null})).unwrap();

    // Act
    let txs: PaginatedResponse<TimestampedTransaction> = get_route(&api_route).await;

    // Assert
    assert_eq!(txs.data, expected.data)
}

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn asset_txs_no_data() {
    // Arrange
    let TestApp { api_address } = common::spawn_app().await;
    let api_route = format!(
        "{api_address}/assets/{TEST_POLICY_HASH}{}/transactions",
        "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
    );

    // Act
    let txs: PaginatedResponse<TimestampedTransaction> = common::get_route(&api_route).await;

    // Assert
    assert_eq!(txs.data.len(), 0)
}

// asset_utxos

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn asset_utxos_route_works() {
    // Arrange
    let TestApp { api_address } = common::spawn_app().await;
    let api_route = format!("{api_address}/assets/{TEST_POLICY_HASH}{TEST_ASSET_NAME}/utxos",);
    let expected_item = AssetUtxo {
        tx_hash: "257916e7ae112cf16f27218e41bfa37c018bab922354201b3b38c9a24c35ab33".into(),
        index: 1,
        slot: 55718892,
        address: TEST_ADDRESS.into(),
        amount: NumOrString::U64(1),
    };

    // Act
    let utxos: PaginatedResponse<AssetUtxo> = common::get_route(&api_route).await;

    // Assert
    assert_eq!(utxos.data, vec![expected_item])
}

// TODO pagination

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn asset_utxos_no_data() {
    // Arrange
    let TestApp { api_address } = common::spawn_app().await;
    let api_route = format!(
        "{api_address}/assets/{TEST_POLICY_HASH}{}/utxos",
        "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
    );

    // Act
    let holders: PaginatedResponse<AssetUtxo> = common::get_route(&api_route).await;

    // Assert
    assert_eq!(holders.data.len(), 0)
}

// policy_accounts

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn policy_accounts_route_works() {
    // Arrange
    let TestApp { api_address } = common::spawn_app().await;
    let api_route = format!("{api_address}/policy/{TEST_POLICY_HASH}/accounts",);
    let expected_len = 32;

    // Act
    let holders: PaginatedResponse<PolicyHolderAccount> = common::get_route(&api_route).await;

    // Assert
    assert_eq!(holders.data.len(), expected_len)
}

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn policy_accounts_pagination() {
    // Arrange
    let TestApp { api_address } = common::spawn_app().await;
    let api_route = format!("{api_address}/policy/{TEST_POLICY_HASH}/accounts?count=25",);
    let expected_len = 25;

    // Act
    let holders: PaginatedResponse<PolicyHolderAccount> = common::get_route(&api_route).await;

    // Assert
    assert_eq!(holders.data.len(), expected_len);

    // Arrange
    let api_route = format!(
        "{api_address}/policy/{TEST_POLICY_HASH}/accounts?count=25&cursor={}",
        holders.next_cursor.unwrap()
    );
    let expected_len = 7;

    // Act
    let holders: PaginatedResponse<PolicyHolderAccount> = common::get_route(&api_route).await;

    // Assert
    assert_eq!(holders.data.len(), expected_len);
    assert!(holders.next_cursor.is_none())
}

// policy_addresses

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn policy_addresses_route_works() {
    // Arrange
    let TestApp { api_address } = common::spawn_app().await;
    let api_route = format!("{api_address}/policy/{TEST_POLICY_HASH}/addresses",);
    let expected_item = PolicyHolder {
        address: TEST_ADDRESS.into(),
        assets: vec![AssetInPolicy {
            name: "6164616d616e74".into(),
            amount: NumOrString::U64(1),
        }],
    };

    // Act
    let holders: PaginatedResponse<PolicyHolder> = common::get_route(&api_route).await;

    // Assert
    assert!(holders.data.contains(&expected_item))
}

// TODO pagination

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn policy_addresses_no_data() {
    // Arrange
    let TestApp { api_address } = common::spawn_app().await;
    let api_route = format!(
        "{api_address}/policy/{}/addresses",
        "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
    );
    let expected_len = 0;

    // Act
    let holders: PaginatedResponse<PolicyHolder> = common::get_route(&api_route).await;

    // Assert
    assert_eq!(holders.data.len(), expected_len)
}

// policy_assets

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn policy_assets_route_works_cip25() {
    // Arrange
    let TestApp { api_address } = common::spawn_app().await;
    let api_route = format!("{api_address}/policy/{TEST_POLICY_HASH}/assets");

    let cip_25 = json!({
        "name": "$adamant",
        "description": "The Handle Standard",
        "website": "https://adahandle.com",
        "image": "ipfs://Qmai8iwE1Diw5YZVYSpSPAbXM5AVprheve8AAXJzwXoftf",
        "core": {
           "og": 0,
           "termsofuse": "https://adahandle.com/tou",
           "handleEncoding": "utf-8",
           "prefix": "$",
           "version": 0
        },
        "augmentations": []
    });

    let expected_item = AssetInfoConcise {
        asset_name: hex::encode("adamant"),
        asset_name_ascii: "adamant".into(),
        fingerprint: "asset105lxc60yjpqygjsnn29e5hjyafnfw75pqwwza2".into(),
        total_supply: "1".into(),
        asset_standards: AssetStandards {
            cip25_metadata: Some(cip_25),
            cip68_metadata: None,
        },
    };

    // Act
    let assets: PaginatedResponse<AssetInfoConcise> = common::get_route(&api_route).await;

    // Assert
    assert!(assets.data.contains(&expected_item))
}

// policy_info

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn policy_info_route_works() {
    // Arrange
    let TestApp { api_address } = common::spawn_app().await;
    let api_route = format!("{api_address}/policy/{TEST_POLICY_HASH}");
    let expected: TimestampedResponse<PolicyInfo> = serde_json::from_value(json!({"data":{"policy_id":"f0ff48bbb7bbe9d59a40f1ce90e9e9d0ff5002ec48f232b49ca0fb9a","script":{"hash":"f0ff48bbb7bbe9d59a40f1ce90e9e9d0ff5002ec48f232b49ca0fb9a","type":"native","bytes":"8200581c4da965a049dfd15ed1ee19fba6e2974a0b79fc416dd1796a1f97f5e1","json":{"keyHash":"4da965a049dfd15ed1ee19fba6e2974a0b79fc416dd1796a1f97f5e1","type":"sig"}},"assets_of_policy":11,"total_supply":"11","unique_holders":{"by_address":34,"by_account":32},"first_mint_tx":{"tx_hash":"257916e7ae112cf16f27218e41bfa37c018bab922354201b3b38c9a24c35ab33","slot":55718892,"timestamp":"2022-03-14 19:13:03"},"latest_mint_tx":{"tx_hash":"257916e7ae112cf16f27218e41bfa37c018bab922354201b3b38c9a24c35ab33","slot":55718892,"timestamp":"2022-03-14 19:13:03"}},"last_updated":{"timestamp":"2023-10-18 07:30:52","block_hash":"0e1e924710135acfe200ab13d290bd282a67584fd54456f0dcac0aeaa38bb2c2","block_slot":106047961}})).unwrap();

    // Act
    let info: TimestampedResponse<PolicyInfo> = common::get_route(&api_route).await;

    // Assert
    assert_eq!(info.data, expected.data)
}

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn policy_info_no_data() {
    // Arrange
    let TestApp { api_address } = common::spawn_app().await;
    let api_route = format!("{api_address}/policy/{TEST_DUMMY_HEX_28_BYTES}");

    // Act
    let response = common::get_route_any_status(&api_route).await;

    // Asset
    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "Expected 404 for invalid request, got: {} {:?}",
        response.status(),
        response.text().await
    )
}

// policy mints

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn policy_mints_route_works() {
    // Arrange
    let TestApp { api_address } = common::spawn_app().await;
    let api_route = format!(
        "{api_address}/policy/f0ff48bbb7bbe9d59a40f1ce90e9e9d0ff5002ec48f232b49ca0fb9a/mints"
    );
    let expected: PaginatedResponse<PolicyMintTransaction> = serde_json::from_value(json!({"data":[{"tx_hash":"257916e7ae112cf16f27218e41bfa37c018bab922354201b3b38c9a24c35ab33","slot":55718892,"timestamp":"2022-03-14 19:13:03","assets":[{"name":"6164616d616e74","amount":"1"},{"name":"6264736d","amount":"1"},{"name":"63617264616e6f737765657473","amount":"1"},{"name":"64696f6e61656174726170","amount":"1"},{"name":"647261676f6e6d","amount":"1"},{"name":"677579677579","amount":"1"},{"name":"6d6f6e73616e746f","amount":"1"},{"name":"70616e63616b655f73776170","amount":"1"},{"name":"7061792e64796c616e","amount":"1"},{"name":"746578616e73","amount":"1"},{"name":"796f726f692e616461","amount":"1"}]}],"last_updated":{"timestamp":"2023-10-18 07:30:52","block_hash":"0e1e924710135acfe200ab13d290bd282a67584fd54456f0dcac0aeaa38bb2c2","block_slot":106047961},"next_cursor":null})).unwrap();

    // Act
    let mints: PaginatedResponse<PolicyMintTransaction> = common::get_route(&api_route).await;

    // Assert
    assert_eq!(mints.data, expected.data)
}

// policy txs

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn policy_txs_route_works() {
    // Arrange
    let TestApp { api_address } = common::spawn_app().await;
    let api_route = format!("{api_address}/policy/f0ff48bbb7bbe9d59a40f1ce90e9e9d0ff5002ec48f232b49ca0fb9a/transactions?count=3");
    let expected: PaginatedResponse<PolicyTransaction> = serde_json::from_value(json!({"data":[{"tx_hash":"09b2c948cf12de9eba590eb6af9ddc1e98b9342a79568b63fea06541cb3fecc1","slot":49503576,"assets":["61736d72","647572616e676f","67657268617274","67756e6e657273"]},{"tx_hash":"1c0375e0aa718c9a203d6fc35522f0de953fcccfe7a74b544a5a246f4d5103d4","slot":49503576,"assets":["6368696c6c696e67","6d6f6e6579706f6f6c"]},{"tx_hash":"5d588bb46091b249f0f6874e97e3738d16e4f20f250242d6e08a93ccbf0d0e30","slot":49503576,"assets":["63617264616e6f2e616461"]}],"last_updated":{"timestamp":"2023-10-18 07:30:52","block_hash":"0e1e924710135acfe200ab13d290bd282a67584fd54456f0dcac0aeaa38bb2c2","block_slot":106047961},"next_cursor":"AAAAAALzXVgAQQ"})).unwrap();

    // Act
    let txs: PaginatedResponse<PolicyTransaction> = common::get_route(&api_route).await;

    // Assert
    assert_eq!(txs.data, expected.data)
}

// policy_utxos

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn policy_utxos_route_works() {
    // Arrange
    let TestApp { api_address } = common::spawn_app().await;
    let api_route = format!("{api_address}/policy/{TEST_POLICY_HASH}/utxos");
    let expected_item = PolicyUtxo {
        tx_hash: "257916e7ae112cf16f27218e41bfa37c018bab922354201b3b38c9a24c35ab33".into(),
        index: 1,
        slot: 55718892,
        address: TEST_ADDRESS.into(),
        assets: vec![AssetInPolicy {
            name: "6164616d616e74".into(),
            amount: NumOrString::U64(1),
        }],
    };

    // Act
    let utxos: PaginatedResponse<PolicyUtxo> = common::get_route(&api_route).await;

    // Assert
    assert!(utxos.data.contains(&expected_item))
}

// TODO pagination

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn policy_utxos_no_data() {
    // Arrange
    let TestApp { api_address } = common::spawn_app().await;
    let api_route = format!(
        "{api_address}/policy/{}/utxos",
        "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
    );
    let expected_len = 0;

    // Act
    let utxos: PaginatedResponse<PolicyUtxo> = common::get_route(&api_route).await;

    // Assert
    assert_eq!(utxos.data.len(), expected_len)
}

// TODO CIP68 resolving: asset info requires dbsync info for asset

// block_info

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn block_info_route_works() {
    // Arrange
    let TestApp { api_address } = common::spawn_app().await;
    let api_route = format!("{api_address}/blocks/{TEST_BLOCK_HASH}");
    // cardanoscan
    let expected_item = BlockInfo {
        hash: TEST_BLOCK_HASH.into(),
        height: 7867166,
        absolute_slot: 73867237,
        timestamp: "2022-10-10 20:25:28".into(),
        epoch: 368,
        epoch_slot: 254437,
        block_producer: Some("pool103w4na57zyunn2s7r8cgrnqgsn3tskj4w69yx3alx4sezphv53t".into()),
        confirmations: 1794142,
        tx_hashes: vec![
            "31a84c3c6200bec2498b18c42f882fa690cd0d32a9c84a2019eb5cc42f5971d0",
            "a90e31b3de59452659617c351e5f746b819cb8b026bf945dd41b4cc199bcc8c9",
            "dbe30d4f6f42342d7cbfa950bb330cd111bea525b17c2ca115fcec19f1f2e55c",
            "836c720301e2f463fcaf3ffa746213c03616cc003a1802ddb1243e738140b109",
            "660c2a41c5a6618db2a514fe0d84bd2570e8f2e0ed8e3ac9b810f5f8230c171d",
            "357a18477c72d771df77736f83abba44fa826a3e36bc31bb81ca4e2f06475cc6",
            "a79137811a11914fa6b5543871496e2efbbfd4fb63e3ef808fbbf4247a194f7f",
            "f89ad271629e827e6722617cce03f7c966f37b9d9143ab356600f31591b902b0",
            "ae2cc8e8bf536899acb036c7c560c5658abfdffb84a35f18e9a8cf72fb160e35",
            "b84082a6c4fd731f2a4263d5e99f205798d4356ee5386633ef47727ba2a345fd",
            "6dc497eb7acf460a491cff25ca22dcdad1fa42bb133262afc20c828d7003b439",
            "ba7464e4d1c8610e3bef857ddaf7e52083291b5e30750e6bce7ebfeef07f4790",
            "0344e44a44c644f782088ca7e1143304b20f5cd3e5a4c77748bf4efe5aea06f8",
            "1eec3e58f7001c5b875456232e79cbfdb64e2be0e2b32c06f5c07cab38069634",
        ]
        .into_iter()
        .map(|s| s.into())
        .collect(),
        total_fees: NumOrString::U64(4195080),
        total_ex_units: ExUnits {
            mem: 1460018 + 4000000 + 1731322,
            steps: 431855142 + 2000000000 + 516001566,
        },
        script_invocations: 3,
        size: 26570,
        previous_block: Some(
            "3c1778df54aedc9e6226d31cc316a8b5b7912800f909b62f77a46ad61fa10a0c".into(),
        ),
        next_block: Some("9583602d6bafe9c0a782e5e40315581057c8527f1da38bf68a792161fc9fac02".into()),
        total_output_lovelace: "102300616446".into(),
        era: LedgerEra::Vasil,
        protocol_version: (7, 0),
        vrf_key: Some("f913889ad4f696f436dfa19803ebee2219cee59daba879ab9591c75fc8ee33e4".into()),
        operational_certificate: Some(OperationalCert { hot_vkey: "278c4f65bdca2498882a47eb876f023a86bc75b9307bd80ac361587f1c0fc1f1".into(), sequence_number: 4, kes_period: 527, kes_signature: "39a017fbd95a909a3bcca2f368fcc26e7fab64fb09dc48c7d4a99db5bd68b6280a61fe66da3f048b237754d48cfa1c3f954895f4fe41cb71c296997b53dd6501".into() })
    };

    // Act
    let resolved: TimestampedResponse<BlockInfo> = common::get_route(&api_route).await;

    // Assert
    assert_eq!(resolved.data, expected_item)
}

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn block_info_no_data() {
    // Arrange
    let TestApp { api_address } = common::spawn_app().await;
    let api_route = format!("{api_address}/blocks/{TEST_DUMMY_HEX_32_BYTES}");

    // Act
    let response = common::get_route_any_status(&api_route).await;

    // Assert
    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "Expected 404 for invalid request, got: {} {:?}",
        response.status(),
        response.text().await
    )
}

// adahandle_resolve

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn adahandle_resolve_route_works() {
    // Arrange
    let TestApp { api_address } = common::spawn_app().await;
    let api_route = format!("{api_address}/ecosystem/adahandle/{}", "adamant");
    let expected_item = TEST_ADDRESS;

    // Act
    let resolved: TimestampedResponse<Address> = common::get_route(&api_route).await;

    // Assert
    assert_eq!(resolved.data.0, expected_item)
}

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn adahandle_resolve_no_data() {
    // Arrange
    let TestApp { api_address } = common::spawn_app().await;
    let api_route = format!("{api_address}/ecosystem/adahandle/{}", "x9k2m4p7q1w8e5r"); // handle that does not exist

    // Act
    let response = common::get_route_any_status(&api_route).await;

    // Asset
    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "Expected 404 for invalid request, got: {} {:?}",
        response.status(),
        response.text().await
    )
}

// address_by_txo

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn address_by_txo_route_works() {
    // Arrange
    let TestApp { api_address } = common::spawn_app().await;
    let api_route = format!(
        "{api_address}/transactions/{TEST_TX_HASH}/outputs/{}/address",
        1
    );
    let expected_item = TEST_ADDRESS;

    // Act
    let address: TimestampedResponse<Address> = common::get_route(&api_route).await;

    // Assert
    assert_eq!(address.data.0, expected_item)
}

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn address_by_txo_no_data() {
    // Arrange
    let TestApp { api_address } = common::spawn_app().await;
    let api_route = format!(
        "{api_address}/transactions/{}/outputs/{}/address",
        "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef", 0
    );

    // Act
    let response = common::get_route_any_status(&api_route).await;
    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "Expected 404 for invalid request, got: {} {:?}",
        response.status(),
        response.text().await
    )
}

// evaluate_redeemers PV2

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn evaluate_redeemers_route_works() {
    // Arrange
    let TestApp { api_address } = common::spawn_app().await;
    let api_route = format!("{api_address}/transactions/evaluate");
    let expected = vec![
        EvaluatedRedeemer {
            redeemer_tag: RedeemerTag::Spend,
            redeemer_index: 0,
            ex_units: ExUnits {
                mem: 426418,
                steps: 100711792,
            },
        },
        EvaluatedRedeemer {
            redeemer_tag: RedeemerTag::Spend,
            redeemer_index: 1,
            ex_units: ExUnits {
                mem: 385832,
                steps: 90941773,
            },
        },
        EvaluatedRedeemer {
            redeemer_tag: RedeemerTag::Mint,
            redeemer_index: 0,
            ex_units: ExUnits {
                mem: 388722,
                steps: 90231806,
            },
        },
    ];

    // 39d10447486a013ea1f52f15593791a95b9faa83b453104c43b2a0ce092de2b4
    let request_body = EvaluateRequest {
        cbor: "84a80084825820649748a242393deac81d6f88a2b7edfec28b1966d1d88b7f0a36e2d8c60ef9e9008258206dba3ccaf44fb402407ad5a435086da0e8a86876cdd276738d7bc8a0915ca5a100825820dd095fe8edadab53933d6cb168a357dddacc9d5e451ed74cca0760d5ce8c146601825820dd095fe8edadab53933d6cb168a357dddacc9d5e451ed74cca0760d5ce8c146603018483583911730d04da38ffb7756072340338098c04413e75fedfb5669289e7c7bf4aee89758c07ed706429631ac4d6e59e617d59f522d9aa6ee50c54d0821b00000287b00945aba2581c804f5544c1962a40546827cab750a88404dc7108c0f588b72964754fa144565946491b000003767bc3e7a5581cc285d6d7e61163b7f7a918f28e450e37d55dc684450d87b96750d8dba140015820ea66bcd5fdbfda6a6b14fbc1795de570ae2fa5174a818bc5a9736fe052e657ea82583901fc444326158549122e7df38e0c826bba1244e2ad42b5cba789598c9bf55d5fc9990a5d2f55f8824fc25e9a0db75926abcd5c8a1abdb9ea24821a001e8480a1581c4d07e0ceae00e6c53598cea00a53c54a94c6b6aa071482244cc0adb5a14f567946695f43726564656e7469616c01825839011b30d3b5b34e5abbba339ebfc56fcaefc6f56224774fec5db9d8e03474714e4c629583b917f6b0b667c39dff7442dd1be175ec6cb0806f20821a001e8480a1581c36babde255ba721444a5c2bf7ea85b0ccbe5452088573d355681e1d9a150567946695f4144412f565946495f4c501a0c33538282583901fc444326158549122e7df38e0c826bba1244e2ad42b5cba789598c9bf55d5fc9990a5d2f55f8824fc25e9a0db75926abcd5c8a1abdb9ea241a2b26aedc021a00099035031a06495c5f07582084011fa48d284f783df73bea623ae974bc8fd254ce1d850ae88545772f0c2d8e09a1581c36babde255ba721444a5c2bf7ea85b0ccbe5452088573d355681e1d9a150567946695f4144412f565946495f4c501a0c3353820b5820555217a59aec20a668cb79a15390f94d2e8eaeaf6be7d1660702e9eba8c6c2510d818258209828e034926c1185bea65ed51aa13d5ca98f30c0a6df379f8302044974fe27b700a40081825820adca84345664f48d3b1276609b37d630809aca1af540d41b1af384f62aadabb55840a537f7810afe7931356001e6af0e39bb4964bd2dc8e8170fe0c7eb8327c904367e497d84e0b3f1873aaa24e3dd8a63aef8967c0b02783448ff1263bb38412e080383590a8f590a8c010000332323232322232322322323253353330093333573466e1cd55cea803a40004646424660020060046464646666ae68cdc3a800a40184642444444460020106eb4d5d09aab9e500323333573466e1d4009200a232122222223002008375a6ae84d55cf280211999ab9a3370ea00690041190911111118018041bad357426aae7940148cccd5cd19b875004480188c848888888c010020dd69aba135573ca00c46666ae68cdc3a802a400842444444400a46666ae68cdc3a8032400446424444444600c0106464646666ae68cdc39aab9d5002480008cc8848cc00400c008dd69aba15002375a6ae84d5d1280111931a99ab9c01d01c01b01a135573ca00226ea8004d5d09aab9e500823333573466e1d401d2000232122222223007008375a6ae84d55cf280491931a99ab9c01a019018017016015014013012011135573aa00226ea8004d5d09aba25008375c6ae85401c8c98d4cd5ce0078070068061999ab9a3370ea0089001109100111999ab9a3370ea00a9000109100091931a99ab9c01000f00e00d00c3333573466e1cd55cea8012400046644246600200600464646464646464646464646666ae68cdc39aab9d500a480008cccccccccc888888888848cccccccccc00402c02802402001c01801401000c008d5d0a80519a80b90009aba1500935742a0106ae85401cd5d0a8031aba1500535742a00866a02eeb8d5d0a8019aba15002357426ae8940088c98d4cd5ce00d80d00c80c09aba25001135744a00226ae8940044d5d1280089aba25001135744a00226ae8940044d5d1280089aab9e5001137540026ae854008c8c8c8cccd5cd19b875001480188c848888c010014c8c8c8c8c8c8cccd5cd19b8750014803084888888800c8cccd5cd19b875002480288488888880108cccd5cd19b875003480208cc8848888888cc004024020dd71aba15005375a6ae84d5d1280291999ab9a3370ea00890031199109111111198010048041bae35742a00e6eb8d5d09aba2500723333573466e1d40152004233221222222233006009008301b35742a0126eb8d5d09aba2500923333573466e1d40192002232122222223007008301c357426aae79402c8cccd5cd19b875007480008c848888888c014020c074d5d09aab9e500c23263533573804003e03c03a03803603403203002e26aae7540104d55cf280189aab9e5002135573ca00226ea8004d5d09aab9e500323333573466e1d400920042321222230020053011357426aae7940108cccd5cd19b875003480088c848888c004014c8c8c8cccd5cd19b8735573aa004900011991091980080180119191999ab9a3370e6aae75400520002375c6ae84d55cf280111931a99ab9c01c01b01a019137540026ae854008dd69aba135744a004464c6a66ae7006406005c0584d55cf280089baa001357426aae7940148cccd5cd19b875004480008c848888c00c014dd71aba135573ca00c464c6a66ae7005805405004c0480440404d55cea80089baa001357426ae8940088c98d4cd5ce007807006806080689931a99ab9c4901035054350000d00c135573ca00226ea80044d55ce9baa001135573ca00226ea800448c88c008dd60009900099180080091191999aab9f0022122002233221223300100400330053574200660046ae8800c01cc0080088c8c8c8c8cccd5cd19b875001480088ccc888488ccc00401401000cdd69aba15004375a6ae85400cdd69aba135744a00646666ae68cdc3a8012400046424460040066464646666ae68cdc3a800a400446424460020066eb8d5d09aab9e500323333573466e1d400920002321223002003375c6ae84d55cf280211931a99ab9c00f00e00d00c00b135573aa00226ea8004d5d09aab9e500623263533573801401201000e00c26aae75400c4d5d1280089aab9e5001137540029309000a481035054310033232323322323232323232323232332223222253350021350012232350032222222222533533355301512001321233001225335002210031001002501e25335333573466e3c0300040540504d40800045407c00c84054404cd4c8c8d4cc8848cc00400c008ccdc624000030004a66a666ae68cdc7a800a4410000b00a150151350165001223355011002001133371802e02e0026a00a4400444004260086a6464646464a66a6666666ae900148cccd5cd19b8735573aa00a900011999aab9f500525019233335573ea00a4a03446666aae7d40149406c8cccd55cf9aba2500625335323232323333333574800846666ae68cdc39aab9d5004480008cccd55cfa8021281191999aab9f500425024233335573e6ae89401494cd4c088d5d0a80390a99a99a811119191919191999999aba400623333573466e1d40092002233335573ea00c4a05e46666aae7d4018940c08cccd55cfa8031281891999aab9f35744a00e4a66a605a6ae854028854cd4c0b8d5d0a80510a99a98179aba1500a21350361223330010050040031503415033150322503203303203103023333573466e1d400d2000233335573ea00e4a06046666aae7cd5d128041299a98171aba150092135033122300200315031250310320312502f02c02b2502d2502d2502d2502d02e135573aa00826ae8940044d5d1280089aab9e5001137540026ae85401c84d40a048cc00400c0085409854094940940980940909408807c940849408494084940840884d5d1280089aab9e5001137540026ae854024854cd4ccd54054070cd54054070060d5d0a80490a99a99a80d00e9aba150092135020123330010040030021501e1501d1501c2501c01d01c01b01a250180152501725017250172501701821001135626135744a00226ae8940044d55cf280089baa00135001223500222222222225335009132635335738921035054380001f01b22100222200232001355011225335001100422135002225335333573466e3c00801c02402040244c01800c488008488004c8004d5403488448894cd40044d400c88004884ccd401488008c010008ccd54c01c480040140100044488c88ccccccd5d2000aa8029299a98019bab002213500f0011500d55005550055500500e3200135500e223233335573e00446a01e2440044a66a600c6aae754008854cd4c018d55cf280190a99a98031aba200521350123212233001003004335500b003002150101500f1500e00f135742002224a0102244246600200600446666666ae900049401c9401c9401c8d4020dd6801128038040911919191999999aba400423333573466e1d40092000233335573ea0084a01846666aae7cd5d128029299a98049aba15006213500f3500f0011500d2500d00e00d23333573466e1d400d2002233335573ea00a46a01ca01a4a01a01c4a0180120104a0144a0144a0144a01401626aae7540084d55cf280089baa00123232323333333574800846666ae68cdc3a8012400446666aae7d4010940288cccd55cf9aba2500525335300a35742a00c426a01a24460020062a0164a01601801646666ae68cdc3a801a400046666aae7d40149402c8cccd55cf9aba2500625335300b35742a00e426a01c24460040062a0184a01801a0184a01400e00c4a0104a0104a0104a01001226aae7540084d55cf280089baa0014988ccccccd5d20009280192801928019280191a8021bae002004121223002003112200112001480e0448c8c00400488cc00cc00800800522011cc285d6d7e61163b7f7a918f28e450e37d55dc684450d87b96750d8db000159085f59085c010000332332232323322323232323322323233223232323322322322232325335330053333573466e1cd55ce9baa00448000806c8c98d4cd5ce00600d80b80b1999ab9a3370e6aae754009200023322123300100300232323232323232323232323333573466e1cd55cea8052400046666666666444444444424666666666600201601401201000e00c00a0080060046ae854028cd40648004d5d0a8049aba1500835742a00e6ae854018d5d0a8029aba1500433501975c6ae85400cd5d0a8011aba135744a004464c6a66ae7006009c08c0884d5d1280089aba25001135744a00226ae8940044d5d1280089aba25001135744a00226ae8940044d55cf280089baa00135742a0046464646666ae68cdc3a800a400c46424444600800a6464646464646666ae68cdc3a800a401842444444400646666ae68cdc3a8012401442444444400846666ae68cdc3a801a40104664424444444660020120106eb8d5d0a8029bad357426ae8940148cccd5cd19b875004480188cc8848888888cc008024020dd71aba15007375c6ae84d5d1280391999ab9a3370ea00a9002119910911111119803004804180d1aba15009375c6ae84d5d1280491999ab9a3370ea00c9001119091111111803804180d9aba135573ca01646666ae68cdc3a803a400046424444444600a01060386ae84d55cf280611931a99ab9c01d02c028027026025024023022021135573aa00826aae79400c4d55cf280109aab9e5001137540026ae84d55cf280191999ab9a3370ea004900211909111180100298081aba135573ca00846666ae68cdc3a801a400446424444600200a6464646666ae68cdc39aab9d5002480008cc8848cc00400c008c8c8cccd5cd19b8735573aa002900011bae357426aae7940088c98d4cd5ce00c81401201189baa00135742a0046eb4d5d09aba2500223263533573802c04a04204026aae7940044dd50009aba135573ca00a46666ae68cdc3a8022400046424444600600a6eb8d5d09aab9e500623263533573802604403c03a03803603426aae7540044dd50009aba135744a004464c6a66ae7003006c05c05840684d401d24010350543500135573ca00226ea8004c888c00cd4c8c8c8c8c94cd4ccccccd5d200291999ab9a3370e6aae7540152000233335573ea00a4a04246666aae7d4014940888cccd55cfa8029281191999aab9f35744a00c4a66a646464646666666ae900108cccd5cd19b8735573aa008900011999aab9f50042502b233335573ea0084a05846666aae7cd5d128029299a98139aba15007215335335027232323232323333333574800c46666ae68cdc3a8012400446666aae7d4018940dc8cccd55cfa8031281c11999aab9f500625039233335573e6ae89401c94cd4c0c8d5d0a80510a99a98199aba1500a215335303435742a014426a07c66605e0060040022a0782a0762a0744a07407006e06c06a46666ae68cdc3a801a400046666aae7d401c940e08cccd55cf9aba2500825335303335742a012426a076605a0022a0724a07206e06c4a06e0620604a06a4a06a4a06a4a06a06626aae7540104d5d1280089aba25001135573ca00226ea8004d5d0a803909a81809198008018010a8170a81692816815815014928150121281492814928149281481389aba25001135573ca00226ea8004d5d0a80490a99a999aa80c81199aa80c81180e9aba1500921533533501f02435742a012426a050246660020080060042a04c2a04a2a0484a04804404204003e4a0400344a03e4a03e4a03e4a03e03a4200226ac4c26ae8940044d5d1280089aab9e5001137540026a002446a0044444444444a66a01226a0229201035054380022100222200232001355018225335001100522135002225335333573466e3c00801c02802440284c01800c48c98d4cd5ce00080a0080910010910009191919191999ab9a3370ea002900111998049bad35742a0086eb4d5d0a8019bad357426ae89400c8cccd5cd19b875002480008c02cc8c8c8cccd5cd19b875001480088c060dd71aba135573ca00646666ae68cdc3a80124000460346eb8d5d09aab9e500423263533573801a03803002e02c26aae7540044dd50009aba135573ca00c464c6a66ae7002005c04c0480444d55cea80189aba25001135573ca00226ea800524103505431001232230023758002640026aa024446666aae7c004940288cd4024c010d5d080118019aba200201121223002003222122333001005004003112232233333335748002aa00a4a66a60066eac00884d404c0045404554015540155401403cc8004d5404088c8cccd55cf80111a809a8049299a98031aab9d5002215335300635573ca00642a66a600c6ae8801484d4058cd402c48cc00401000c004540505404c540480404d5d0800889280608910010910911980080200191999999aba40012500a2500a2500a23500b375a0044a0140102446464646666666ae900108cccd5cd19b875002480008cccd55cfa8021280791999aab9f35744a00a4a66a60126ae85401884d4048d404800454040940400380348cccd5cd19b875003480088cccd55cfa80291a808a80812808007128078048041280692806928069280680589aab9d5002135573ca00226ea80048c8c8c8ccccccd5d200211999ab9a3370ea004900111999aab9f50042500d233335573e6ae89401494cd4c030d5d0a803109a80818058008a8071280700600591999ab9a3370ea006900011999aab9f50052500e233335573e6ae89401894cd4c034d5d0a803909a80898068008a80792807806806128068038031280592805928059280580489aab9d5002135573ca00226ea80052621223002003212230010032333333357480024a0084a0084a0084a00846a00a6eb80080084800448488c00800c4488004448c8c00400488cc00cc00800800522011cc285d6d7e61163b7f7a918f28e450e37d55dc684450d87b96750d8db0001590a1c590a19010000332323232322232323223223232533533300a3333573466e1cd55cea804240004646464246660020080060046eb4d5d09aba25009375a6ae854020dd69aba1500823263533573802001e01c01a6666ae68cdc3a80224004424400446666ae68cdc3a802a40004244002464c6a66ae7004404003c038034cccd5cd19b8735573aa004900011991091980080180119191919191919191919191999ab9a3370e6aae754029200023333333333222222222212333333333300100b00a00900800700600500400300235742a01466a03040026ae854024d5d0a8041aba1500735742a00c6ae854014d5d0a80219a80c3ae35742a0066ae854008d5d09aba2500223263533573803803603403226ae8940044d5d1280089aba25001135744a00226ae8940044d5d1280089aba25001135744a00226aae7940044dd50009aba150023232323333573466e1d400520062321222230040053232323232323333573466e1d4005200c21222222200323333573466e1d4009200a21222222200423333573466e1d400d2008233221222222233001009008375c6ae854014dd69aba135744a00a46666ae68cdc3a8022400c4664424444444660040120106eb8d5d0a8039bae357426ae89401c8cccd5cd19b875005480108cc8848888888cc018024020c070d5d0a8049bae357426ae8940248cccd5cd19b875006480088c848888888c01c020c074d5d09aab9e500b23333573466e1d401d2000232122222223005008301e357426aae7940308c98d4cd5ce01081000f80f00e80e00d80d00c80c09aab9d5004135573ca00626aae7940084d55cf280089baa001357426aae79400c8cccd5cd19b875002480108c848888c008014c048d5d09aab9e500423333573466e1d400d20022321222230010053232323333573466e1cd55cea8012400046644246600200600464646666ae68cdc39aab9d5001480008dd71aba135573ca004464c6a66ae7007407006c0684dd50009aba15002375a6ae84d5d1280111931a99ab9c01a019018017135573ca00226ea8004d5d09aab9e500523333573466e1d40112000232122223003005375c6ae84d55cf280311931a99ab9c017016015014013012011135573aa00226ea8004d5d09aba2500223263533573802001e01c01a201c264c6a66ae71241035054350000e00d135573ca00226ea80044d55ce9baa001135744a00226aae7940044dd50008919118011bac00132001323001001223233335573e00442440044664424466002008006600a6ae8400cc008d5d100180398010011191919191999ab9a3370ea002900111999110911998008028020019bad35742a0086eb4d5d0a8019bad357426ae89400c8cccd5cd19b875002480008c8488c00800cc8c8c8cccd5cd19b875001480088c8488c00400cdd71aba135573ca00646666ae68cdc3a8012400046424460040066eb8d5d09aab9e500423263533573801e01c01a01801626aae7540044dd50009aba135573ca00c464c6a66ae7002802402001c0184d55cea80189aba25001135573ca00226ea8005261200149010350543100332332233223232323232323232323222232322300235323232323253353333333574800a46666ae68cdc39aab9d5005480008cccd55cfa8029280d91999aab9f50052501c233335573ea00a4a03a46666aae7cd5d128031299a991919191999999aba400423333573466e1cd55cea8022400046666aae7d4010940948cccd55cfa8021281311999aab9f35744a00a4a66a603e6ae85401c854cd4cd407c8c8c8c8c8c8ccccccd5d200311999ab9a3370ea004900111999aab9f500625031233335573ea00c4a06446666aae7d4018940cc8cccd55cf9aba2500725335302a35742a01442a66a60566ae854028854cd4c0b0d5d0a805109a81c0911998008028020018a81b0a81a8a81a1281a01801781701691999ab9a3370ea006900011999aab9f500725032233335573e6ae89402094cd4c0acd5d0a804909a81a89118010018a81992819817817128188160159281792817928179281781589aab9d5004135744a00226ae8940044d55cf280089baa00135742a00e426a05424660020060042a0502a04e4a04e0460440424a04803e4a0464a0464a0464a04603e26ae8940044d55cf280089baa00135742a01242a66a666aa02603066aa02603002a6ae854024854cd4cd405c064d5d0a804909a811091998008020018010a8100a80f8a80f1280f00d00c80c00b9280d00a9280c9280c9280c9280c80a9080089ab1309aba25001135744a00226aae7940044dd500099a9806890009a800911a8011111111111004a4004444004640026aa02644a66a00220224426a00444a66a666ae68cdc780100380b00a880b098030019a80191111111a8039100108911911999999aba4001550052533530033756004426a0260022a022aa00aaa00aaa00a01a640026aa02044646666aae7c0088d404c48800894cd4c018d55cea80110a99a98031aab9e500321533530063574400a426a02c24466002246600200c00a0062a0282a0262a02401c26ae8400444940308ccccccd5d200092806128061280611a8069bad0022500c0081223232323333333574800846666ae68cdc3a8012400046666aae7d4010940448cccd55cf9aba2500525335300935742a00c426a0286a0280022a0244a02401c01a46666ae68cdc3a801a400446666aae7d40148d404d4048940480389404403002c9403c9403c9403c9403c02c4d55cea80109aab9e50011375400246464646666666ae900108cccd5cd19b875002480088cccd55cfa8021280791999aab9f35744a00a4a66a60126ae85401884d4048488c00400c540409404003002c8cccd5cd19b875003480008cccd55cfa8029280811999aab9f35744a00c4a66a60146ae85401c84d404c488c00800c54044940440340309403c028024940349403494034940340244d55cea80109aab9e50011375400246666666ae90004940249402494024940248d4028dd7001002990009aa80411091299a999ab9a33710002900000480409a802a481035054360015335002135005490103505437002215335333573466e1c00d200000b00a10021335300612001001337020069001091931a99ab9c0010030024984800448800848800448488c00800c4488004448c8c00400488cc00cc008008004cd4488ccd44888cd4488cd4488ccccccc008cd540112211c4d07e0ceae00e6c53598cea00a53c54a94c6b6aa071482244cc0adb50048810f567946695f43726564656e7469616c0033550044891cc285d6d7e61163b7f7a918f28e450e37d55dc684450d87b96750d8db00488100488110567946695f4144412f565946495f4c500033300948303df9c052014480a0cd540112210048810033550044891c804f5544c1962a40546827cab750a88404dc7108c0f588b72964754f004881045659464900350074891c4aee89758c07ed706429631ac4d6e59e617d59f522d9aa6ee50c54d000222222212333333300100800700600500400300220011122123300100300211200112122300200311220011200122212333001004003002200110483d8799f1a19abde7e1a08f9be911b000002e4631b97e2ffd8799f58381b30d3b5b34e5abbba339ebfc56fcaefc6f56224774fec5db9d8e03474714e4c629583b917f6b0b667c39dff7442dd1be175ec6cb0806f20d8799f1a0c3033efffffd8799f1a19abde7e1a08f9be911b000002e46f4eeb64ff0583840000d87980821a000681b21a0778db81840001d87980821a0005e3281a06c582cb840100d87980821a0005ee721a06aa2675f5a11902a2a1636d73678176567946693a204c50204f726465722050726f63657373".into(),
        additional_utxos: Some(vec![
            AdditionalUtxo {
                tx_hash: "649748a242393deac81d6f88a2b7edfec28b1966d1d88b7f0a36e2d8c60ef9e9".into(),
                index: 0,
                txout_cbor: "83581d713422c13d5a97e68fe725d3d740b7f96cb315e3b44d03a2be28863beb821a0ae72e72a1581c804f5544c1962a40546827cab750a88404dc7108c0f588b72964754fa144565946491a0e9b3eda58205b90f5d273e2d7f7db45dc4f0efdcfe7536250a9abf18dee57abee9c798944c7".into(),
            }
        ])
    };

    // Act
    let redeemers: Vec<EvaluatedRedeemer> = common::post_json_route(&api_route, request_body).await;

    // Assert
    assert_eq!(redeemers, expected);
}

// evaluate_redeemers PV3

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn evaluate_redeemers_pv3_route_works() {
    // Arrange
    let TestApp { api_address } = common::spawn_app().await;
    let api_route = format!("{api_address}/transactions/evaluate");
    let expected = vec![EvaluatedRedeemer {
        redeemer_tag: RedeemerTag::Propose,
        redeemer_index: 0,
        ex_units: ExUnits {
            mem: 576238,
            steps: 121963335,
        },
    }];

    // f6cb185a1fe988dad1011eb239d9a281e8abd3557417918cffd7ef9e7afcf599
    let request_body = EvaluateRequest {
        cbor: "84a600d901028182582009b70e56d4c306808fe8d8af1e0837409a0c1f3029173f300fa97ad938458ee3010dd901028182582009b70e56d4c306808fe8d8af1e0837409a0c1f3029173f300fa97ad938458ee30001818258390081642bb711d53313dde9dbd991d621414c6d2bdf06a9181932f2154231e66d448f7fe215f77ab30eb7409585f9196c7037487130b8c81eaa1b000000bc796b688e021a0004d57c0b582020404426e705a652c0bafdcfe4301b6f7a4e4e5110bc2aba664625fcea3deb1d14d9010281841b000000174876e800581de031e66d448f7fe215f77ab30eb7409585f9196c7037487130b8c81eaa8400f6a115821a000493e0197530581cfa24fb305126805cf2164c161d852a0e7330cf988f1fe558cf7d4a64827668747470733a2f2f6269742e6c792f337a434832484c58201111111111111111111111111111111111111111111111111111111111111111a307d90102815908545908510101003232323232323232323232323232323232323232323232323232323232323232323232323232323232259323255333573466e1d20000011180098111bab357426ae88d55cf00104554ccd5cd19b87480100044600422c6aae74004dd51aba1357446ae88d55cf1baa3255333573466e1d200a35573a002226ae84d5d11aab9e00111637546ae84d5d11aba235573c6ea800642b26006003149a2c8a4c301f801c0052000c00e0070018016006901e4070c00e003000c00d20d00fc000c0003003800a4005801c00e003002c00d20c09a0c80e1801c006001801a4101b5881380018000600700148013003801c006005801a410100078001801c006001801a4101001f8001800060070014801b0038018096007001800600690404002600060001801c0052008c00e006025801c006001801a41209d8001800060070014802b003801c006005801a410112f501c3003800c00300348202b7881300030000c00e00290066007003800c00b003482032ad7b806038403060070014803b00380180960003003800a4021801c00e003002c00d20f40380e1801c006001801a41403f800100a0c00e0029009600f0030078040c00e002900a600f003800c00b003301a483403e01a600700180060066034904801e00060001801c0052016c01e00600f801c006001801980c2402900e30000c00e002901060070030128060c00e00290116007003800c00b003483c0ba03860070018006006906432e00040283003800a40498003003800a404d802c00e00f003800c00b003301a480cb0003003800c003003301a4802b00030001801c01e0070018016006603490605c0160006007001800600660349048276000600030000c00e0029014600b003801c00c04b003800c00300348203a2489b00030001801c00e006025801c006001801a4101b11dc2df80018000c0003003800a4055802c00e007003012c00e003000c00d2080b8b872c000c0006007003801809600700180060069040607e4155016000600030000c00e00290166007003012c00e003000c00d2080c001c000c0003003800a405d801c00e003002c00d20c80180e1801c006001801a412007800100a0c00e00290186007003013c0006007001480cb005801801e006003801800e00600500403003800a4069802c00c00f003001c00c007003803c00e003002c00c05300333023480692028c0004014c00c00b003003c00c00f003003c00e00f003800c00b00301480590052008003003800a406d801c00e003002c00d2000c00d2006c00060070018006006900a600060001801c0052038c00e007001801600690006006901260003003800c003003483281300020141801c005203ac00e006027801c006001801a403d800180006007001480f3003801804e00700180060069040404af3c4e302600060001801c005203ec00e006013801c006001801a4101416f0fd20b80018000600700148103003801c006005801a403501c3003800c0030034812b00030000c00e0029021600f003800c00a01ac00e003000c00ccc08d20d00f4800b00030000c0000000000803c00c016008401e006009801c006001801807e0060298000c000401e006007801c0060018018074020c000400e00f003800c00b003010c000802180020070018006006019801805e0003000400600580180760060138000800c00b00330134805200c400e00300080330004006005801a4001801a410112f58000801c00600901260008019806a40118002007001800600690404a75ee01e00060008018046000801801e000300c4832004c025201430094800a0030028052003002c00d2002c000300648010c0092002300748028c0312000300b48018c0292012300948008c0212066801a40018000c0192008300a2233335573e00250002801994004d55ce800cd55cf0008d5d08014c00cd5d10011263009222532900389800a4d2219002912c80344c01526910c80148964cc04cdd68010034564cc03801400626601800e0071801226601800e01518010096400a3000910c008600444002600244004a664600200244246466004460044460040064600444600200646a660080080066a00600224446600644b20051800484ccc02600244666ae68cdc3801000c00200500a91199ab9a33710004003000801488ccd5cd19b89002001800400a44666ae68cdc4801000c00a00122333573466e20008006005000912a999ab9a3371200400222002220052255333573466e2400800444008440040026eb400a42660080026eb000a4264666015001229002914801c8954ccd5cd19b8700400211333573466e1c00c006001002118011229002914801c88cc044cdc100200099b82002003245200522900391199ab9a3371066e08010004cdc1001001c002004403245200522900391199ab9a3371266e08010004cdc1001001c00a00048a400a45200722333573466e20cdc100200099b820020038014000912c99807001000c40062004912c99807001000c400a2002001199919ab9a357466ae880048cc028dd69aba1003375a6ae84008d5d1000934000dd60010a40064666ae68d5d1800c0020052225933006003357420031330050023574400318010600a444aa666ae68cdc3a400000222c22aa666ae68cdc4000a4000226600666e05200000233702900000088994004cdc2001800ccdc20010008cc010008004c01088954ccd5cd19b87480000044400844cc00c004cdc300100091119803112c800c60012219002911919806912c800c4c02401a442b26600a004019130040018c008002590028c804c8888888800d1900991111111002a244b267201722222222008001000c600518000001112a999ab9a3370e004002230001155333573466e240080044600823002229002914801c88ccd5cd19b893370400800266e0800800e00100208c8c0040048c0088cc00800800505a182050082d87980821a0008caee1a0745034700818258205179f9b97f45223176e54302e82b94d1cf849ea8dcc068d641adca4aa6c653e958404a8040c01dfa96066037d7d2ae8191a1e80d12801c261bdcf3f33540138c66c9c5875f4d5b0a1694fe744dace5854ee975bea27551a6a01b1d2d003c0e0fb603f5f6".into(),
        additional_utxos: Some(vec![
            AdditionalUtxo {
                tx_hash: "09b70e56d4c306808fe8d8af1e0837409a0c1f3029173f300fa97ad938458ee3".into(),
                index: 1,
                txout_cbor: "8258390081642bb711d53313dde9dbd991d621414c6d2bdf06a9181932f2154231e66d448f7fe215f77ab30eb7409585f9196c7037487130b8c81eaa1b000000d3c1e7260a".into(),
            }
        ])
    };

    // Act
    let redeemers: Vec<EvaluatedRedeemer> = common::post_json_route(&api_route, request_body).await;

    // Assert
    assert_eq!(redeemers, expected);
}

// tx_cbor_by_tx_hash

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn tx_cbor_by_tx_hash_route_works() {
    // Arrange
    let TestApp { api_address } = common::spawn_app().await;
    let api_route = format!("{api_address}/transactions/{TEST_TX_HASH}/cbor");
    let expected_item = "84a700818258208a0f8e8cb8e55765970402fd2ef7a7a05e150dc1a1bcf995bfde26a8d08afafd181c018c825839019b70b6f8225ac0ee35592d43524bd99cc351951c1344c69dbdad251e0ad90f1d15272951c81f24f654879b300e0f2326b1abf2fe5d156cfd821a00169b08a1581cf0ff48bbb7bbe9d59a40f1ce90e9e9d0ff5002ec48f232b49ca0fb9aa14d63617264616e6f7377656574730182582841736dfcaa011821cb7d71ad7d546a750c3d6d059a13b35d56bc36d1e38dcb2095eabb49cfe8c63d821a00160a5ba1581cf0ff48bbb7bbe9d59a40f1ce90e9e9d0ff5002ec48f232b49ca0fb9aa1476164616d616e7401825839019defeeed14b9cf47c4e07bc792a69b9f6fbc50e60abe04f60b4d8262910adf8e77e0b2a8a812fe0c4dd63807fc1fb4f52931121ccf1f0b5b821a00169b08a1581cf0ff48bbb7bbe9d59a40f1ce90e9e9d0ff5002ec48f232b49ca0fb9aa14c70616e63616b655f7377617001825839013069df66642f7b3bd124ee47154153fa8a9d8cdd1ed49f1f3685ceb4e30d23391ce8b90c0826afe194b0805b367489961aa7db54a82fe8fa821a00160a5ba1581cf0ff48bbb7bbe9d59a40f1ce90e9e9d0ff5002ec48f232b49ca0fb9aa147647261676f6e6d01825839016baf49640d9c9103b86b8c0651c744ea424ee285d16b772131215a4042445c7d04c390029ebcd12df8c30d039ec9e077be171c5305b05ada821a00169b08a1581cf0ff48bbb7bbe9d59a40f1ce90e9e9d0ff5002ec48f232b49ca0fb9aa1497061792e64796c616e0182583901e3c3529dc8e7005fd63f66d8f68afc56dbd80e65f366a32f4bbd91052382cfd7fcd8b567984850d3e4409d5559f7d3be342c755d581eb041821a00160a5ba1581cf0ff48bbb7bbe9d59a40f1ce90e9e9d0ff5002ec48f232b49ca0fb9aa1446264736d0182583901ff4f036c99f49e81712dbb63e34bba5cf4df92b173554615faec726f22ac90011ddf6493ac89818e24567297cb1c9037177cefeb2187da9a821a00169b08a1581cf0ff48bbb7bbe9d59a40f1ce90e9e9d0ff5002ec48f232b49ca0fb9aa149796f726f692e6164610182583901c3a05245ea1d143684b4e30ab81c6078cbaa751e5c3acec6b5ec29c4faec10327f326217bf2ee08ca2920220320538cb815bec5ea1f6d81a821a00160a5ba1581cf0ff48bbb7bbe9d59a40f1ce90e9e9d0ff5002ec48f232b49ca0fb9aa1466775796775790182583901bc9d96ea73c428bad3ad7517692c580163bed0ff95917e74fabd536446d5b0f299e8f7f23b840891c65837d6d0c9859cc8d30243d396106a821a00160a5ba1581cf0ff48bbb7bbe9d59a40f1ce90e9e9d0ff5002ec48f232b49ca0fb9aa146746578616e7301825839015fe4376aeebbb2f8ec1a484f303780111bc37023909e759f4426bfbf97c7416d6ffdb28d0867558a23315593bf3e02763740541b6dc80e9d821a00160a5ba1581cf0ff48bbb7bbe9d59a40f1ce90e9e9d0ff5002ec48f232b49ca0fb9aa1486d6f6e73616e746f01825839017d9a77835cb488079670e0024745f8ba022ff0c57f4c8bd10d181c7c0e22014fd488a4be1cb58581312399b78ab933c10fc70f0fff1eceab821a00169b08a1581cf0ff48bbb7bbe9d59a40f1ce90e9e9d0ff5002ec48f232b49ca0fb9aa14b64696f6e616561747261700182583901fbbb5ac3afdbb06bf37d143a23204e6d771c9e7c2c92f6b05c9268283f5d3d13f414ddac483c81ed09742b52185bde1f499c2d2d3aed44951a169abd7a021a00064fdd031a03526123075820fd3d037f14616c69a93b019ec614fc3fbae20d2a6f8563a919a862218ebc46ca080009a1581cf0ff48bbb7bbe9d59a40f1ce90e9e9d0ff5002ec48f232b49ca0fb9aab476164616d616e7401446264736d014d63617264616e6f737765657473014b64696f6e616561747261700147647261676f6e6d014667757967757901486d6f6e73616e746f014c70616e63616b655f7377617001497061792e64796c616e0146746578616e730149796f726f692e61646101a2008c82582082202ed922d20e73b363376c72497aa130301dfe5a198c6e02f16894d945803258409c6e23e39ef7e614d9500a6b8b908ed77a83c6d95dcd839f7b9d666bedb04be937b8b591cc010a5d4799f4cb82a893f24f54d5b4d8b4f9a1d223583161c9ba07825820b62a7193927f47ef703ba99a06add1ee2d4d94853a16dcd68ad0f5d9baa185305840747f83967c81b009158b8eb8efa71017e6266a7f374d9a9b1903e8706a602984ce64f7f0e0d40d3f56000ff9c66c1ca0928ef8cb84b5009cf3b6995bb2ec870c825820b62a7193927f47ef703ba99a06add1ee2d4d94853a16dcd68ad0f5d9baa185305840747f83967c81b009158b8eb8efa71017e6266a7f374d9a9b1903e8706a602984ce64f7f0e0d40d3f56000ff9c66c1ca0928ef8cb84b5009cf3b6995bb2ec870c825820b62a7193927f47ef703ba99a06add1ee2d4d94853a16dcd68ad0f5d9baa185305840747f83967c81b009158b8eb8efa71017e6266a7f374d9a9b1903e8706a602984ce64f7f0e0d40d3f56000ff9c66c1ca0928ef8cb84b5009cf3b6995bb2ec870c825820b62a7193927f47ef703ba99a06add1ee2d4d94853a16dcd68ad0f5d9baa185305840747f83967c81b009158b8eb8efa71017e6266a7f374d9a9b1903e8706a602984ce64f7f0e0d40d3f56000ff9c66c1ca0928ef8cb84b5009cf3b6995bb2ec870c825820b62a7193927f47ef703ba99a06add1ee2d4d94853a16dcd68ad0f5d9baa185305840747f83967c81b009158b8eb8efa71017e6266a7f374d9a9b1903e8706a602984ce64f7f0e0d40d3f56000ff9c66c1ca0928ef8cb84b5009cf3b6995bb2ec870c825820b62a7193927f47ef703ba99a06add1ee2d4d94853a16dcd68ad0f5d9baa185305840747f83967c81b009158b8eb8efa71017e6266a7f374d9a9b1903e8706a602984ce64f7f0e0d40d3f56000ff9c66c1ca0928ef8cb84b5009cf3b6995bb2ec870c825820b62a7193927f47ef703ba99a06add1ee2d4d94853a16dcd68ad0f5d9baa185305840747f83967c81b009158b8eb8efa71017e6266a7f374d9a9b1903e8706a602984ce64f7f0e0d40d3f56000ff9c66c1ca0928ef8cb84b5009cf3b6995bb2ec870c825820b62a7193927f47ef703ba99a06add1ee2d4d94853a16dcd68ad0f5d9baa185305840747f83967c81b009158b8eb8efa71017e6266a7f374d9a9b1903e8706a602984ce64f7f0e0d40d3f56000ff9c66c1ca0928ef8cb84b5009cf3b6995bb2ec870c825820b62a7193927f47ef703ba99a06add1ee2d4d94853a16dcd68ad0f5d9baa185305840747f83967c81b009158b8eb8efa71017e6266a7f374d9a9b1903e8706a602984ce64f7f0e0d40d3f56000ff9c66c1ca0928ef8cb84b5009cf3b6995bb2ec870c825820b62a7193927f47ef703ba99a06add1ee2d4d94853a16dcd68ad0f5d9baa185305840747f83967c81b009158b8eb8efa71017e6266a7f374d9a9b1903e8706a602984ce64f7f0e0d40d3f56000ff9c66c1ca0928ef8cb84b5009cf3b6995bb2ec870c825820b62a7193927f47ef703ba99a06add1ee2d4d94853a16dcd68ad0f5d9baa185305840747f83967c81b009158b8eb8efa71017e6266a7f374d9a9b1903e8706a602984ce64f7f0e0d40d3f56000ff9c66c1ca0928ef8cb84b5009cf3b6995bb2ec870c018b8200581c4da965a049dfd15ed1ee19fba6e2974a0b79fc416dd1796a1f97f5e18200581c4da965a049dfd15ed1ee19fba6e2974a0b79fc416dd1796a1f97f5e18200581c4da965a049dfd15ed1ee19fba6e2974a0b79fc416dd1796a1f97f5e18200581c4da965a049dfd15ed1ee19fba6e2974a0b79fc416dd1796a1f97f5e18200581c4da965a049dfd15ed1ee19fba6e2974a0b79fc416dd1796a1f97f5e18200581c4da965a049dfd15ed1ee19fba6e2974a0b79fc416dd1796a1f97f5e18200581c4da965a049dfd15ed1ee19fba6e2974a0b79fc416dd1796a1f97f5e18200581c4da965a049dfd15ed1ee19fba6e2974a0b79fc416dd1796a1f97f5e18200581c4da965a049dfd15ed1ee19fba6e2974a0b79fc416dd1796a1f97f5e18200581c4da965a049dfd15ed1ee19fba6e2974a0b79fc416dd1796a1f97f5e18200581c4da965a049dfd15ed1ee19fba6e2974a0b79fc416dd1796a1f97f5e1f5a11902d1a178386630666634386262623762626539643539613430663163653930653965396430666635303032656334386632333262343963613066623961ab6d63617264616e6f737765657473a6646e616d656e2463617264616e6f7377656574736b6465736372697074696f6e735468652048616e646c65205374616e6461726467776562736974657568747470733a2f2f61646168616e646c652e636f6d65696d6167657835697066733a2f2f516d614e6f56706b7333674152346f61794d6b7839754855326261444d47517242417a704d31505a3365624a427564636f7265a5626f67006a7465726d736f66757365781968747470733a2f2f61646168616e646c652e636f6d2f746f756e68616e646c65456e636f64696e67657574662d386670726566697861246776657273696f6e006d6175676d656e746174696f6e7380676164616d616e74a6646e616d6568246164616d616e746b6465736372697074696f6e735468652048616e646c65205374616e6461726467776562736974657568747470733a2f2f61646168616e646c652e636f6d65696d6167657835697066733a2f2f516d6169386977453144697735595a5659537053504162584d354156707268657665384141584a7a77586f66746664636f7265a5626f67006a7465726d736f66757365781968747470733a2f2f61646168616e646c652e636f6d2f746f756e68616e646c65456e636f64696e67657574662d386670726566697861246776657273696f6e006d6175676d656e746174696f6e73806c70616e63616b655f73776170a6646e616d656d2470616e63616b655f737761706b6465736372697074696f6e735468652048616e646c65205374616e6461726467776562736974657568747470733a2f2f61646168616e646c652e636f6d65696d6167657835697066733a2f2f516d6246654274713863764d71694e386e73387632616a775a6a57653174626b684144645975734348457a6b6a4564636f7265a5626f67006a7465726d736f66757365781968747470733a2f2f61646168616e646c652e636f6d2f746f756e68616e646c65456e636f64696e67657574662d386670726566697861246776657273696f6e006d6175676d656e746174696f6e738067647261676f6e6da6646e616d656824647261676f6e6d6b6465736372697074696f6e735468652048616e646c65205374616e6461726467776562736974657568747470733a2f2f61646168616e646c652e636f6d65696d6167657835697066733a2f2f516d54546b6d58724158614768356863665a3279336a5754376b727263786a6b47655842577531696b6837646a5a64636f7265a5626f67006a7465726d736f66757365781968747470733a2f2f61646168616e646c652e636f6d2f746f756e68616e646c65456e636f64696e67657574662d386670726566697861246776657273696f6e006d6175676d656e746174696f6e7380697061792e64796c616ea6646e616d656a247061792e64796c616e6b6465736372697074696f6e735468652048616e646c65205374616e6461726467776562736974657568747470733a2f2f61646168616e646c652e636f6d65696d6167657835697066733a2f2f516d5136343555784551667838325a5533736d364d35734d36326766585347636270714474565a6e6a617379796f64636f7265a5626f67006a7465726d736f66757365781968747470733a2f2f61646168616e646c652e636f6d2f746f756e68616e646c65456e636f64696e67657574662d386670726566697861246776657273696f6e006d6175676d656e746174696f6e7380646264736da6646e616d6565246264736d6b6465736372697074696f6e735468652048616e646c65205374616e6461726467776562736974657568747470733a2f2f61646168616e646c652e636f6d65696d6167657835697066733a2f2f516d66547079337962574c317465434d5641696568376174487459677063435a35457735386553557848675a466264636f7265a5626f67006a7465726d736f66757365781968747470733a2f2f61646168616e646c652e636f6d2f746f756e68616e646c65456e636f64696e67657574662d386670726566697861246776657273696f6e006d6175676d656e746174696f6e738069796f726f692e616461a6646e616d656a24796f726f692e6164616b6465736372697074696f6e735468652048616e646c65205374616e6461726467776562736974657568747470733a2f2f61646168616e646c652e636f6d65696d6167657835697066733a2f2f516d66534d4e4d68567544645432376959313571555350327a467a584136634173767a4e59634b5768655836447264636f7265a5626f67006a7465726d736f66757365781968747470733a2f2f61646168616e646c652e636f6d2f746f756e68616e646c65456e636f64696e67657574662d386670726566697861246776657273696f6e006d6175676d656e746174696f6e738066677579677579a6646e616d6567246775796775796b6465736372697074696f6e735468652048616e646c65205374616e6461726467776562736974657568747470733a2f2f61646168616e646c652e636f6d65696d6167657835697066733a2f2f516d527132515162646e55687364553653354879436b374b747839755735674153747531737a487a556b37766f6364636f7265a5626f67006a7465726d736f66757365781968747470733a2f2f61646168616e646c652e636f6d2f746f756e68616e646c65456e636f64696e67657574662d386670726566697861246776657273696f6e006d6175676d656e746174696f6e738066746578616e73a6646e616d656724746578616e736b6465736372697074696f6e735468652048616e646c65205374616e6461726467776562736974657568747470733a2f2f61646168616e646c652e636f6d65696d6167657835697066733a2f2f516d5535754748716337734c5069616363416e794b326772476a63444d38415758427448367a525639446d65383364636f7265a5626f67006a7465726d736f66757365781968747470733a2f2f61646168616e646c652e636f6d2f746f756e68616e646c65456e636f64696e67657574662d386670726566697861246776657273696f6e006d6175676d656e746174696f6e7380686d6f6e73616e746fa6646e616d6569246d6f6e73616e746f6b6465736372697074696f6e735468652048616e646c65205374616e6461726467776562736974657568747470733a2f2f61646168616e646c652e636f6d65696d6167657835697066733a2f2f516d554673706e5a514d5751795a396e79364e426b6e36796d52766f39366f6939563765675852724b4751474c3264636f7265a5626f67006a7465726d736f66757365781968747470733a2f2f61646168616e646c652e636f6d2f746f756e68616e646c65456e636f64696e67657574662d386670726566697861246776657273696f6e006d6175676d656e746174696f6e73806b64696f6e61656174726170a6646e616d656c2464696f6e616561747261706b6465736372697074696f6e735468652048616e646c65205374616e6461726467776562736974657568747470733a2f2f61646168616e646c652e636f6d65696d6167657835697066733a2f2f516d50685a7a44686943356236444345465848487637716e79765a5979393443347376564869343137435938625864636f7265a5626f67006a7465726d736f66757365781968747470733a2f2f61646168616e646c652e636f6d2f746f756e68616e646c65456e636f64696e67657574662d386670726566697861246776657273696f6e006d6175676d656e746174696f6e7380";

    // Act
    let tx_cbor: TimestampedResponse<TxCbor> = common::get_route(&api_route).await;

    // Assert
    assert_eq!(tx_cbor.data.0, expected_item)
}

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn tx_cbor_by_tx_hash_no_data() {
    // Arrange
    let TestApp { api_address } = common::spawn_app().await;
    let api_route = format!(
        "{api_address}/transactions/{}/cbor",
        "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
    );

    // Act
    let response = common::get_route_any_status(&api_route).await;
    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "Expected 404 for invalid request, got: {} {:?}",
        response.status(),
        response.text().await
    )
}

// TODO tx info: uses dbsync and polyphony

// txo_by_txo_ref

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn txo_by_txo_ref_route_works() {
    // Arrange
    let TestApp { api_address } = common::spawn_app().await;
    let api_route = format!(
        "{api_address}/transactions/{TEST_TX_HASH}/outputs/{}/txo",
        1
    );
    let expected_item = UtxoWithBytes {
        tx_hash: "257916e7ae112cf16f27218e41bfa37c018bab922354201b3b38c9a24c35ab33".into(),
        index: 1,
        assets: vec![
            Asset {
                unit: "lovelace".into(),
                amount: NumOrString::U64(1444443),
            },
            Asset {
                unit: "f0ff48bbb7bbe9d59a40f1ce90e9e9d0ff5002ec48f232b49ca0fb9a6164616d616e74"
                    .into(),
                amount: NumOrString::U64(1),
            },
        ],
        address: TEST_ADDRESS.into(),
        datum: None,
        reference_script: None,
        txout_cbor: None,
    };

    // Act
    let txo: TimestampedResponse<UtxoWithBytes> = common::get_route(&api_route).await;

    // Assert
    assert_eq!(txo.data, expected_item);
}

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn txo_by_txo_ref_inline_datum() {
    // Arrange
    let TestApp { api_address } = common::spawn_app().await;
    let api_route = format!(
        "{api_address}/transactions/{}/outputs/{}/txo",
        TEST_TXO_INLINE_DATUM.0, TEST_TXO_INLINE_DATUM.1
    );
    let expected_item = Some(DatumOption {
        // from cardanoscan
        datum_type: DatumOptionType::Inline,
        hash: "5f7f360208c46d739296ea3ff4f41d85240d94aa3f59e5b1c82c2bbeceb57f23".into(),
        bytes: Some("d8799f581ce1d915c10c840017bd39088a82507b27150a438e8907784221491309581ce1d915c10c840017bd39088a82507b27150a438e8907784221491309a140a14001a140a1401a009896801a9a7ec8001b00000183c392c9a1ff".into()),
        // from dbsync
        json: serde_json::from_str(r#"{"fields":[{"bytes":"e1d915c10c840017bd39088a82507b27150a438e8907784221491309"},{"bytes":"e1d915c10c840017bd39088a82507b27150a438e8907784221491309"},{"map":[{"k":{"bytes":""},"v":{"map":[{"k":{"bytes":""},"v":{"int":1}}]}}]},{"map":[{"k":{"bytes":""},"v":{"map":[{"k":{"bytes":""},"v":{"int":10000000}}]}}]},{"int":2592000000},{"int":1665433520545}],"constructor":0}"#).unwrap(),
    });

    // Act
    let txo: TimestampedResponse<UtxoWithBytes> = common::get_route(&api_route).await;

    // Assert
    assert_eq!(txo.data.datum, expected_item);
}

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn txo_by_txo_ref_resolved_datum_hash() {
    // Arrange
    let TestApp { api_address } = common::spawn_app().await;
    let api_route = format!(
        "{api_address}/transactions/{}/outputs/{}/txo?resolve_datums=true",
        TEST_TXO_DATUM_HASH.0, TEST_TXO_DATUM_HASH.1
    );
    let expected = Some(DatumOption {
        // from cardanoscan
        datum_type: DatumOptionType::Hash,
        hash: TEST_DATUM_HASH.into(),
        bytes: Some("d8799fd8799fd8799f581cfc6e1b47816bc4a4165a39f3c6dd65868e6b74a743dc2dcf54eb2d4cffd8799fd8799fd8799f581cd756fb7ceee90ed5af3f193d73f9970ca640ab540eedac272cb28c29ffffffffd8799fd8799f581cfc6e1b47816bc4a4165a39f3c6dd65868e6b74a743dc2dcf54eb2d4cffd8799fd8799fd8799f581cd756fb7ceee90ed5af3f193d73f9970ca640ab540eedac272cb28c29ffffffffd87a80d87a9fd8799f581c25f0fc240e91bd95dcdaebd2ba7713fc5168ac77234a3d79449fc20c47534f4349455459ff1a0bebc200ff1a001e84801a001e8480ff".into()),
        // from dbsync
        json: serde_json::from_str(r#"{"fields":[{"fields":[{"fields":[{"bytes":"fc6e1b47816bc4a4165a39f3c6dd65868e6b74a743dc2dcf54eb2d4c"}],"constructor":0},{"fields":[{"fields":[{"fields":[{"bytes":"d756fb7ceee90ed5af3f193d73f9970ca640ab540eedac272cb28c29"}],"constructor":0}],"constructor":0}],"constructor":0}],"constructor":0},{"fields":[{"fields":[{"bytes":"fc6e1b47816bc4a4165a39f3c6dd65868e6b74a743dc2dcf54eb2d4c"}],"constructor":0},{"fields":[{"fields":[{"fields":[{"bytes":"d756fb7ceee90ed5af3f193d73f9970ca640ab540eedac272cb28c29"}],"constructor":0}],"constructor":0}],"constructor":0}],"constructor":0},{"fields":[],"constructor":1},{"fields":[{"fields":[{"bytes":"25f0fc240e91bd95dcdaebd2ba7713fc5168ac77234a3d79449fc20c"},{"bytes":"534f4349455459"}],"constructor":0},{"int":200000000}],"constructor":1},{"int":2000000},{"int":2000000}],"constructor":0}"#).unwrap(),
    });

    // Act
    let txo: TimestampedResponse<UtxoWithBytes> = common::get_route(&api_route).await;

    // Assert
    assert_eq!(txo.data.datum, expected);
}

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn txo_by_txo_ref_reference_script_pv2() {
    // Arrange
    let TestApp { api_address } = common::spawn_app().await;
    let api_route = format!(
        "{api_address}/transactions/{}/outputs/{}/txo",
        TEST_TXO_REF_SCRIPT_PV2.0, TEST_TXO_REF_SCRIPT_PV2.1
    );
    let expected = Some(Script {
        // from cardanoscan
        script_type: ScriptType::PlutusV2,
        hash: "91a8746ce94cedb221c503b645007449ec0ed509759fa0a4f7ac4e6b".into(),
        bytes: "59056201000032323232323232323232323232323232323232323232323232323222233335734646666ae68cc050dd61aba1357446010002466ebc004c0780149289198101198109198111198099808998080029191999ab9a3370e6aae74dd5000a4000466ebcc08002cc090cc08c009200024a093181118110009804804119baf33301c32374e6660020026eb0c0880192f5c04446666ae68d5d18011001119aba03233333573466e1cd55ce9baa00148000893001010000224c01010100498004c078c084d5d08019998020021aba200300249888cc06c0080053010100004c0101010023015337106eb4c080c080c080d5d098058021bad300a0072323301230103300f0042330232323333573466e1cd55ce9baa001480088cdd798121811181218121810191998008009bac3025302500a23375e604c002604c60440164446666ae68d5d18011311999ab9a30023574200646ae840108ccc014014d5d1002001a4c93181218110011250498c088c0880088c8cccd5cd19b8735573a0029002119baf3374a900019aba0302400b335740604001666ae80014cd5d0180600599aba0300d00b33574066038601c016601a01697ae0357426aae7800892824c6ea8c06c008004ccc04088cdc0801000980c003180380324c60380024931324c46ae84c01c0048d5d0980280091aba130030012357446ae88c0080048d5d1180100091aba23002001235744601a002446e9cc8ccc004004dd618068018011111999ab9a35746004497ae023333573460046ae8400c8cd5d01aba100433300500535744008006466600a00a6ae8801000d264988ccc03000488ccc01088cdc00010008011807800a6101a000222332232374c6660020026601600600497adef6c60222333357346ae8c00880088cc88c8cccd5cd1aba30012003233574066ec0010dd3001001a4c6644646660020026602600600497adef6c60222333357346ae8c00880088cc88c8cccd5cd19b8700148000800c8cd5d019bb0004375000400693198099980b0040011980b0038011aba1003333004004357440060049319806804001198068038011aba100333300400435744006004931bab0023756002446644646600200266012006004446666ae68d5d18009251232333357346016664464660020026601e006004446666ae68d5d18009251232333357346022602266e20cc040018004cc0400140049281198028029aba2004498d5d080124c6601200c0026601200a00249408cc014014d5d100224c6ae840092637560046eac00488c8cc00400400c88cccd5cd1aba300124bd6f7b63011999ab9a3375e6aae74d5d080100211bab35573c6ae8400c8cc010010d5d1001a4c9311191998008008018011111999ab9a357460044900011999ab9a3375e6aae74d5d080180111bad35573c6ae840108ccc014014d5d1002001a4c931199ab9a0014a094488c8c8c8ccc004004ccc00800800c01000c888cccd5cd1aba3001200323357406ae84008ccc01001000cd5d100124c4446666ae68d5d1800925eb808c8cccd5cd19804803119baf001002233300600600535744008466ae80008ccc018018014d5d100224c6aae74d5d080124c6466002002006446666ae68d5d1800925eb808cd5d01aab9d35742004660060066ae88009262232333001001003002222333357346ae8c00892811999ab9a30023574200649448ccc014014d5d1002001a4c93111ba8337006eb4008dd680091aba1300200123574460080024446466660020026eb001000c0088888cccd5cd1aba300320022333300500535744008006660060046ae8401126235742600400246ae88c0140048cc0080052002223333573466e1cd55ce9baa0020012003264988d5d0980100091aab9e3754002446666ae68c009262300249892824c1".into(),
        // only native scripts can be encoded in json
        json: None,
    });

    // Act
    let txo: TimestampedResponse<UtxoWithBytes> = common::get_route(&api_route).await;

    // Assert
    assert_eq!(txo.data.reference_script, expected);
}

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn txo_by_txo_ref_no_data() {
    // Arrange
    let TestApp { api_address } = common::spawn_app().await;
    let api_route = format!(
        "{api_address}/transactions/{}/outputs/{}/txo",
        "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef", 0
    );

    // Act
    let response = common::get_route_any_status(&api_route).await;

    // Assert
    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "Expected 404 for invalid request, got: {} {:?}",
        response.status(),
        response.text().await
    )
}

// txos_by_txo_refs

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn txos_by_txo_refs_route_works() {
    // Arrange
    let TestApp { api_address } = common::spawn_app().await;
    let api_route = format!("{api_address}/transactions/outputs");
    let expected_length = 2;
    let request_body = vec![format!("{TEST_TX_HASH}#0"), format!("{TEST_TX_HASH_2}#1")];

    // Act
    let utxos: PaginatedResponse<UtxoWithBytes> =
        common::post_json_route(&api_route, request_body).await;

    // Assert
    assert_eq!(utxos.data.len(), expected_length);
}

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn txos_by_txo_refs_pagination() {
    // Arrange
    let TestApp { api_address } = common::spawn_app().await;
    let request_body = vec![format!("{TEST_TX_HASH}#0"), format!("{TEST_TX_HASH_2}#1")];
    let api_route = format!("{api_address}/transactions/outputs?count=1");
    let expected_length = 1;

    // Act
    let utxos: PaginatedResponse<UtxoWithBytes> =
        common::post_json_route(&api_route, request_body).await;

    // Assert
    assert_eq!(utxos.data.len(), expected_length);

    // Arrange
    let request_body = vec![format!("{TEST_TX_HASH}#0"), format!("{TEST_TX_HASH_2}#1")];
    let api_route = format!(
        "{api_address}/transactions/outputs?count=1&cursor={}",
        utxos.next_cursor.unwrap()
    );
    let expected_length = 1;

    // Act
    let utxos: PaginatedResponse<UtxoWithBytes> =
        common::post_json_route(&api_route, request_body).await;

    // Assert
    assert_eq!(utxos.data.len(), expected_length);
    assert!(utxos.next_cursor.is_none());
}

// chain_tip

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn chain_tip_route_works() {
    // Arrange
    let TestApp { api_address } = common::spawn_app().await;
    let api_route = format!("{api_address}/chain-tip");
    let expected_item = ChainTip {
        slot: 110764304,
        block_hash: "58df3617b77c9b8da958c118c3daf9cabae86e31aca761fe9bb8d57b40fe14be".into(),
        height: 9661308,
    };

    // Act
    let tip: TimestampedResponse<ChainTip> = common::get_route(&api_route).await;

    // Assert
    assert_eq!(tip.data, expected_item);
}

// decode_address (not really polyphony)

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn decode_address_route_works() {
    // Arrange
    let TestApp { api_address } = common::spawn_app().await;
    let api_route = format!("{api_address}/addresses/addr1zxgx3far7qygq0k6epa0zcvcvrevmn0ypsnfsue94nsn3tvpw288a4x0xf8pxgcntelxmyclq83s0ykeehchz2wtspks905plm/decode",);
    let expected = AddressInfo {
        bech32: Some("addr1zxgx3far7qygq0k6epa0zcvcvrevmn0ypsnfsue94nsn3tvpw288a4x0xf8pxgcntelxmyclq83s0ykeehchz2wtspks905plm".into()),
        hex: "119068a7a3f008803edac87af1619860f2cdcde40c26987325ace138ad81728e7ed4cf324e1323135e7e6d931f01e30792d9cdf17129cb806d".into(),
        network: Some(NetworkId::Mainnet),
        payment_cred: Some(PaymentCredential {
            kind: PaymentCredKind::Script,
            bech32: "addr_shared_vkh1jp520glspzqrakkg0tckrxrq7txumeqvy6v8xfdvuyu26qqv5qn".into(),
            hex: "9068a7a3f008803edac87af1619860f2cdcde40c26987325ace138ad".into(),
        }),
        staking_cred: Some(StakingCredential {
            kind: StakingCredKind::Key,
            bech32: Some("stake_vkh1s9egulk5eueyuyerzd08umvnruq7xpujm8xlzuffewqx6s0mg9v".into()),
            reward_address: Some("stake1uxqh9rn76n8nynsnyvf4ulndjv0srcc8jtvumut3989cqmgjt49h6".into()),
            hex: Some("81728e7ed4cf324e1323135e7e6d931f01e30792d9cdf17129cb806d".into()),
            pointer: None,
        })
    };

    // Act
    let actual: AddressInfo = common::get_route(&api_route).await;

    // Assert
    assert_eq!(actual, expected)
}

// lookup_datum

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn datum_by_hash_route_works() {
    // Arrange
    let TestApp { api_address } = common::spawn_app().await;
    let api_route = format!("{api_address}/datums/{}", TEST_DATUM_HASH);
    let expected = Datum {
        bytes: "d8799fd8799fd8799f581cfc6e1b47816bc4a4165a39f3c6dd65868e6b74a743dc2dcf54eb2d4cffd8799fd8799fd8799f581cd756fb7ceee90ed5af3f193d73f9970ca640ab540eedac272cb28c29ffffffffd8799fd8799f581cfc6e1b47816bc4a4165a39f3c6dd65868e6b74a743dc2dcf54eb2d4cffd8799fd8799fd8799f581cd756fb7ceee90ed5af3f193d73f9970ca640ab540eedac272cb28c29ffffffffd87a80d87a9fd8799f581c25f0fc240e91bd95dcdaebd2ba7713fc5168ac77234a3d79449fc20c47534f4349455459ff1a0bebc200ff1a001e84801a001e8480ff".into(),
        json: serde_json::from_str(r#"{"fields":[{"fields":[{"fields":[{"bytes":"fc6e1b47816bc4a4165a39f3c6dd65868e6b74a743dc2dcf54eb2d4c"}],"constructor":0},{"fields":[{"fields":[{"fields":[{"bytes":"d756fb7ceee90ed5af3f193d73f9970ca640ab540eedac272cb28c29"}],"constructor":0}],"constructor":0}],"constructor":0}],"constructor":0},{"fields":[{"fields":[{"bytes":"fc6e1b47816bc4a4165a39f3c6dd65868e6b74a743dc2dcf54eb2d4c"}],"constructor":0},{"fields":[{"fields":[{"fields":[{"bytes":"d756fb7ceee90ed5af3f193d73f9970ca640ab540eedac272cb28c29"}],"constructor":0}],"constructor":0}],"constructor":0}],"constructor":0},{"fields":[],"constructor":1},{"fields":[{"fields":[{"bytes":"25f0fc240e91bd95dcdaebd2ba7713fc5168ac77234a3d79449fc20c"},{"bytes":"534f4349455459"}],"constructor":0},{"int":200000000}],"constructor":1},{"int":2000000},{"int":2000000}],"constructor":0}"#).unwrap(),
    };

    // Act
    let datum: TimestampedResponse<Datum> = common::get_route(&api_route).await;

    // Assert
    assert_eq!(datum.data, expected);
}

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn datums_by_hashes_route_works() {
    // Arrange
    let TestApp { api_address } = common::spawn_app().await;
    let api_route = format!("{api_address}/datums");
    let request_body = vec![
        TEST_DATUM_HASH,
        "5f7f360208c46d739296ea3ff4f41d85240d94aa3f59e5b1c82c2bbeceb57f23",
    ];

    // Act
    let datums: TimestampedResponse<HashMap<String, Datum>> =
        common::post_json_route(&api_route, request_body).await;

    // Assert
    assert!(datums.data.contains_key(TEST_DATUM_HASH));
    assert!(datums
        .data
        .contains_key("5f7f360208c46d739296ea3ff4f41d85240d94aa3f59e5b1c82c2bbeceb57f23"));
}

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn lookup_datum_hash_inline_route_works() {
    // Arrange
    let TestApp { api_address } = common::spawn_app().await;
    let api_route = format!(
        "{api_address}/datums/{}",
        "5f7f360208c46d739296ea3ff4f41d85240d94aa3f59e5b1c82c2bbeceb57f23" // hash of inline datum
    );
    let expected_item = Datum { json: serde_json::from_str(r#"{"fields":[{"bytes":"e1d915c10c840017bd39088a82507b27150a438e8907784221491309"},{"bytes":"e1d915c10c840017bd39088a82507b27150a438e8907784221491309"},{"map":[{"k":{"bytes":""},"v":{"map":[{"k":{"bytes":""},"v":{"int":1}}]}}]},{"map":[{"k":{"bytes":""},"v":{"map":[{"k":{"bytes":""},"v":{"int":10000000}}]}}]},{"int":2592000000},{"int":1665433520545}],"constructor":0}"#).unwrap(), bytes: "d8799f581ce1d915c10c840017bd39088a82507b27150a438e8907784221491309581ce1d915c10c840017bd39088a82507b27150a438e8907784221491309a140a14001a140a1401a009896801a9a7ec8001b00000183c392c9a1ff".into() };

    // Act
    let datum: TimestampedResponse<Datum> = common::get_route(&api_route).await;

    // Assert
    assert_eq!(datum.data, expected_item);
}

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn lookup_datum_hash_no_data() {
    // Arrange
    let TestApp { api_address } = common::spawn_app().await;
    let api_route = format!("{api_address}/datums/{}", TEST_DUMMY_HEX_32_BYTES);

    // Act
    let response = common::get_route_any_status(&api_route).await;

    // Assert
    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "Expected 404 for invalid request, got: {} {:?}",
        response.status(),
        response.text().await
    )
}

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn script_by_hash_native() {
    // Arrange
    let TestApp { api_address } = spawn_app().await;
    let api_route = format!("{api_address}/scripts/{TEST_SCRIPT_HASH_NATIVE}");
    let expected = ScriptFirstSeen {
        hash: TEST_SCRIPT_HASH_NATIVE.into(),
        script_type: ScriptType::Native,
        bytes: "8200581c4da965a049dfd15ed1ee19fba6e2974a0b79fc416dd1796a1f97f5e1".into(),
        json: Some(
            json!({"keyHash":"4da965a049dfd15ed1ee19fba6e2974a0b79fc416dd1796a1f97f5e1","type":"sig"}),
        ),
        first_seen: TimestampedTransaction {
            tx_hash: "257916e7ae112cf16f27218e41bfa37c018bab922354201b3b38c9a24c35ab33".into(),
            slot: 55718892,
            timestamp: "2022-03-14 19:13:03".into(),
        },
    };

    // Act
    let script: TimestampedResponse<ScriptFirstSeen> = get_route(&api_route).await;

    // Assert
    assert_eq!(script.data, expected);
}

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn script_by_hash_pv1() {
    // Arrange
    let TestApp { api_address } = spawn_app().await;
    let api_route = format!("{api_address}/scripts/{TEST_SCRIPT_HASH_PV1}");
    let expected = ScriptFirstSeen {
        hash: TEST_SCRIPT_HASH_PV1.into(),
        script_type: ScriptType::PlutusV1,
        bytes: "59014f59014c01000032323232323232322223232325333009300e30070021323233533300b3370e9000180480109118011bae30100031225001232533300d3300e22533301300114a02a66601e66ebcc04800400c5288980118070009bac3010300c300c300c300c300c300c300c007149858dd48008b18060009baa300c300b3754601860166ea80184ccccc0288894ccc04000440084c8c94ccc038cd4ccc038c04cc030008488c008dd718098018912800919b8f0014891ce1317b152faac13426e6a83e06ff88a4d62cce3c1634ab0a5ec133090014a0266008444a00226600a446004602600a601a00626600a008601a006601e0026ea8c03cc038dd5180798071baa300f300b300e3754601e00244a0026eb0c03000c92616300a001375400660106ea8c024c020dd5000aab9d5744ae688c8c0088cc0080080048c0088cc00800800555cf2ba15573e6e1d200201".into(),
        json: None,
        first_seen: TimestampedTransaction { tx_hash: "d45be194817fba4a373d9d8ffa8e60596fa038d40df3b05560a0f357d00f92aa".into(), slot: 73867252, timestamp: "2022-10-10 20:25:43".into() }
    };

    // Act
    let script: TimestampedResponse<ScriptFirstSeen> = get_route(&api_route).await;

    // Assert
    assert_eq!(script.data, expected);
}

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn script_by_hash_no_data() {
    // Arrange
    let TestApp { api_address } = spawn_app().await;
    let api_route = format!("{api_address}/scripts/{TEST_DUMMY_HEX_28_BYTES}");

    // Act
    let response = get_route_any_status(&api_route).await;

    // Assert
    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "Expected 404 for invalid request, got: {} {:?}",
        response.status(),
        response.text().await
    )
}

// "e33433bdc122bd4032e2d4d2371d75658f81804c50e56c4edf2da01baaccccc7" // additional signer, contracts, metadata

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn tx_info_route_works() {
    // Arrange
    let TestApp { api_address } = spawn_app().await;

    let api_route = format!("{api_address}/transactions/e33433bdc122bd4032e2d4d2371d75658f81804c50e56c4edf2da01baaccccc7");

    let expected: TimestampedResponse<TransactionInfo> = serde_json::from_value(json!({"data":{"tx_hash":"e33433bdc122bd4032e2d4d2371d75658f81804c50e56c4edf2da01baaccccc7","block_hash":"58df3617b77c9b8da958c118c3daf9cabae86e31aca761fe9bb8d57b40fe14be","block_tx_index":26,"block_height":9661308,"block_timestamp":1702330595,"block_absolute_slot":110764304,"block_epoch":453,"inputs":[{"tx_hash":"498965c4ca9e705e0e4fa90c7b723b6bf5bcdf4362e4843c8b9bd54eaa73c9ad","index":0,"assets":[{"unit":"lovelace","amount":344000000}],"address":"addr1zxn9efv2f6w82hagxqtn62ju4m293tqvw0uhmdl64ch8uw6j2c79gy9l76sdg0xwhd7r0c0kna0tycz4y5s6mlenh8pq6s3z70","datum":{"type":"hash","hash":"352956040ebdafc51cb80aed1dcbbbceff03dcfde2eb56cc29511856b5bb476a","bytes":"d8799fd8799fd8799f581c3e7016902520a84e8911db26a045ad31224da1a631929f5fc149724dffd8799fd8799fd8799f581c09d9128f44e94b849a09d90eeaec4b26c60c4f11a40a94ca2267e353ffffffffd8799fd8799f581c3e7016902520a84e8911db26a045ad31224da1a631929f5fc149724dffd8799fd8799fd8799f581c09d9128f44e94b849a09d90eeaec4b26c60c4f11a40a94ca2267e353ffffffffd87a80d8799fd8799f581c5d16cc1a177b5d9ba9cfa9793b07e60f1fb70fea1f8aef064415d11443494147ff1a4183768bff1a001e84801a001e8480ff","json":{"constructor":0,"fields":[{"constructor":0,"fields":[{"constructor":0,"fields":[{"bytes":"3e7016902520a84e8911db26a045ad31224da1a631929f5fc149724d"}]},{"constructor":0,"fields":[{"constructor":0,"fields":[{"constructor":0,"fields":[{"bytes":"09d9128f44e94b849a09d90eeaec4b26c60c4f11a40a94ca2267e353"}]}]}]}]},{"constructor":0,"fields":[{"constructor":0,"fields":[{"bytes":"3e7016902520a84e8911db26a045ad31224da1a631929f5fc149724d"}]},{"constructor":0,"fields":[{"constructor":0,"fields":[{"constructor":0,"fields":[{"bytes":"09d9128f44e94b849a09d90eeaec4b26c60c4f11a40a94ca2267e353"}]}]}]}]},{"constructor":1,"fields":[]},{"constructor":0,"fields":[{"constructor":0,"fields":[{"bytes":"5d16cc1a177b5d9ba9cfa9793b07e60f1fb70fea1f8aef064415d114"},{"bytes":"494147"}]},{"int":1099134603}]},{"int":2000000},{"int":2000000}]}},"reference_script":null},{"tx_hash":"c22e28eb033ac63a549e65e0407d374c14bf5805b37f7c8a7b1a0770fe00c656","index":0,"assets":[{"unit":"lovelace","amount":1416027292509i64},{"unit":"0be55d262b29f564998ff81efe21bdc0022621c12f15af08d0f2ddb1bdfd144032f09ad980b8d205fef0737c2232b4e90a5d34cc814d0ef687052400","amount":1},{"unit":"13aa2accf2e1561723aa26871e071fdf32c867cff7e7d50ad470d62f4d494e53574150","amount":1},{"unit":"5d16cc1a177b5d9ba9cfa9793b07e60f1fb70fea1f8aef064415d114494147","amount":4615496690137i64},{"unit":"e4214b7cce62ac6fbba385d164df48e157eae5863521b4b67ca71d86bdfd144032f09ad980b8d205fef0737c2232b4e90a5d34cc814d0ef687052400","amount":1365147}],"address":"addr1z8snz7c4974vzdpxu65ruphl3zjdvtxw8strf2c2tmqnxz2j2c79gy9l76sdg0xwhd7r0c0kna0tycz4y5s6mlenh8pq0xmsha","datum":{"type":"hash","hash":"d97ccf3eba5574c513e902ca376bd087c03311c425d481a4ffc38b5c27b8cb4c","bytes":"d8799fd8799f4040ffd8799f581c5d16cc1a177b5d9ba9cfa9793b07e60f1fb70fea1f8aef064415d11443494147ff1b0000018bc1e7de051b0000025339590c7ad8799fd8799fd8799fd8799f581caafb1196434cb837fd6f21323ca37b302dff6387e8a84b3fa28faf56ffd8799fd8799fd8799f581c52563c5410bff6a0d43ccebb7c37e1f69f5eb260552521adff33b9c2ffffffffd87a80ffffff","json":{"constructor":0,"fields":[{"constructor":0,"fields":[{"bytes":""},{"bytes":""}]},{"constructor":0,"fields":[{"bytes":"5d16cc1a177b5d9ba9cfa9793b07e60f1fb70fea1f8aef064415d114"},{"bytes":"494147"}]},{"int":1699765280261i64},{"int":2556467678330i64},{"constructor":0,"fields":[{"constructor":0,"fields":[{"constructor":0,"fields":[{"constructor":0,"fields":[{"bytes":"aafb1196434cb837fd6f21323ca37b302dff6387e8a84b3fa28faf56"}]},{"constructor":0,"fields":[{"constructor":0,"fields":[{"constructor":0,"fields":[{"bytes":"52563c5410bff6a0d43ccebb7c37e1f69f5eb260552521adff33b9c2"}]}]}]}]},{"constructor":1,"fields":[]}]}]}]}},"reference_script":null},{"tx_hash":"c8fcd44cfa28d6cd9b58ff1cd8c5ce1dc4872ec2655fa723c58ef683610bdc4b","index":2,"assets":[{"unit":"lovelace","amount":1455465782},{"unit":"2f2e0404310c106e2a260e8eb5a7e43f00cff42c667489d30e17981631373032363237323030303030","amount":1}],"address":"addr1qx7tzh4qen0p50ntefz8yujwgqt7zulef6t6vrf7dq4xa82j2c79gy9l76sdg0xwhd7r0c0kna0tycz4y5s6mlenh8pqrkj6fh","datum":null,"reference_script":null}],"outputs":[{"tx_hash":"e33433bdc122bd4032e2d4d2371d75658f81804c50e56c4edf2da01baaccccc7","index":0,"assets":[{"unit":"lovelace","amount":1416367292509i64},{"unit":"0be55d262b29f564998ff81efe21bdc0022621c12f15af08d0f2ddb1bdfd144032f09ad980b8d205fef0737c2232b4e90a5d34cc814d0ef687052400","amount":1},{"unit":"13aa2accf2e1561723aa26871e071fdf32c867cff7e7d50ad470d62f4d494e53574150","amount":1},{"unit":"5d16cc1a177b5d9ba9cfa9793b07e60f1fb70fea1f8aef064415d114494147","amount":4614392059860i64},{"unit":"e4214b7cce62ac6fbba385d164df48e157eae5863521b4b67ca71d86bdfd144032f09ad980b8d205fef0737c2232b4e90a5d34cc814d0ef687052400","amount":1365147}],"address":"addr1z8snz7c4974vzdpxu65ruphl3zjdvtxw8strf2c2tmqnxz2j2c79gy9l76sdg0xwhd7r0c0kna0tycz4y5s6mlenh8pq0xmsha","datum":{"type":"hash","hash":"d97ccf3eba5574c513e902ca376bd087c03311c425d481a4ffc38b5c27b8cb4c","bytes":"d8799fd8799f4040ffd8799f581c5d16cc1a177b5d9ba9cfa9793b07e60f1fb70fea1f8aef064415d11443494147ff1b0000018bc1e7de051b0000025339590c7ad8799fd8799fd8799fd8799f581caafb1196434cb837fd6f21323ca37b302dff6387e8a84b3fa28faf56ffd8799fd8799fd8799f581c52563c5410bff6a0d43ccebb7c37e1f69f5eb260552521adff33b9c2ffffffffd87a80ffffff","json":{"constructor":0,"fields":[{"constructor":0,"fields":[{"bytes":""},{"bytes":""}]},{"constructor":0,"fields":[{"bytes":"5d16cc1a177b5d9ba9cfa9793b07e60f1fb70fea1f8aef064415d114"},{"bytes":"494147"}]},{"int":1699765280261i64},{"int":2556467678330i64},{"constructor":0,"fields":[{"constructor":0,"fields":[{"constructor":0,"fields":[{"constructor":0,"fields":[{"bytes":"aafb1196434cb837fd6f21323ca37b302dff6387e8a84b3fa28faf56"}]},{"constructor":0,"fields":[{"constructor":0,"fields":[{"constructor":0,"fields":[{"bytes":"52563c5410bff6a0d43ccebb7c37e1f69f5eb260552521adff33b9c2"}]}]}]}]},{"constructor":1,"fields":[]}]}]}]}},"reference_script":null},{"tx_hash":"e33433bdc122bd4032e2d4d2371d75658f81804c50e56c4edf2da01baaccccc7","index":1,"assets":[{"unit":"lovelace","amount":2000000},{"unit":"5d16cc1a177b5d9ba9cfa9793b07e60f1fb70fea1f8aef064415d114494147","amount":1104630277}],"address":"addr1qyl8q95sy5s2sn5fz8djdgz945cjyndp5cce986lc9yhyngfmyfg738ffwzf5zwepm4wcjexccxy7ydyp22v5gn8udfste7yl4","datum":null,"reference_script":null},{"tx_hash":"e33433bdc122bd4032e2d4d2371d75658f81804c50e56c4edf2da01baaccccc7","index":2,"assets":[{"unit":"lovelace","amount":1456673898},{"unit":"2f2e0404310c106e2a260e8eb5a7e43f00cff42c667489d30e17981631373032363237323030303030","amount":1}],"address":"addr1qx7tzh4qen0p50ntefz8yujwgqt7zulef6t6vrf7dq4xa82j2c79gy9l76sdg0xwhd7r0c0kna0tycz4y5s6mlenh8pqrkj6fh","datum":null,"reference_script":null}],"reference_inputs":[],"collateral_inputs":[{"tx_hash":"c8fcd44cfa28d6cd9b58ff1cd8c5ce1dc4872ec2655fa723c58ef683610bdc4b","index":2,"assets":[{"unit":"lovelace","amount":1455465782},{"unit":"2f2e0404310c106e2a260e8eb5a7e43f00cff42c667489d30e17981631373032363237323030303030","amount":1}],"address":"addr1qx7tzh4qen0p50ntefz8yujwgqt7zulef6t6vrf7dq4xa82j2c79gy9l76sdg0xwhd7r0c0kna0tycz4y5s6mlenh8pqrkj6fh","datum":null,"reference_script":null}],"collateral_return":{"tx_hash":"e33433bdc122bd4032e2d4d2371d75658f81804c50e56c4edf2da01baaccccc7","index":3,"assets":[{"unit":"lovelace","amount":1450465782},{"unit":"2f2e0404310c106e2a260e8eb5a7e43f00cff42c667489d30e17981631373032363237323030303030","amount":1}],"address":"addr1qx7tzh4qen0p50ntefz8yujwgqt7zulef6t6vrf7dq4xa82j2c79gy9l76sdg0xwhd7r0c0kna0tycz4y5s6mlenh8pqrkj6fh","datum":null,"reference_script":null},"mint":[],"invalid_before":110764281,"invalid_hereafter":110765281,"fee":791884,"deposit":0,"certificates":{"stake_registrations":[],"stake_deregistrations":[],"stake_delegations":[],"pool_registrations":[],"pool_retirements":[],"reg_certs":[],"unreg_certs":[],"vote_delegations":[],"stake_vote_delegations":[],"stake_reg_delegations":[],"vote_reg_delegations":[],"stake_vote_reg_delegations":[],"auth_committee_hot_certs":[],"resign_committee_cold_certs":[],"reg_drep_certs":[],"unreg_drep_certs":[],"update_drep_certs":[],"mir_transfers":[]},"withdrawals":[],"additional_signers":["bcb15ea0ccde1a3e6bca4472724e4017e173f94e97a60d3e682a6e9d"],"scripts_executed":[{"hash":"a65ca58a4e9c755fa830173d2a5caed458ac0c73f97db7faae2e7e3b","type":"plutusv1","bytes":"59014c01000032323232323232322223232325333009300e30070021323233533300b3370e9000180480109118011bae30100031225001232533300d3300e22533301300114a02a66601e66ebcc04800400c5288980118070009bac3010300c300c300c300c300c300c300c007149858dd48008b18060009baa300c300b3754601860166ea80184ccccc0288894ccc04000440084c8c94ccc038cd4ccc038c04cc030008488c008dd718098018912800919b8f0014891ce1317b152faac13426e6a83e06ff88a4d62cce3c1634ab0a5ec133090014a0266008444a00226600a446004602600a601a00626600a008601a006601e0026ea8c03cc038dd5180798071baa300f300b300e3754601e00244a0026eb0c03000c92616300a001375400660106ea8c024c020dd5000aab9d5744ae688c8c0088cc0080080048c0088cc00800800555cf2ba15573e6e1d200201","json":null},{"hash":"e1317b152faac13426e6a83e06ff88a4d62cce3c1634ab0a5ec13309","type":"plutusv1","bytes":"591e1801000032323232323232323232323232323232323232323232323232323232323232323232323232323232323232323232323232323232323232323232323232323232323232323232323232323232323232323232323232323232323232323232323232323232222323232533533355333573460cc0042646424446600200a0086eb4d5d09aba25002375a6ae85400454ccd5cd1832801099091118010021bad357426aae7800c54ccd5cd1832001099190911198018028021bad357426ae894008c0ccd5d0a80082e1119191a827911111a80391191111aa99a9824806108008b1119191a9a9a80103102d91191919191919191aa99a982a89119982d91299a99820a8071a80103409980200100088008008020b03c9111919191919191919191919191982a299a8050a99aa8100999ab9a3094013303a307908101330820106700b06706e0060063305433355307908101305c05a305953353502b2233500206e2071210011635014222222207a33054353501422208e0122350012322533355333500a2153335004215333500c2130054984c011261533350052130054984c0112603c04f15333500b2130044984c00d261533350042130044984c00d2603b153335003205003a04f153335003215333500b2130044984c00d261533350042130044984c00d2603b04e15333500a2130034984c009261533350032130034984c0092603a153350010700820108201070253335002215333500a21533350042133303a03b00200116161604e15333500921533350032133303903a00200116161604d04e33054330500153306f00b02733054330533306301602548008cc150cc14ccc18c02c0952002330543305333063001010002330543333084012222533500315335002135001222223305d3305c0053306c01402b3305d3305c0043306c01402a3305d3305c00300c3305d3305c0023306c0140193305c001304800d0910122153350041622153350071622153350081622133300c003001323232323232323533307d0050070272222225335330743305100248000cc1440052000161333335003235500b2222223501d222223501b22235051222223232323232323232323253353308b013308b013306e00848000cc1b802920003308b013306d00201e3308b0153353306e00a00315335330870100e07013308b013308a013309a010110703370066e0402800c060cc22804cc2680404411c0084cc22c04cc22804cc268040441c0060cc22c04cc22804cc2680404411c008cc22804cdc08050019984d008088070a99a998370041a802055008a99a99843808078380998458099845009984d0080883819b80337020106a008154020306611402661340202208e00426611602661140266134020220e003066116026611402661340202208e004661140266e04020d40102a804cc2680404403c4cc22c04cc22804cc2680404411c008cc22804cc268040441c0060cc22c04cc20c04048070cc2080404006c4c8c8c8ccccc2c80400c008cdc019b800180050013370002e002a66a0082605466e0800c00840594cd400c4cccc0a005406406005c520003370002e00866e0005cd40102a80458c27c04028d40082a404d40042a404d54cd54ccd5cd19b8900148000278044c94ccd5cd19b890014800027c044c94ccd5cd19b8900148000280044c28404ccc2ec0400c0080054ccd5cd19b8900400610041006350020ad0121001160b901350010b0015333573466e2400400c54ccd5cd19b880010031330a8010023370666e080080400444cc2a0040080104cc2a004cdc199b820040110100043370666e080040380414cd4cc1fc01c1a04cdc0998490081100399b80011010133092010220073370666e080040300354cd4cc1f40101984cdc0998480081000219b8000f00e133090010200042235500c2222223501e222223501c222350522222232323232323232323253353308b013306e00748000cc22c04cc1b4008074cc22c04cc22c04cc22804cc2680404011c008cc22804cc268040401c005ccc22c04cc20c0404406ccc2080403c0684c8c8c8c8ccccc2cc0400c008cdc019b800180060013370002e002a66a00a2605666e0800c00840594cd40104cccc0a405406406005c52000350020b301350010b60153353330690870100e01e1330ae013370002c00e02a26615c0202c66e0005401c58c27c04024cdc199b820010123370200400266604400600266e0ccdc0981219b803370400400466e08cdc119b82337049004241941e90680780200198600080199b824801120ca0f350050a70130be01001350030a60153353308001001069133702661260204600266e000440404cc24c0408c004d400428804d54cd4ccc1801f80140544ccc2d0040140340304ccc2d004010030034888d400c88ccc2e40401401000c88d54030888888d407888888d4070888d414888888c8c8c8c94cd4cc21804cc1a4009200033086013306800101833086015335330820101906b13308501330950100b0193370000202426610c026610a026612a020160320026610a026612a020160d60246610c02660fc01802c660fa01402a2a66a6660c8104020120322666661540266e00044008cdc08080008078070068999998550099b810110013370002000401e01c01a2c66603e6a00614c026a00614a02002a66a66100020020d2266e04cc24c0408c004cdc0008808099849808118009a800851009aa99a99983003f00280a899985a00802806806099985a008020060069111a8019119985c80802802001911aa8061111111a80f111111a80e1111a82911111191919191919299a998440099835802240006611002660d80020086611002a66a66108020360da266110026610e026612e0201a03666e00068050cc21c04cc25c04034014cdc08020008a99a99842008028368998440099843809984b8080680d80d19843809984b8080680299b8033702008002028266110026610e026612e0201a03603466110026610e026612e0201a00a66e04010004cc21c04cc25c040341b4050cc22004cc20004038060cc1fc03005c54cd4ccc1982100402c06c4ccccc2b004cdc000980099b8101201a01101000f1333330ac013370202603466e0004800404404003c594cd54ccd5cd19b880190011309f013370066e0ccdc119b82002019483403ccdc119b81001019483283d200209e012100116350040a601350030a60153353308001001069133702661260204600266e000440404cc24c0408c004d400428804d54cd4ccc1801f80140544ccc2d0040140340304ccc2d004010030034888d400c88ccc2e40401401000c88d54030888888d407888888d4070888d414888888c8c8c8c94cd4cc21804cc1a4011200033086013308601330680030193306800201833086015335330820100906b133086013308501330950100b009337000060246610a026612a020160100042a66a66104020100d626610c026610a026612a0201601066e00008048cc21404cc2540402c02400c4cc21804cc21404cc2540402c02400ccc21804cc21404cc2540402c020008cc21404cc2540402c1ac048cc21804cc1f8030058cc1f40280544c8c8c8ccccc2b40400800ccdc019b8101200700133700022002a66a0082604a66e0800800c40414cd400c4cccc08c03c04c048044520003370202400866e0404000858c26804010cdc199b8200200e00d3370666e08004038030cc244040840f8888c8cdc199b820010033370066e0801120d00f0013370400290650791112999ab9a3371200890000a4000264a666ae68cdc48008028a4000264a666ae68cdc4800a400029000080099b833370400466e04004014cdc019b8200148028014c014cdc10018011192999ab9a33710004900004d808a999ab9a30a10100214800054ccd5cd1851008010a40042a666ae68c28c04008520021330010023370066e0c009200448008c254048894ccd5cd19b880010021330030013370666e00cdc1802000800a4008200426660f2002006046464a666ae68c27c04d55ce80089919191919191919191919191919091999998008050048040028018011bad357426ae88008dd69aba100135744010a666ae68c2b4040084c8c8488888cc01001c018dd69aba135744a0046660f8eb9d71aba15001153335734615802004264642444446600200e00c6eb4d5d09aba25002375a6ae85400454ccd5cd18558080109909111118028031bad357426aae7801854ccd5cd18550080109919091111198010038031bad357426ae894008ccc1f1d73ae35742a0022a666ae68c2a4040084c8c8488888cc00c01c018dd69aba135744a0046660f8eb9d71aba150010a101135573c00a6aae74010cc1e1d71aba100530743574200a60e66ae84014dd51aba1001357440026ae88004d5d10009aab9e0010970137540026a002104026a00810c0260c82446660d444a66a660b066606c0d46a004104026a03c1040266606c0a06a6a0040fc0ee05e266008004002200200202a60c82446660d444a66a660b066606c0a06a0040ee6a0200ee66606c0a06a0040ee05e2660080040022002002026666660f0660c602c044660c602c04203e660c602c02003c660a8660a0044607a008660a8660a0042607c008660a8024660a8a66a60c82446660d444a66a660a66aa03a104026a6a0040ee1040226600800400220020020260d6442a66a0020fe440dea66a6660640a600490000998299981d183c840809984100833800a40042660a66607460f21020266104020ce0029000183800999b8100101d303d001333066059008010305333307f22322325333573466e1c010dc680488010a999ab9a308f0100415333573466e1d205a500313370290001980299b800044800800800400454ccd5cd19b885002481805854ccd5cd19b8950023370090302402426600866e0000d20023370066e08005201433702a00490300b099b8e0060014800120001533500400115335501a13335734611a026606860e60f6660f80c200a0c20d00022a66a0062a66aa0320022666ae68c23804cc0ccc1c81e8cc1ec18001018019c0044ccd5cd18460099819183883c9983d02f80182f833299a9983d91299a8008321109a80111299a9982b00101089834800898030019a9a99a983b03c00501083883610a99a8008b1109a80111299a8018a99a99827800a400420042c112022c666ae68cdc49982c800803240000c80ba6a0020d4a66a60b02446660bc44a66a66088a0226a0040d6266008004002200200200e2c0f8660ce02c6a00a0d460c2006a66a60a42446660b044a66a660826aa0160e06a6a6a0040d80ca0e026600800400220020020060b2442a66a0020da440ba60ba0026a0280d4660b40020246a6a0080c80be26a6a0020c20b4a66a609601c420022c2a66a660600040320ba266060002032603200e6068010464646a09c4444464646464646464660706606a00c0066607066068660a600c018660a6006018660706606e6608e6a6a66a60c60ca00401e0bc0b2660b601201090011981c1981f998188070009981c1981b9811800a40006607066068604201c60420026607066068604401c60440026606e604801c6048002660706606a60aa00a09666070a66a609024466609c44a66a60a26a6a0040c40b8266008004002200200200809e442a66a0020c6440a6a66a609024466609c44a66a60a26a0040b8266008004002200200260b000e09e442a66a0020c6440a666609a08000600860a40066a0020aca66a608824466609444a66a660666a6a6a00e0bc0ae0c46a6a0040ae0c4266008004002200200260a80062c0d06a0100ba6a6a0020b00a6a66a608400c420022c603000c606600e4464646a09e44444a66a6a00e440a8426464646464646464646464646607e660700286660aa09000800c6607e66076014660b40060246607e6607e6607c6609c6a0040c00120106607e6606e6a0040bc6a01a0d26606c6a0040be6a01a0ca6607e6607866aa60ce0d846a00244660ca00466aa60d40de46a00244660d0004666a0026e012000700466e0000520000013304000b3500922330723306400233072330640010090540540033303f3303e3304e3535335306a06c0010160650603306200f00e48008cc0fccc0f0c1700181494cd4c13c488ccc154894cd4c160d4d40081a418c4cc010008004400400400c1588854cd40041a888168c168014cd4c1a01a800c04cd40041754cd4c12c488ccc144894cd4cc0dcd4d4030194178d40081784cc010008004400400400c581bcc160004d403418cccd4080178d4d4080188178004cc11800c004cc164020d4004170cc140004020d4d40041681554cd4c11001c8400458124c06401cc0d0020448004584d55cf0011aab9d0013754004444a66a6600600400207809c46a0020b0246666666600204044a666ae68cdc38010008020a999ab9a3371200400203203044666ae68cdc400100081b01e802802001912999ab9a337120040022002200444a666ae68cdc4801000880108008881f11199ab9a3371000400207406644666ae68cdc480100081c81911199ab9a337120040020620706607c91100488100223333550023303f2233350050480010023500304222337000029001000a4000660784446006600400240026607666076e01200070246a0024444400a46a0020a246a0024407246a0024406c464a666ae68c140d55ce8008991919191981e2999ab9a305435573a00626464646464646464646464646464646464646464646424666666666600201a01801601401201000e00a00600460446ae84d5d10011980f1981dbae2001357420026ae88008cc071d71aba100135744016a666ae68c190d55ce804899191919827a999ab9a306735573a004264660a066038eb4d5d0800980d9aba1357440026aae7800817d4ccd5cd18339aab9d001132330503301c75a6ae84004c06cd5d09aba200135573c0020be6ea8d5d09aba200237546ae84004d55cf00482e1980c9981b019bad35742014660300326ae84028ccc059d70029aba100a33301575c0086ae84028cc054008d5d08051980a1192999ab9a306035573a0022646609260326ae84004c010d5d09aba200135573c0020b06ea8004d5d08051192999ab9a305f35573a002264646660b060606ae84008ccc059d70029aba10013303375c6ae84d5d10009aba200135573c0020ae6ea8004cc045d73ad37546ae84004d5d10009aba2001357440026ae88004d5d10009aba200135573c006098a666ae68c15c0044c848888c010014c02cd5d09aab9e00215333573460ac00226424444600400a60486ae84d55cf0010a999ab9a3055001132122223001005300c357426aae7800854ccd5cd182a0008990911118018029bae357426aae78008130d55ce8009baa357426ae88008dd51aba100135573c0020906ea80048c94ccd5cd182800081e8a999ab9a304f00102b04735573a6ea800488c8c94ccd5cd18290008058a999ab9a3051001130193004357426aae7800854ccd5cd18280008050241aab9d00137540024464460046eac004c10888cccd55cf8009014119198239981c98031aab9d001300535573c00260086ae8800cd5d0801020919118011bac00130402233335573e002404c46608860086ae84008c00cd5d100101f91919192999ab9a305300211222203515333573460a4004220922a666ae68c1440084c8c848888888cc004024020dd69aba135744a0046eb8d5d0a8008a999ab9a3050002132321222222233002009008375c6ae84d5d128011bae35742a0022a666ae68c13c0084c8c848888888cc018024020dd71aba135744a004603a6ae85400454ccd5cd1827001099091111111803804180e9aba135573c0062a666ae68c1340084c848888888c014020c074d5d09aab9e003045135573c0046aae74004dd50009192999ab9a304a35573a0022646606660086ae84004dd69aba1357440026aae78004108dd50009192999ab9a304935573a00226eb8d5d09aab9e0010413754002220582205444a66a00442a66a00442660240040020462a66a0024046068446a004446a006446666010008006004002446a004444446a00c44444a66a6601e01400a2a66a6601e0120082a666ae68cdc38040018a999ab9a3370e00e0042a66a00c42a66a004426a004446a004446a00a446a00444a66a666602e00c00a0040022a66a00e42a66a008426604800400206a2a66a006406a08c0680562a66a002405607805405405405444446466a00a466a0084a666ae68cdc78010008018121013919a802101392999ab9a3371e0040020060482a66a00642a66a0044266a004466a00446601200400244405444466a0084054444a666ae68cdc38030018a999ab9a3370e00a0042660220080020520520442a66a00240440664466a004466a00446601c0040024046466a004404646601c004002446a004446a00644a666ae68cdc780200109980780180081091199aa9815019180680191a80091199aa981681a980800311a80091199a800919805a40000020144660160029000000998030010009981280100a91199ab9a3370e00400202c03a44a66a00420020324466aa605205c46a002446604e004666a002466aa605a06446a0024466056004601800200244666010016004002466aa605a06446a0024466056004601600200266600600c004002444666aa605005c06466aa605205c46a002446604e0046010002666aa605005c446a00444a66a666aa6054064601a01646a002446601400400a00c200626606c00800602800266aa605205c46a002446604e0046606844a66a002260120064426a00444a66a6601800401022444660040140082600c00600800442444600200842444600600844666ae68cdc780100080800b9980e80080a11299a801012080091980e11199a8018128010009a80080f9192999ab9a303435573a00226464646466666042666016eb9d71aba100433300b75ceb8d5d08019bad357420046eb4d5d0800998051192999ab9a303a35573a0022646604660146ae84004cc035d71aba1357440026aae780040c8dd50009aba1357440026ae88004d5d10009aba200135573c0020586ea80048c94ccd5cd18199aab9d0011323301c3005357420026600c0086ae84d5d10009aab9e00102b375400246464a666ae68c0d00044c8c8c8c8c8488ccc00401801000cdd69aba1357440046eb4d5d08009aba2002375a6ae84004d55cf0010a999ab9a3033001130103004357426aae780080acd55ce8009baa0012323253335734606600226424460020066eb8d5d09aab9e00215333573460640022601e6eb8d5d09aab9e00202a35573a0026ea800488c8c94ccd5cd18190008980798021aba135573c0042a666ae68c0cc0040380a8d55ce8009baa001222325333573460626aae740044c8cc068c014d5d080098021aba1357440026aae780040a4dd5000911a8009119198131119a800a4000446a00444a666ae68cdc7801004898038008980300180298129119a800a4000446a00444a666ae68cdc7801003880089803001919a80081100211a800911a80111111111111999a805900b900b900b9199aa981101500b11a80091299a998090010020980c00180b805912999ab9a3371e6a0040346a0020342666ae68cdc39a80100b1a80080b001805003880b91180f11299a80088019109980300118020009299a800900b0019111a801111299a800909a8029111111111299aa99a999aa981001400a11a800912999ab9a3371e00401c2602c00602a0044260286a0020440244260240022c2c2006424460040066601444a66a0044200620020022018446602e44a66a00203c4426a00444a666ae68cdc78010038a99a8008111109a80111299a8018a999ab9a302d00113301400b0020262202813006003002235001222222222200a2350012201c23500122222222220092220032220012220023333300248811c0be55d262b29f564998ff81efe21bdc0022621c12f15af08d0f2ddb10048811ce4214b7cce62ac6fbba385d164df48e157eae5863521b4b67ca71d8600330014891c13aa2accf2e1561723aa26871e071fdf32c867cff7e7d50ad470d62f004881074d494e535741500048811c2f2e0404310c106e2a260e8eb5a7e43f00cff42c667489d30e179816004881054f574e455200221233001003002222221233333001006005004003002300b22112225335001135003006221333500500c300400233355300700f0050040012200130092211222533500110022213300500233355300700d005004001300822112253350010052213300f30040023355300600b00400111001220023005221225333573466e20005200013005490103505436001533500213005491035054370022153335734602c0062004266a600c01000266e0400d2002253357380022c240026004444a66a00220044426a004446600e66601000400c00200660024444a66a00220044426a00444a666ae68c0500044ccc02001c01800c4ccc02001ccc028ccc02c01c00800401800c8c8c00400488cc00cc00800800488488cc00401000c88848ccc00401000c00854cd5ce2490350543100162215335001100200715335738921001622222222007220053704904d0f910b111110021b8748000dc3a40046e1d2004370e90031b8748020dc3a40146e1d200c01","json":null}],"scripts_successful":true,"redeemers":{"spends":[{"script_hash":"a65ca58a4e9c755fa830173d2a5caed458ac0c73f97db7faae2e7e3b","input":{"tx_hash":"498965c4ca9e705e0e4fa90c7b723b6bf5bcdf4362e4843c8b9bd54eaa73c9ad","index":0},"input_index":0,"data":{"json":{"constructor":0,"fields":[]},"bytes":"d87980"},"ex_units":[42061,14890343]},{"script_hash":"e1317b152faac13426e6a83e06ff88a4d62cce3c1634ab0a5ec13309","input":{"tx_hash":"c22e28eb033ac63a549e65e0407d374c14bf5805b37f7c8a7b1a0770fe00c656","index":0},"input_index":1,"data":{"json":{"constructor":0,"fields":[{"constructor":0,"fields":[{"constructor":0,"fields":[{"bytes":"bcb15ea0ccde1a3e6bca4472724e4017e173f94e97a60d3e682a6e9d"}]},{"constructor":0,"fields":[{"constructor":0,"fields":[{"constructor":0,"fields":[{"bytes":"52563c5410bff6a0d43ccebb7c37e1f69f5eb260552521adff33b9c2"}]}]}]}]},{"int":2}]},"bytes":"d8799fd8799fd8799f581cbcb15ea0ccde1a3e6bca4472724e4017e173f94e97a60d3e682a6e9dffd8799fd8799fd8799f581c52563c5410bff6a0d43ccebb7c37e1f69f5eb260552521adff33b9c2ffffffff02ff"},"ex_units":[2639497,790336775]}],"mints":[],"withdrawals":[],"certificates":[],"votes":[],"proposals":[]},"metadata":{"674":{"msg":["Minswap: Order Executed"]}},"size":9626},"last_updated":{"timestamp":"2023-12-13 21:17:17","block_hash":"3cd6a410a2dbe474f42831e42ea47ea492d7c21e7a12a013029374792dbeece8","block_slot":110935946}})).unwrap();

    // Act
    let info: TimestampedResponse<TransactionInfo> = get_route(&api_route).await;

    // Assert
    assert_eq!(info.data, expected.data)
}

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn tx_info_no_data() {
    // Arrange
    let TestApp { api_address } = spawn_app().await;
    let api_route = format!("{api_address}/transactions/{TEST_DUMMY_HEX_32_BYTES}");

    // Act
    let response = get_route_any_status(&api_route).await;

    // Assert
    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "Expected 404 for invalid request, got: {} {:?}",
        response.status(),
        response.text().await
    )
}

#[ignore = "requires a running local stack (docker compose up); run with cargo test -- --ignored"]
#[tokio::test]
#[traced_test]
async fn balance_by_payment_cred_route_works() {
    // Arrange
    let TestApp { api_address } = spawn_app().await;

    let api_route = format!("{api_address}/addresses/cred/addr_vkh1wdkle2sprqsuklt34474g6n4ps7k6pv6zwe4644uxmg7xj54y87/balance");

    let expected: TimestampedResponse<Balance> = serde_json::from_value(json!({"data":{"lovelace":"1444443","assets":{"f0ff48bbb7bbe9d59a40f1ce90e9e9d0ff5002ec48f232b49ca0fb9a":{"6164616d616e74":"1"}}},"last_updated":{"timestamp":"2023-12-11 21:36:35","block_hash":"58df3617b77c9b8da958c118c3daf9cabae86e31aca761fe9bb8d57b40fe14be","block_slot":110764304}})).unwrap();

    // Act
    let info: TimestampedResponse<Balance> = get_route(&api_route).await;

    // Assert
    assert_eq!(info.data, expected.data)
}
