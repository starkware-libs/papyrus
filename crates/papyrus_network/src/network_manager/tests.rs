use futures::future::poll_fn;
use futures::SinkExt;
use papyrus_storage::test_utils::get_test_storage;

use super::NetworkManager;
use crate::{NetworkConfig, Query};

#[tokio::test]
async fn register_subscriber_and_use_channels() {
    let ((storage_reader, _storage_writer), _temp_dir) = get_test_storage();
    let mut net_manager = NetworkManager::new(NetworkConfig::default(), storage_reader);

    let (mut query_sender, _response_receiver) = net_manager.register_subscriber();
    let query = Query::default();
    poll_fn(|cx| query_sender.poll_ready_unpin(cx)).await.unwrap();
    query_sender.start_send_unpin(query).unwrap();

    assert_eq!(query_sender.query_id, 0);

    // TODO: receive data once network manager can get a swarm mock.
}
