use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use futures_util::pin_mut;
use mockito::{mock, Matcher};
use reqwest::StatusCode;
use tokio_stream::StreamExt;

use crate::starknet::BlockNumber;
use crate::starknet_client::{Block, ClientError};

use super::*;

fn create_test_central_source() -> CentralSource {
    let config = CentralSourceConfig {
        url: mockito::server_url(),
        retry_base_millis: 10,
        retry_max_delay_millis: 1000,
        max_retries: 4,
    };
    CentralSource::new(config).unwrap()
}

#[tokio::test]
async fn stream_block_headers() {
    let mut central_source = create_test_central_source();

    // Prepare mock calls.
    let mock_last = mock("GET", "/feeder_gateway/get_last_batch_id")
        .with_status(200)
        .with_body("8")
        .create();
    let mock_headers = mock("GET", "/feeder_gateway/get_block")
        // TODO(dan): consider using a regex.
        .match_query(Matcher::AnyOf(vec![
            Matcher::UrlEncoded("blockNumber".to_string(), "5".to_string()),
            Matcher::UrlEncoded("blockNumber".to_string(), "6".to_string()),
            Matcher::UrlEncoded("blockNumber".to_string(), "7".to_string()),
            Matcher::UrlEncoded("blockNumber".to_string(), "8".to_string()),
        ]))
        .with_status(200)
        .with_body(serde_json::to_string(&Block::default()).unwrap())
        .expect(4)
        .create();

    let last_block_number = central_source.get_block_number().await.unwrap();
    let mut initial_block_num = BlockNumber(5);
    let stream = central_source.stream_new_blocks(initial_block_num, last_block_number);
    pin_mut!(stream);
    while let Some((block_number, _header)) = stream.next().await {
        assert_eq!(initial_block_num, block_number);
        initial_block_num = initial_block_num.next();
    }
    mock_last.assert();
    mock_headers.assert();
}

#[tokio::test]
async fn test_retry() {
    let central_source = create_test_central_source();

    // Fails on all retries.
    let time = SystemTime::now();
    let err = central_source
        .retry::<u128, _>(|| async {
            Err(ClientError::BadResponse {
                status: StatusCode::TOO_MANY_REQUESTS,
            })
        })
        .await
        .unwrap_err();
    assert!((2110..2200).contains(&time.elapsed().unwrap().as_millis()));
    assert_matches!(err, ClientError::BadResponse { status } if status == StatusCode::TOO_MANY_REQUESTS);

    // Succeeds on the second attempt.
    let count = Arc::new(Mutex::new(0));
    let time = SystemTime::now();
    let res = central_source
        .retry(|| async {
            let mut guard = count.lock().unwrap();
            if *guard < 2 {
                *guard += 1;
                Err(ClientError::BadResponse {
                    status: StatusCode::TOO_MANY_REQUESTS,
                })
            } else {
                Ok(time.elapsed().unwrap().as_millis())
            }
        })
        .await
        .unwrap();
    assert!((110..200).contains(&res));
}
