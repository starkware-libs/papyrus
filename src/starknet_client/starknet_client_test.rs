use mockito::mock;

use crate::starknet::{
    BlockHash, BlockHeader, BlockNumber, BlockTimestamp, ContractAddress, GasPrice, GlobalRoot,
    StarkHash,
};
// TODO(dan): use SN structs once avilable & sort.
use crate::starknet_client::objects::transactions::{
    CallData, EntryPointSelector, EntryPointType, InvokeTransaction, MaxFee, TransactionHash,
    TransactionSignature, TransactionType,
};
use crate::starknet_client::objects::{
    ContractAddress as OtherContractAddress, StarkHash as OtherStarkHash,
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
async fn get_block_data() {
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
            "transactions": [
                {
                    "contract_address": "0x639897809c39093765f34d76776b8d081904ab30184f694f20224723ef07863",
                    "entry_point_selector": "0x15d40a3d6ca2ac30f4031e42be28da9b056fef9bb7357ac5e85627ee876e5ad",
                    "entry_point_type": "EXTERNAL",
                    "calldata": [
                        "0x3",
                        "0x4bc8ac16658025bff4a3bd0760e84fcf075417a4c55c6fae716efdd8f1ed26c"
                    ],
                    "signature": [
                        "0xbe0d6cdf1333a316ab03b7f057ee0c66716d3d983fa02ad4c46389cbe3bb75",
                        "0x396ec012117a44f204e3b501217502c9b261ef5d3da341757026df844a99d4a"
                    ],
                    "transaction_hash": "0xb7bcb42e0cfb09e38a2c21061f72d36271cc8cf13647938d4e41066c051ea8",
                    "max_fee": "0x6e0917047fd8",
                    "type": "INVOKE_FUNCTION"
                }
            ]
        }"#;
    let mock = mock("GET", "/feeder_gateway/get_block?blockNumber=20")
        .with_status(200)
        .with_body(body)
        .create();
    let (block_header, transactions) = starknet_client.block_data(BlockNumber(20)).await.unwrap();
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
    let expected_transactions = vec![Transaction::Invoke(InvokeTransaction {
        calldata: CallData(vec![
            (OtherStarkHash(bytes_from_hex_str::<32, true>("0x3").unwrap())),
            (OtherStarkHash(
                bytes_from_hex_str::<32, true>(
                    "0x4bc8ac16658025bff4a3bd0760e84fcf075417a4c55c6fae716efdd8f1ed26c",
                )
                .unwrap(),
            )),
        ]),
        contract_address: OtherContractAddress(OtherStarkHash(
            bytes_from_hex_str::<32, true>(
                "0x639897809c39093765f34d76776b8d081904ab30184f694f20224723ef07863",
            )
            .unwrap(),
        )),
        entry_point_selector: EntryPointSelector(OtherStarkHash(
            bytes_from_hex_str::<32, true>(
                "0x15d40a3d6ca2ac30f4031e42be28da9b056fef9bb7357ac5e85627ee876e5ad",
            )
            .unwrap(),
        )),
        entry_point_type: EntryPointType::External,
        max_fee: MaxFee(0x6e0917047fd8),
        signature: TransactionSignature(vec![
            (OtherStarkHash(
                bytes_from_hex_str::<32, true>(
                    "0xbe0d6cdf1333a316ab03b7f057ee0c66716d3d983fa02ad4c46389cbe3bb75",
                )
                .unwrap(),
            )),
            (OtherStarkHash(
                bytes_from_hex_str::<32, true>(
                    "0x396ec012117a44f204e3b501217502c9b261ef5d3da341757026df844a99d4a",
                )
                .unwrap(),
            )),
        ]),
        transaction_hash: TransactionHash(OtherStarkHash(
            bytes_from_hex_str::<32, true>(
                "0xb7bcb42e0cfb09e38a2c21061f72d36271cc8cf13647938d4e41066c051ea8",
            )
            .unwrap(),
        )),
        r#type: TransactionType::InvokeFunction,
    })];
    assert_eq!(transactions, expected_transactions);
}

#[tokio::test]
async fn test_block_not_found_error_code() {
    let starknet_client = StarknetClient::new(&mockito::server_url()).unwrap();
    let body = r#"{"code": "StarknetErrorCode.BLOCK_NOT_FOUND", "message": "Block number 2347239846 was not found."}"#;
    let mock = mock("GET", "/feeder_gateway/get_block?blockNumber=2347239846")
        .with_status(500)
        .with_body(body)
        .create();
    let err = starknet_client
        .block_data(BlockNumber(2347239846))
        .await
        .unwrap_err();
    mock.assert();
    assert_matches!(
        err,
        ClientError::StarknetError(StarknetError {
            code: StarknetErrorCode::BlockNotFound,
            message: msg
        }) if msg == "Block number 2347239846 was not found."
    );
}
