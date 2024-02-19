use std::time::Duration;

use futures::{AsyncReadExt, AsyncWriteExt};
use pretty_assertions::assert_eq;

use super::{
    read_message,
    read_message_without_length_prefix,
    write_message,
    write_message_without_length_prefix,
};
use crate::test_utils::{dummy_data, get_connected_streams};

#[tokio::test]
async fn read_write_positive_flow() {
    let (mut stream1, mut stream2, _) = get_connected_streams().await;
    let messages = dummy_data();
    for message in &messages {
        write_message(message, &mut stream1).await.unwrap();
    }
    for expected_message in &messages {
        assert_eq!(*expected_message, read_message(&mut stream2).await.unwrap().unwrap());
    }
}

#[tokio::test]
async fn read_write_without_length_prefix_positive_flow() {
    let (stream1, stream2, _) = get_connected_streams().await;
    let (_read_stream1, write_stream1) = stream1.split();
    let (read_stream2, _write_stream2) = stream2.split();
    let message = dummy_data().first().unwrap().clone();
    write_message_without_length_prefix(&message, write_stream1).await.unwrap();
    assert_eq!(message, read_message_without_length_prefix(read_stream2).await.unwrap());
}

#[tokio::test]
async fn read_message_returns_none_when_other_stream_is_closed() {
    let (mut stream1, mut stream2, _) = get_connected_streams().await;
    stream1.close().await.unwrap();
    assert!(read_message(&mut stream2).await.unwrap().is_none());
}

#[tokio::test]
async fn read_message_is_pending_when_other_stream_didnt_send() {
    let (_stream1, mut stream2, _) = get_connected_streams().await;
    assert!(
        tokio::time::timeout(Duration::from_millis(10), read_message(&mut stream2)).await.is_err()
    );
}
