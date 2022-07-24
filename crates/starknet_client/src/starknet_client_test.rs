use mockito::mock;
use starknet_api::{
    shash, BlockNumber, ClassHash, ContractAddress, Fee, Nonce, StarkHash, TransactionHash,
    TransactionSignature, TransactionVersion,
};

// TODO(dan): use SN structs once available & sort.
use super::objects::block::BlockStateUpdate;
use super::objects::transaction::{DeclareTransaction, TransactionType};
use super::test_utils::read_resource::read_resource_file;
use super::{Block, StarknetClient, GET_BLOCK_URL, GET_STATE_UPDATE_URL};

#[test]
fn test_new_urls() {
    let url_base_str = "https://url";
    let starknet_client = StarknetClient::new(url_base_str).unwrap();
    assert_eq!(
        starknet_client.urls.get_block.as_str(),
        url_base_str.to_string() + "/" + GET_BLOCK_URL
    );
    assert_eq!(
        starknet_client.urls.get_state_update.as_str(),
        url_base_str.to_string() + "/" + GET_STATE_UPDATE_URL
    );
}

#[tokio::test]
async fn get_block_number() {
    let starknet_client = StarknetClient::new(&mockito::server_url()).unwrap();

    // There are blocks in Starknet.
    let mock_block = mock("GET", "/feeder_gateway/get_block")
        .with_status(200)
        .with_body(read_resource_file("block.json"))
        .create();
    let block_number = starknet_client.block_number().await.unwrap();
    mock_block.assert();
    assert_eq!(block_number.unwrap(), BlockNumber(20));

    // There are no blocks in Starknet.
    let body = r#"{"code": "StarknetErrorCode.BLOCK_NOT_FOUND", "message": "Block number -1 was not found."}"#;
    let mock_no_block =
        mock("GET", "/feeder_gateway/get_block").with_status(500).with_body(body).create();
    let block_number = starknet_client.block_number().await.unwrap();
    mock_no_block.assert();
    assert!(block_number.is_none());
}

#[tokio::test]
async fn declare_tx_serde() {
    let declare_tx = DeclareTransaction {
        class_hash: ClassHash(shash!(
            "0x7319e2f01b0947afd86c0bb0e95029551b32f6dc192c47b2e8b08415eebbc25"
        )),
        sender_address: ContractAddress(shash!("0x1")),
        nonce: Nonce(shash!("0x0")),
        max_fee: Fee(0),
        version: TransactionVersion(shash!("0x1")),
        transaction_hash: TransactionHash(shash!(
            "0x2f2ef64daffdc72bf33b34ad024891691b8eb1d0ab70cc7f8fb71f6fd5e1f22"
        )),
        signature: TransactionSignature(vec![]),
        r#type: TransactionType::Declare,
    };
    let raw_declare_tx = serde_json::to_string(&declare_tx).unwrap();
    assert_eq!(declare_tx, serde_json::from_str(&raw_declare_tx).unwrap());
}

#[tokio::test]
async fn test_state_update() {
    let starknet_client = StarknetClient::new(&mockito::server_url()).unwrap();
    let raw_state_update = read_resource_file("block_state_update.json");
    let mock = mock("GET", "/feeder_gateway/get_state_update?blockNumber=123456")
        .with_status(200)
        .with_body(&raw_state_update)
        .create();
    let state_update = starknet_client.state_update(BlockNumber(123456)).await.unwrap();
    mock.assert();
    let expected_state_update: BlockStateUpdate = serde_json::from_str(&raw_state_update).unwrap();
    assert_eq!(state_update, expected_state_update);
}

#[tokio::test]
async fn get_block() {
    let starknet_client = StarknetClient::new(&mockito::server_url()).unwrap();
    let raw_block = read_resource_file("block.json");
    let mock = mock("GET", "/feeder_gateway/get_block?blockNumber=20")
        .with_status(200)
        .with_body(&raw_block)
        .create();
    let block = starknet_client.block(BlockNumber(20)).await.unwrap().unwrap();
    mock.assert();
    let expected_block: Block = serde_json::from_str(&raw_block).unwrap();
    assert_eq!(block, expected_block);
}

#[tokio::test]
async fn test_block_not_found_error_code() {
    let starknet_client = StarknetClient::new(&mockito::server_url()).unwrap();
    let body = r#"{"code": "StarknetErrorCode.BLOCK_NOT_FOUND", "message": "Block number 2347239846 was not found."}"#;
    let mock = mock("GET", "/feeder_gateway/get_block?blockNumber=2347239846")
        .with_status(500)
        .with_body(body)
        .create();
    assert!(starknet_client.block(BlockNumber(2347239846)).await.unwrap().is_none());
    mock.assert();
}
