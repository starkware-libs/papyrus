use futures_util::pin_mut;
use mockito::{mock, Matcher};
use tokio_stream::StreamExt;

use crate::starknet::BlockNumber;
use crate::starknet_client::Block;
use crate::sync::CentralSource;

use super::CentralSourceConfig;

#[tokio::test]
async fn stream_block_headers() {
    let config = CentralSourceConfig {
        url: mockito::server_url(),
    };
    let central_source = CentralSource::new(config).unwrap();

    // Prepare mock calls.
    let mock_last = mock("GET", "/feeder_gateway/get_last_batch_id")
        .with_status(200)
        .with_body("9")
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
    while let Some((block_number, _header, _body)) = stream.next().await {
        assert_eq!(initial_block_num, block_number);
        initial_block_num = initial_block_num.next();
    }
    mock_last.assert();
    mock_headers.assert();
}
