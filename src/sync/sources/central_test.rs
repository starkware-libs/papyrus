use futures_util::pin_mut;
use mockito::{mock, Matcher};
use tokio_stream::StreamExt;

use crate::starknet::BlockNumber;
use crate::starknet_client::objects::Block;
use crate::sync::CentralSource;

#[tokio::test]
async fn get_block_header() {
    let mut central_source = CentralSource::new(&mockito::server_url()).unwrap();

    // Prepare mock calls.
    let mock_last = mock("GET", "/feeder_gateway/get_last_batch_id")
        .with_status(200)
        .with_body("8")
        // TODO(dan): remove.
        .expect(2)
        .create();
    let mock_headers = mock("GET", "/feeder_gateway/get_block")
        // TODO(dan): consider using a regex.
        .match_query(Matcher::AnyOf(vec![
            Matcher::UrlEncoded("blockNumber".into(), "5".into()),
            Matcher::UrlEncoded("blockNumber".into(), "6".into()),
            Matcher::UrlEncoded("blockNumber".into(), "7".into()),
            Matcher::UrlEncoded("blockNumber".into(), "8".into()),
        ]))
        .with_status(200)
        .with_body(serde_json::to_string(&Block::default()).unwrap())
        .expect(4)
        .create();

    let last_block_number = central_source.get_block_number().await.unwrap();
    let mut initial_block_num = BlockNumber(5);
    let stream = central_source.stream_new_blocks(initial_block_num, Some(last_block_number));
    pin_mut!(stream);
    while let Some(Ok((block_number, _header))) = stream.next().await {
        assert_eq!(initial_block_num, block_number);
        initial_block_num = initial_block_num.next();
    }

    mock_last.assert();
    mock_headers.assert();
}
