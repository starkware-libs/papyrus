use papyrus_node::config::NodeConfig;
use tempfile::TempDir;

use crate::run_threads;

#[tokio::test]
async fn run_threads_stop() {
    let mut config = NodeConfig::default();
    config.storage.db_config.path_prefix = TempDir::new().unwrap().path().into();

    // Error when not supplying legal central URL.
    config.central.url = "_not_legal_url".to_string();
    assert!(run_threads(config.clone()).await.is_err());
}
