use std::collections::HashMap;

use super::version_config::VERSION_CONFIG;
use crate::api::version_config::VersionState;

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
    // verify each version is listed only once
    config_version_counter.iter().for_each(|version_counter| assert_eq!(version_counter.1, &1))
}
