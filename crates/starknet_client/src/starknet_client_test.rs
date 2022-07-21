use std::collections::HashMap;

use mockito::mock;
use starknet_api::serde_utils::bytes_from_hex_str;
use starknet_api::{
    shash, BlockHash, BlockNumber, ClassHash, ContractAddress, DeployedContract, Fee, GlobalRoot,
    Nonce, StarkHash, StorageEntry, StorageKey, TransactionHash, TransactionSignature,
    TransactionVersion,
};

// TODO(dan): use SN structs once available & sort.
use super::objects::block::{BlockStateUpdate, StateDiff};
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
    let state_update = starknet_client.state_update(BlockNumber(123456)).await.unwrap();
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
                class_hash: ClassHash(shash!(
                    "0x071c3c99f5cf76fc19945d4b8b7d34c7c5528f22730d56192b50c6bbfd338a64"
                )),
            }],
            declared_contracts: vec![],
        },
    };
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
