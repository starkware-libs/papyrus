use mockito::mock;

use crate::starknet::{
    BlockHash, BlockHeader, BlockNumber, BlockTimestamp, ContractAddress, GasPrice, GlobalRoot,
    StarkHash,
};

use super::serde_utils::bytes_from_hex_str;
use super::*;

#[tokio::test]
async fn get_block_number() {
    let starknet_client = StarknetClient::new(&mockito::server_url()).unwrap();
    let mock = mock("GET", "/feeder_gateway/get_last_batch_id")
        .with_status(200)
        .with_body("195812")
        .create();
    let block_number = starknet_client.block_number().await.unwrap();
    mock.assert();
    assert_eq!(block_number, BlockNumber(195812));
}

#[tokio::test]
async fn get_block_header() {
    let starknet_client = StarknetClient::new(&mockito::server_url()).unwrap();
    let body = r#"
        {
            "block_hash": "0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5483",
            "parent_block_hash": "0x7d74dfc2bd87ac89447a56c51abc9b6d9aca1de21cc25fd9922f3a3779ec72d",
            "block_number": 20,
            "state_root": "07e809a2dfbe95a40ad8e5a12683e8d64c194e67a1b440027bfce23683e9fb48",
            "timestamp": 1636991716,
            "sequencer_address": "0x37b2cd6baaa515f520383bee7b7094f892f4c770695fc329a8973e841a971ae",
            "status": "ACCEPTED_ON_L1",
            "gas_price": "0x174876e800",
            "transaction_receipts": [],
            "transactions": []
        }"#;
    let mock = mock("GET", "/feeder_gateway/get_block?blockNumber=20")
        .with_status(200)
        .with_body(body)
        .create();
    let block_header: BlockHeader = starknet_client.block_header(20).await.unwrap();
    mock.assert();
    let expected_block_header = BlockHeader {
        block_hash: BlockHash(StarkHash(
            bytes_from_hex_str::<32, true>(
                "0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5483",
            )
            .unwrap(),
        )),
        parent_hash: BlockHash(StarkHash(
            bytes_from_hex_str::<32, true>(
                "0x7d74dfc2bd87ac89447a56c51abc9b6d9aca1de21cc25fd9922f3a3779ec72d",
            )
            .unwrap(),
        )),
        gas_price: GasPrice(0x174876e800),
        number: BlockNumber(20),
        sequencer: ContractAddress(StarkHash(
            bytes_from_hex_str::<32, true>(
                "0x37b2cd6baaa515f520383bee7b7094f892f4c770695fc329a8973e841a971ae",
            )
            .unwrap(),
        )),
        timestamp: BlockTimestamp(1636991716),
        state_root: GlobalRoot(StarkHash(
            bytes_from_hex_str::<32, false>(
                "07e809a2dfbe95a40ad8e5a12683e8d64c194e67a1b440027bfce23683e9fb48",
            )
            .unwrap(),
        )),
    };
    assert_eq!(block_header, expected_block_header);
}

#[tokio::test]
async fn test_block_not_found_error_code() {
    let starknet_client = StarknetClient::new(&mockito::server_url()).unwrap();
    let body = r#"{"code": "StarknetErrorCode.BLOCK_NOT_FOUND", "message": "Block number 2347239846 was not found."}"#;
    let mock = mock("GET", "/feeder_gateway/get_block?blockNumber=2347239846")
        .with_status(500)
        .with_body(body)
        .create();
    let err = starknet_client.block_header(2347239846).await.unwrap_err();
    mock.assert();
    assert_matches!(
        err,
        ClientError::StarknetError(StarknetError {
            code: StarknetErrorCode::BlockNotFound,
            ..
        })
    );
}
