use ctor::ctor;
use mapi::config::Config;
use serial_test::serial;
use std::env;
use tracing_test::traced_test;

mod common;

#[ctor]
fn test_init() {
    // This code will run before each test
    env::set_var("TOKEN_REGISTRY_DATAPLANE_ID", "123");
    env::set_var("TOKEN_REGISTRY_INSTANCE_ID", "1");
    env::set_var("REDIS", "redis://localhost:6379");
}

// The token registry env vars are optional: when unset, the config loads with
// `None` and the API serves null token-registry metadata.
#[tokio::test]
#[serial]
#[traced_test]
async fn config_test_registry_dataplane_id_missing() {
    // Setup:
    env::remove_var("TOKEN_REGISTRY_DATAPLANE_ID");
    env::set_var("TOKEN_REGISTRY_INSTANCE_ID", "1");

    // Test:
    let config = Config::default();

    // Assert:
    assert_eq!(config.token_registry_dataplane_id, None);
    assert_eq!(config.token_registry_instance_id, Some(1));
}

#[tokio::test]
#[serial]
#[traced_test]
async fn config_test_registry_instance_id_missing() {
    // Setup:
    env::set_var("TOKEN_REGISTRY_DATAPLANE_ID", "1");
    env::remove_var("TOKEN_REGISTRY_INSTANCE_ID");

    // Test:
    let config = Config::default();

    // Assert:
    assert_eq!(config.token_registry_dataplane_id, Some(1));
    assert_eq!(config.token_registry_instance_id, None);
}

#[tokio::test]
#[serial]
#[traced_test]
async fn config_test_no_env_var_missing() {
    // Setup:
    env::set_var("TOKEN_REGISTRY_DATAPLANE_ID", "1");
    env::set_var("TOKEN_REGISTRY_INSTANCE_ID", "1");

    // Test:
    let config = Config::default();

    // Assert:
    assert_eq!(config.token_registry_dataplane_id, Some(1));
    assert_eq!(config.token_registry_instance_id, Some(1));
}
