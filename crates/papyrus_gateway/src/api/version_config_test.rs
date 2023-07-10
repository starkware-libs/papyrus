use std::collections::HashMap;

use assert_matches::assert_matches;
use pretty_assertions::assert_eq;

use super::version_config::VERSION_CONFIG;
use crate::api::version_config::{VersionState, LATEST_VERSION_ID};

#[tokio::test]
async fn validate_version_configuration() {
    let mut config_type_counter =
        HashMap::from([(&VersionState::Supported, 0), (&VersionState::Deprecated, 0)]);
    let mut config_version_counter = HashMap::new();
    VERSION_CONFIG.iter().for_each(|config| {
        let (version_id, version_state) = config;
        config_type_counter.entry(version_state).and_modify(|counter| *counter += 1);
        config_version_counter.entry(*version_id).and_modify(|counter| *counter += 1).or_insert(1);
    });
    // verify latest version is also supported
    assert_matches!(VERSION_CONFIG.iter().find(|version| version.0 == LATEST_VERSION_ID), Some(_));
    // verify each version is listed once
    config_version_counter.iter().for_each(|version_counter| assert_eq!(*version_counter.1, 1))
}
