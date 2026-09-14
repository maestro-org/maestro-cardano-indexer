// Integration tests requiring a running local stack (docker compose up).
// Enable with: cargo test -p mapi --features stack-tests
#![cfg(feature = "stack-tests")]

use std::env;

use ctor::ctor;
use mapi::key_resolver::{self, ReducerType};
use serial_test::serial;
use strum::IntoEnumIterator;
use tracing_test::traced_test;

mod common;

#[ctor]
fn test_init() {
    // This code will run before each test
    env::set_var("TOKEN_REGISTRY_DATAPLANE_ID", "123");
    env::set_var("TOKEN_REGISTRY_INSTANCE_ID", "1");
    env::set_var("REDIS", "redis://localhost:6379");
}

#[tokio::test]
#[serial]
#[traced_test]
async fn test_all_cardano_reducers_have_instance() {
    // Iterate over all ReducerType variants using strum's iter method
    ReducerType::iter().for_each(|reducer_type| {
        let instance_result =
            std::panic::catch_unwind(|| key_resolver::get_instance_for_reducer(reducer_type));
        assert!(
            instance_result.is_ok(),
            "Reducer {:?} does not have a valid instance mapping",
            reducer_type
        );
    });
}

#[tokio::test]
#[serial]
#[traced_test]
async fn test_resolve_standard_cardano_reducer() {
    // Arrange: reducer of type UtxoCborByAddress
    let reducer = ReducerType::UtxoCborByAddress;
    let redis_address: String = "redis://localhost:6379".to_string();
    key_resolver::initialize_config(key_resolver::Network::Mainnet, redis_address);

    // Act: Call the resolve_key function
    let result = key_resolver::resolve_key(reducer);

    // Assert: Check that the result is Ok with the expected values
    match result {
        Ok((dataplane_id, instance_id)) => {
            assert_eq!(dataplane_id, 123, "Expected dataplane_id to be 123");
            assert_eq!(instance_id, 1, "Expected instance_id to be 1");
        }
        Err(e) => panic!("Expected Ok result, but got an error: {:?}", e),
    }
}
