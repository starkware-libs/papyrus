use mockito::mock;

use crate::starknet::BlockNumber;

use super::*;

#[tokio::test]
async fn get_block_number() {
    let starknet_client = StarknetClient::new(&mockito::server_url()).unwrap();
    let mock = mock("GET", "/feeder_gateway/get_last_batch_id")
        .with_status(200)
        .with_body("195812")
        .create();
    let block_number: BlockNumber = starknet_client.block_number().await.unwrap();
    mock.assert();
    assert_eq!(block_number, BlockNumber(195812));
}
