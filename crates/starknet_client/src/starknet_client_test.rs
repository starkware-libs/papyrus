use std::collections::HashMap;

use assert_matches::assert_matches;
use mockito::mock;
use starknet_api::serde_utils::bytes_from_hex_str;
use starknet_api::{
    shash, BlockHash, BlockNumber, ClassHash, ContractAddress, ContractClass, DeployedContract,
    EntryPoint, EntryPointOffset, EntryPointSelector, EntryPointType, Fee, GlobalRoot, Nonce,
    Program, StarkHash, StorageEntry, StorageKey, TransactionHash, TransactionSignature,
    TransactionVersion,
};

// TODO(dan): use SN structs once available & sort.
use super::objects::block::{BlockStateUpdate, StateDiff};
use super::objects::transaction::{DeclareTransaction, TransactionType};
use super::test_utils::read_resource::read_resource_file;
use super::{
    Block, ClientError, StarknetClient, BLOCK_NUMBER_QUERY, CLASS_HASH_QUERY, GET_BLOCK_URL,
    GET_STATE_UPDATE_URL,
};

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

fn contract_class_body() -> &'static str {
    r#"{
        "abi": [{
            "inputs": [{"name": "implementation", "type": "felt"}],
            "name": "constructor",
            "outputs": [],
            "type": "constructor"
        }],
        "entry_points_by_type": {
            "CONSTRUCTOR": [{
                "offset": "0x62",
                "selector": "0x28ffe4ff0f226a9107253e17a904099aa4f63a02a5621de0576e5aa71bc5194"
            }],
            "EXTERNAL": [{
                "offset": "0x86",
                "selector": "0x0"
            }],
            "L1_HANDLER": []
        },
        "program": {
            "builtins": [],
            "data": ["0x20780017fff7ffd", "0x4", "0x400780017fff7ffd"],
            "prime": "0x800000000000011000000000000000000000000000000000000000000000001",
            "main_scope": "__main__",
            "identifiers": {},
            "attributes": [],
            "debug_info": null,
            "reference_manager": {},
            "hints": {}
        }
    }"#
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
    let mock =
        mock("GET", &format!("/feeder_gateway/get_state_update?{}=123456", BLOCK_NUMBER_QUERY)[..])
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
async fn contract_class() {
    let starknet_client = StarknetClient::new(&mockito::server_url()).unwrap();
    let expected_contract_class = ContractClass {
        abi: serde_json::to_value(vec![HashMap::from([
            (
                "inputs".to_string(),
                serde_json::to_value(vec![HashMap::from([
                    ("name".to_string(), serde_json::Value::String("implementation".to_string())),
                    ("type".to_string(), serde_json::Value::String("felt".to_string())),
                ])])
                .unwrap(),
            ),
            ("name".to_string(), serde_json::Value::String("constructor".to_string())),
            ("type".to_string(), serde_json::Value::String("constructor".to_string())),
            ("outputs".to_string(), serde_json::Value::Array(Vec::new())),
        ])])
        .unwrap(),
        program: Program {
            attributes: serde_json::Value::Array(Vec::new()),
            builtins: serde_json::Value::Array(Vec::new()),
            data: serde_json::Value::Array(vec![
                serde_json::Value::String("0x20780017fff7ffd".to_string()),
                serde_json::Value::String("0x4".to_string()),
                serde_json::Value::String("0x400780017fff7ffd".to_string()),
            ]),
            debug_info: serde_json::Value::Null,
            hints: serde_json::Value::Object(serde_json::Map::new()),
            identifiers: serde_json::Value::Object(serde_json::Map::new()),
            main_scope: serde_json::Value::String("__main__".to_string()),
            prime: serde_json::Value::String(
                "0x800000000000011000000000000000000000000000000000000000000000001".to_string(),
            ),
            reference_manager: serde_json::Value::Object(serde_json::Map::new()),
        },
        entry_points_by_type: HashMap::from([
            (EntryPointType::L1Handler, vec![]),
            (
                EntryPointType::Constructor,
                vec![EntryPoint {
                    selector: EntryPointSelector(shash!(
                        "0x028ffe4ff0f226a9107253e17a904099aa4f63a02a5621de0576e5aa71bc5194"
                    )),
                    offset: EntryPointOffset(shash!(
                        "0x0000000000000000000000000000000000000000000000000000000000000062"
                    )),
                }],
            ),
            (
                EntryPointType::External,
                vec![EntryPoint {
                    selector: EntryPointSelector(shash!(
                        "0x0000000000000000000000000000000000000000000000000000000000000000"
                    )),
                    offset: EntryPointOffset(shash!(
                        "0x0000000000000000000000000000000000000000000000000000000000000086"
                    )),
                }],
            ),
        ]),
    };
    let mock_by_hash =
        mock(
            "GET",
            &format!("/feeder_gateway/get_class_by_hash?\
         {CLASS_HASH_QUERY}=0x7af612493193c771c1b12f511a8b4d3b0c6d0648242af4680c7cd0d06186f17")[..],
        )
        .with_status(200)
        .with_body(contract_class_body())
        .create();
    let contract_class = starknet_client
        .class_by_hash(ClassHash(shash!(
            "0x7af612493193c771c1b12f511a8b4d3b0c6d0648242af4680c7cd0d06186f17"
        )))
        .await
        .unwrap();
    mock_by_hash.assert();
    assert_eq!(contract_class, expected_contract_class);
}

#[tokio::test]
async fn get_block() {
    let starknet_client = StarknetClient::new(&mockito::server_url()).unwrap();
    let raw_block = read_resource_file("block.json");
    let mock = mock("GET", &format!("/feeder_gateway/get_block?{}=20", BLOCK_NUMBER_QUERY)[..])
        .with_status(200)
        .with_body(&raw_block)
        .create();
    let block = starknet_client.block(BlockNumber(20)).await.unwrap().unwrap();
    mock.assert();
    let expected_block: Block = serde_json::from_str(&raw_block).unwrap();
    assert_eq!(block, expected_block);
}

#[tokio::test]
async fn block_unserializable() {
    let starknet_client = StarknetClient::new(&mockito::server_url()).unwrap();
    let body =
        r#"{"block_hash": "0x3f65ef25e87a83d92f32f5e4869a33580f9db47ec980c1ff27bdb5151914de5"}"#;
    let mock = mock("GET", "/feeder_gateway/get_block?blockNumber=20")
        .with_status(200)
        .with_body(body)
        .create();
    let error = starknet_client.block(BlockNumber(20)).await.unwrap_err();
    mock.assert();
    assert_matches!(error, ClientError::SerdeError(_));
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
