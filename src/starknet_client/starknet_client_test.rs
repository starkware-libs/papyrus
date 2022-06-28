use std::collections::HashMap;

use mockito::mock;

use crate::starknet::serde_utils::bytes_from_hex_str;
use crate::starknet::{
    shash, BlockHash, BlockNumber, BlockTimestamp, ClassHash, ContractAddress, DeployedContract,
    GasPrice, GlobalRoot, StarkHash, StorageEntry, StorageKey,
};
use crate::starknet_client::objects::block::BlockStatus;

use super::*;

// TODO(dan): use SN structs once avilable & sort.
use super::objects::block::{BlockStateUpdate, StateDiff};
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
async fn test_state_update() {
    let starknet_client = StarknetClient::new(&mockito::server_url()).unwrap();
    let body = r#"
    {
        "block_hash": "0x3f65ef25e87a83d92f32f5e4869a33580f9db47ec980c1ff27bdb5151914de5",
        "new_root": "02ade8eea6eb6523d22a408a1f035bd351a9a5dce28926ca92d7abb490c0e74a",
        "old_root": "0465b219d93bcb2776aa3abb009423be3e2d04dba6453d7e027830740cd699a4",
        "state_diff":
        {
            "storage_diffs":
            {
                "0x13386f165f065115c1da38d755be261023c32f0134a03a8e66b6bb1e0016014":
                [
                    {
                        "key": "0x3b3a699bb6ef37ff4b9c4e14319c7d8e9c9bdd10ff402d1ebde18c62ae58381",
                        "value": "0x61454dd6e5c83621e41b74c"
                    },
                    {
                        "key": "0x1557182e4359a1f0c6301278e8f5b35a776ab58d39892581e357578fb287836",
                        "value": "0x79dd8085e3e5a96ea43e7d"
                    }
                ]
            },
            "deployed_contracts":
            [
                {
                    "address": "0x3e10411edafd29dfe6d427d03e35cb261b7a5efeee61bf73909ada048c029b9",
                    "class_hash": "0x071c3c99f5cf76fc19945d4b8b7d34c7c5528f22730d56192b50c6bbfd338a64"
                }
            ]
        }
    }"#;
    let mock = mock("GET", "/feeder_gateway/get_state_update?blockNumber=123456")
        .with_status(200)
        .with_body(body)
        .create();
    let state_update = starknet_client
        .state_update(BlockNumber(123456))
        .await
        .unwrap();
    mock.assert();
    let expected_state_update = BlockStateUpdate {
        block_hash: BlockHash(shash!(
            "0x3f65ef25e87a83d92f32f5e4869a33580f9db47ec980c1ff27bdb5151914de5"
        )),
        new_root: GlobalRoot(StarkHash(
            bytes_from_hex_str::<32, false>(
                "02ade8eea6eb6523d22a408a1f035bd351a9a5dce28926ca92d7abb490c0e74a",
            )
            .unwrap(),
        )),
        old_root: GlobalRoot(StarkHash(
            bytes_from_hex_str::<32, false>(
                "0465b219d93bcb2776aa3abb009423be3e2d04dba6453d7e027830740cd699a4",
            )
            .unwrap(),
        )),
        state_diff: StateDiff {
            storage_diffs: HashMap::from([(
                ContractAddress(shash!(
                    "0x13386f165f065115c1da38d755be261023c32f0134a03a8e66b6bb1e0016014"
                )),
                vec![
                    StorageEntry {
                        key: StorageKey(shash!(
                            "0x3b3a699bb6ef37ff4b9c4e14319c7d8e9c9bdd10ff402d1ebde18c62ae58381"
                        )),
                        value: shash!("0x61454dd6e5c83621e41b74c"),
                    },
                    StorageEntry {
                        key: StorageKey(shash!(
                            "0x1557182e4359a1f0c6301278e8f5b35a776ab58d39892581e357578fb287836"
                        )),
                        value: shash!("0x79dd8085e3e5a96ea43e7d"),
                    },
                ],
            )]),
            deployed_contracts: vec![DeployedContract {
                address: ContractAddress(shash!(
                    "0x3e10411edafd29dfe6d427d03e35cb261b7a5efeee61bf73909ada048c029b9"
                )),
                class_hash: ClassHash(StarkHash(
                    bytes_from_hex_str::<32, false>(
                        "071c3c99f5cf76fc19945d4b8b7d34c7c5528f22730d56192b50c6bbfd338a64",
                    )
                    .unwrap(),
                )),
            }],
            declared_contracts: vec![],
        },
    };
    assert_eq!(state_update, expected_state_update);
    // }
}

#[tokio::test]
async fn get_block() {
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
    let block = starknet_client.block(BlockNumber(20)).await.unwrap();
    mock.assert();
    let expected_block = Block {
        block_hash: BlockHash(shash!(
            "0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5483"
        )),
        parent_block_hash: BlockHash(shash!(
            "0x7d74dfc2bd87ac89447a56c51abc9b6d9aca1de21cc25fd9922f3a3779ec72d"
        )),
        gas_price: GasPrice(0x174876e800),
        block_number: BlockNumber(20),
        sequencer_address: ContractAddress(shash!(
            "0x37b2cd6baaa515f520383bee7b7094f892f4c770695fc329a8973e841a971ae"
        )),
        status: BlockStatus::AcceptedOnL1,
        timestamp: BlockTimestamp(1636991716),
        state_root: GlobalRoot(StarkHash(
            bytes_from_hex_str::<32, false>(
                "07e809a2dfbe95a40ad8e5a12683e8d64c194e67a1b440027bfce23683e9fb48",
            )
            .unwrap(),
        )),
        transactions: vec![],
        transaction_receipts: vec![],
    };
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
    let err = starknet_client
        .block(BlockNumber(2347239846))
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
