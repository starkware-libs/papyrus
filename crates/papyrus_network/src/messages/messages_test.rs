use std::time::Duration;

use futures::AsyncWriteExt;
use pretty_assertions::assert_eq;

use super::block::GetBlocksResponse;
use super::{read_message, write_message};
use crate::test_utils::{get_connected_streams, hardcoded_data};

#[tokio::test]
async fn read_write_positive_flow() {
    let (mut stream1, mut stream2, _) = get_connected_streams().await;
    let messages = hardcoded_data();
    for message in &messages {
        write_message(message.clone(), &mut stream1).await.unwrap();
    }
    for expected_message in &messages {
        assert_eq!(*expected_message, read_message(&mut stream2).await.unwrap().unwrap());
    }
}

#[tokio::test]
async fn read_message_returns_none_when_other_stream_is_closed() {
    let (mut stream1, mut stream2, _) = get_connected_streams().await;
    stream1.close().await.unwrap();
    assert!(read_message::<GetBlocksResponse, _>(&mut stream2).await.unwrap().is_none());
}

#[tokio::test]
async fn read_message_is_pending_when_other_stream_didnt_send() {
    let (_stream1, mut stream2, _) = get_connected_streams().await;
    assert!(
        tokio::time::timeout(
            Duration::from_millis(10),
            read_message::<GetBlocksResponse, _>(&mut stream2)
        )
        .await
        .is_err()
    );
}
