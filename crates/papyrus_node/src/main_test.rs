use assert_matches::assert_matches;
use papyrus_node::config::NodeConfig;
use tempdir::TempDir;

use crate::run_threads;

#[tokio::test]
async fn run_threads_stop() {
    let tmp_data_dir = TempDir::new("./data_for_test").unwrap();
    let mut config = NodeConfig::default();
    config.storage.db_config.path_prefix = tmp_data_dir.path().into();

    // Error when not overriding the base layer node URL.
    assert_matches!(run_threads(config.clone()).await, Err(_));

    // Error when not supplying legal central URL.
    config.base_layer.node_url = "value".to_string();
    config.central.url = "_not_legal_url".to_string();
    assert_matches!(run_threads(config.clone()).await, Err(_));
}
