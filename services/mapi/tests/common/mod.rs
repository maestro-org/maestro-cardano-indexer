use reqwest::Response;
use serde::{de::DeserializeOwned, Serialize};
use std::net::TcpListener;

use mapi::config::Config;

// dummy hex-encoded bytes so we can pass input validation
#[allow(dead_code)]
pub const TEST_DUMMY_HEX_32_BYTES: &str =
    "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef";
#[allow(dead_code)]
pub const TEST_DUMMY_HEX_28_BYTES: &str =
    "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef";

// --- relating to block: 55718892.e2792f4e8c421f263b611ee96a72bc4b10d49e34ef98da1ab3ddba1601c7f87d
#[allow(dead_code)]
pub const TEST_ADDRESS: &str =
    "addr1g9ekml92qyvzrjmawxkh64r2w5xr6mg9ngfmxh2khsmdrcudevsft64mf887333adamant";
#[allow(dead_code)]
pub const TEST_ADDRESS_2: &str = "addr1qxdhpdhcyfdvpm34tyk5x5jtmxwvx5v4rsf5f35ahkkj28s2my8369f899gus8ey7e2g0xespc8jxf4340e0uhg4dn7shlqt83";
#[allow(dead_code)]
pub const TEST_NULL_ADDRESS: &str = "addr1vyqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqkdl5mw";
#[allow(dead_code)]
pub const TEST_KEY_PAYMENT_CRED: &str =
    "addr_vkh1wdkle2sprqsuklt34474g6n4ps7k6pv6zwe4644uxmg7xj54y87";
#[allow(dead_code)]
pub const TEST_SCRIPT_PAYMENT_CRED: &str =
    "addr_shared_vkh1ewj7sycvy5y234m3uhudn5dggqk3djr0jheacr3utna5gcnmwp2";
#[allow(dead_code)]
pub const TEST_NULL_KEY_PAYMENT_CRED: &str =
    "addr_vkh1qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqxxt579";
// ada handle policy
#[allow(dead_code)]
pub const TEST_POLICY_HASH: &str = "f0ff48bbb7bbe9d59a40f1ce90e9e9d0ff5002ec48f232b49ca0fb9a";
// token name "adamant", TEST_ADDRESS receives this handle in block
#[allow(dead_code)]
pub const TEST_ASSET_NAME: &str = "6164616d616e74";
#[allow(dead_code)]
pub const TEST_TX_HASH: &str = "257916e7ae112cf16f27218e41bfa37c018bab922354201b3b38c9a24c35ab33";
#[allow(dead_code)]
pub const TEST_TX_HASH_2: &str = "bd69d4230a754a6c1691d0de447edb819842bb9e3fdfc42e52687bf6f6943e54";

// --- relating to block: 73867237.bdf4630098ac6923e0ef1ca0b0d6e00dca96790a8c3dd670948fdfe1456798d2
#[allow(dead_code)]
pub const TEST_BLOCK_HASH: &str =
    "bdf4630098ac6923e0ef1ca0b0d6e00dca96790a8c3dd670948fdfe1456798d2";
#[allow(dead_code)]
pub const TEST_TXO_DATUM_HASH: (&str, usize) = (
    "31a84c3c6200bec2498b18c42f882fa690cd0d32a9c84a2019eb5cc42f5971d0",
    0,
);
#[allow(dead_code)]
pub const TEST_DATUM_HASH: &str =
    "f03819a4039003a8c6b65153351adbc61f983bd22787ed238a5b4f24a01aa5d6";
#[allow(dead_code)]
pub const TEST_TXO_INLINE_DATUM: (&str, usize) = (
    "1eec3e58f7001c5b875456232e79cbfdb64e2be0e2b32c06f5c07cab38069634",
    0,
);
#[allow(dead_code)]
pub const TEST_TXO_REF_SCRIPT_PV2: (&str, usize) = (
    "1eec3e58f7001c5b875456232e79cbfdb64e2be0e2b32c06f5c07cab38069634",
    0,
);

// --- relating to block: 49503576.faaeda2a014e0ad176e06837978c2f663ab0fab5f5874b079b886398099b97be
#[allow(dead_code)]
pub const TEST_PHASE_2_INVALID_TX_HASH: &str =
    "f9ed2fef27cdcf60c863ba03f27d0e38f39c5047cf73ffdf2428b48edbe83234";

// --- relating to db sync
#[allow(dead_code)]
pub const TEST_POOL_ID: &str = "pool10208t5hc4l4gll3d64d2a8mml5tjgg9k0nsjvxplt046k7pwkvc";
#[allow(dead_code)]
pub const TEST_POOL_ID2: &str = "pool1mv0kgwy8pghc0l2kr7dueeaf756a6pvwe0zp0umdpmug565r5hn";
#[allow(dead_code)]
// no metadata is registered for this pool on preprod (as of snapshot being used 7th dec)
pub const TEST_POOL_ID_NO_MD: &str = "pool174mw7e20768e8vj4fn8y6p536n8rkzswsapwtwn354dckpjqzr8";

#[allow(dead_code)]
pub const TEST_SCRIPT_HASH_NATIVE: &str =
    "f0ff48bbb7bbe9d59a40f1ce90e9e9d0ff5002ec48f232b49ca0fb9a";
#[allow(dead_code)]
pub const TEST_SCRIPT_HASH_PV1: &str = "a65ca58a4e9c755fa830173d2a5caed458ac0c73f97db7faae2e7e3b";
#[allow(dead_code)]
pub const TEST_SCRIPT_HASH_PV2: &str = "1cf569e1ec3e0fee92f1f5002bfd4213b796c151c708db46e6e2d3a4";

#[allow(dead_code)]
pub const TEST_STAKE_ADDR_1: &str =
    "stake_test1uzzrlp0dregu7k9suclev6yyz452rw7taxnr4aualf9t0ncrrug5t";
#[allow(dead_code)]
pub const TEST_STAKE_ADDR_2: &str =
    "stake_test1urx2tgde6nqgvrl7te4ard7xxvvn8exa3khfqepns93pl4q82kucs";
#[allow(dead_code)]
pub const TEST_STAKE_ADDR_3: &str =
    "stake_test1upxue2rk4tp0e3tp7l0nmfmj6ar7y9yvngzu0vn7fxs9ags2apttt";

#[allow(dead_code)]
pub const TEST_POLICY_HASH_DBSYNC: &str =
    "ffffe5caf9c66b060a3f721ee8b2a9b90cd83123158760cff7fe7e51";
#[allow(dead_code)]
pub const TEST_ASSET_NAME_DBSYNC: &str = "4e4557434f4c30";

#[allow(dead_code)]
pub struct TestApp {
    pub api_address: String,
}

#[allow(dead_code)]
pub async fn spawn_app() -> TestApp {
    let config = Config::default();

    let app = mapi::app(config)
        .await
        .expect("Error starting mapi instance.");

    let listener =
        TcpListener::bind("127.0.0.1:0").expect("Failed tcplistener to bind to random port.");

    let api_address = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let server = axum::Server::from_tcp(listener)
        .expect("Error using TCP listener in server")
        .serve(app.into_make_service());

    tokio::spawn(server);

    TestApp { api_address }
}

#[allow(dead_code)]
pub async fn get_route<T: DeserializeOwned>(api_route: &str) -> T {
    // Arrange
    let client = reqwest::Client::new();

    // Act
    let response = client
        .get(api_route)
        .send()
        .await
        .expect("\nFailed to execute get request");

    println!("{response:?}");
    // Assert
    assert!(response.status().is_success());

    response.json::<T>().await.expect("Error parsing JSON.")
}

#[allow(dead_code)]
pub async fn get_route_any_status(api_route: &str) -> Response {
    // Arrange
    let client = reqwest::Client::new();

    // Act
    let response = client
        .get(api_route)
        .send()
        .await
        .expect("\nFailed to execute get request");

    println!("{response:?}");

    response
}

#[allow(dead_code)]
pub async fn post_json_route<T: DeserializeOwned, S: Serialize>(api_route: &str, body: S) -> T {
    // Arrange
    let client = reqwest::Client::new();

    // Act
    let response = client
        .post(api_route)
        .json(&body)
        .send()
        .await
        .expect("\nFailed to execute post request");

    println!("{response:?}");
    // Assert
    assert!(response.status().is_success());

    response.json::<T>().await.expect("Error parsing JSON.")
}
