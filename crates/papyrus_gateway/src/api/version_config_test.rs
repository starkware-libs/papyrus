use std::collections::HashMap;

use assert_matches::assert_matches;

use super::version_config::VERSION_CONFIG;
use crate::api::version_config::{VersionState, LATEST_VERSION_ID};

#[tokio::test]
async fn validate_version_configuration() {
    let mut config_type_counter = HashMap::from([
        (&VersionState::Latest, 0),
        (&VersionState::Supported, 0),
        (&VersionState::Deprecated, 0),
    ]);
    let mut config_version_counter = HashMap::new();
    VERSION_CONFIG.iter().for_each(|config| {
        let (version_id, version_state) = config;
        config_type_counter.entry(version_state).and_modify(|counter| *counter += 1);
        config_version_counter.entry(*version_id).and_modify(|counter| *counter += 1).or_insert(1);
    });
    // verify only one version is defined as latest
    assert_eq!(config_type_counter.get(&VersionState::Latest), Some(&1));
    // verify each version is listed once for non latest version or twice in case it is the latest
    // version
    config_version_counter.iter().for_each(
        |version_counter| assert_matches!(*version_counter.1, 1 | 2 if *version_counter.0 == LATEST_VERSION_ID),
    )
}

#[tokio::test]
async fn test_latest_version_id_in_version_config() {
    let Some(res) = VERSION_CONFIG.iter().find(|version| version.1 == VersionState::Latest) else {panic!("no latest version found")};
    assert_eq!(LATEST_VERSION_ID, res.0);
}
