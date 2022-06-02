use mockito::mock;

use crate::starknet::{
    BlockHash, BlockHeader, BlockNumber, BlockTimestamp, ContractAddress, GasPrice, GlobalRoot,
    StarkHash,
};

use super::StarknetClient;

#[tokio::test]
async fn get_block_number() {
    let starknet_client: StarknetClient = StarknetClient::new(&mockito::server_url()).unwrap();
    let mock = mock("GET", "/feeder_gateway/get_last_batch_id")
        .with_status(200)
        .with_body("195812")
        .create();
    let block_number: BlockNumber = starknet_client.block_number().await.unwrap();
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
        block_hash: BlockHash(StarkHash([
            6, 66, 182, 41, 173, 140, 226, 51, 181, 87, 152, 200, 59, 182, 41, 165, 155, 240, 160,
            9, 47, 103, 218, 40, 214, 214, 103, 118, 104, 13, 84, 131,
        ])),
        parent_hash: BlockHash(StarkHash([
            7, 215, 77, 252, 43, 216, 122, 200, 148, 71, 165, 108, 81, 171, 201, 182, 217, 172,
            161, 222, 33, 204, 37, 253, 153, 34, 243, 163, 119, 158, 199, 45,
        ])),
        gas_price: GasPrice(0x174876e800),
        number: BlockNumber(20),
        sequencer: ContractAddress(StarkHash([
            3, 123, 44, 214, 186, 170, 81, 95, 82, 3, 131, 190, 231, 183, 9, 79, 137, 47, 76, 119,
            6, 149, 252, 50, 154, 137, 115, 232, 65, 169, 113, 174,
        ])),
        timestamp: BlockTimestamp(1636991716),
        state_root: GlobalRoot(StarkHash([
            7, 232, 9, 162, 223, 190, 149, 164, 10, 216, 229, 161, 38, 131, 232, 214, 76, 25, 78,
            103, 161, 180, 64, 2, 123, 252, 226, 54, 131, 233, 251, 72,
        ])),
    };
    assert_eq!(block_header, expected_block_header);
}
