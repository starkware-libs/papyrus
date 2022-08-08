use std::collections::HashMap;

use assert_matches::assert_matches;
use mockito::mock;
use reqwest::StatusCode;
use starknet_api::{
    shash, BlockNumber, ClassHash, ContractAddress, ContractClass, EntryPoint, EntryPointOffset,
    EntryPointSelector, EntryPointType, Fee, Nonce, Program, StarkHash, TransactionHash,
    TransactionSignature, TransactionVersion,
};

// TODO(dan): use SN structs once available & sort.
use super::objects::block::BlockStateUpdate;
use super::objects::transaction::{DeclareTransaction, TransactionType};
use super::test_utils::read_resource::read_resource_file;
use super::test_utils::retry::get_test_config;
use super::{
    Block, ClientError, RetryErrorCode, StarknetClient, StarknetClientTrait, BLOCK_NUMBER_QUERY,
    CLASS_HASH_QUERY, GET_BLOCK_URL, GET_STATE_UPDATE_URL,
};

#[test]
fn test_new_urls() {
    let url_base_str = "https://url";
    let starknet_client = StarknetClient::new(url_base_str, get_test_config()).unwrap();
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
            "attributes": [1234],
            "debug_info": null,
            "reference_manager": {},
            "hints": {}
        }
    }"#
}
#[tokio::test]
async fn get_block_number() {
    let starknet_client = StarknetClient::new(&mockito::server_url(), get_test_config()).unwrap();

    // There are blocks in Starknet.
    let mock_block = mock("GET", "/feeder_gateway/get_block")
        .with_status(200)
        .with_body(read_resource_file("block.json"))
        .create();
    let block_number = starknet_client.block_number().await.unwrap();
    mock_block.assert();
    assert_eq!(block_number.unwrap(), BlockNumber(273466));

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
    let starknet_client = StarknetClient::new(&mockito::server_url(), get_test_config()).unwrap();
    let raw_state_update = read_resource_file("block_state_update.json");
    let mock =
        mock("GET", &format!("/feeder_gateway/get_state_update?{}=123456", BLOCK_NUMBER_QUERY)[..])
            .with_status(200)
            .with_body(&raw_state_update)
            .create();
    let state_update = starknet_client.state_update(BlockNumber(123456)).await.unwrap();
    mock.assert();
    let expected_state_update: BlockStateUpdate = serde_json::from_str(&raw_state_update).unwrap();
    assert_eq!(state_update, expected_state_update);
}

#[tokio::test]
async fn test_serialization_precision() {
    let input =
        "{\"value\":244116128358498188146337218061232635775543270890529169229936851982759783745}";
    let serialized = serde_json::from_str::<serde_json::Value>(input).unwrap();
    let deserialized = serde_json::to_string(&serialized).unwrap();
    assert_eq!(input, deserialized);
}

#[tokio::test]
async fn contract_class() {
    let starknet_client = StarknetClient::new(&mockito::server_url(), get_test_config()).unwrap();
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
            attributes: serde_json::Value::Array(vec![serde_json::json!(1234)]),
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
    let starknet_client = StarknetClient::new(&mockito::server_url(), get_test_config()).unwrap();
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
    let starknet_client = StarknetClient::new(&mockito::server_url(), get_test_config()).unwrap();
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
    let starknet_client = StarknetClient::new(&mockito::server_url(), get_test_config()).unwrap();
    let body = r#"{"code": "StarknetErrorCode.BLOCK_NOT_FOUND", "message": "Block number 2347239846 was not found."}"#;
    let mock = mock("GET", "/feeder_gateway/get_block?blockNumber=2347239846")
        .with_status(500)
        .with_body(body)
        .create();
    assert!(starknet_client.block(BlockNumber(2347239846)).await.unwrap().is_none());
    mock.assert();
}

#[tokio::test]
async fn retry_error_codes() {
    let starknet_client = StarknetClient::new(&mockito::server_url(), get_test_config()).unwrap();
    for (status_code, error_code) in [
        (StatusCode::TEMPORARY_REDIRECT, RetryErrorCode::Redirect),
        (StatusCode::REQUEST_TIMEOUT, RetryErrorCode::Timeout),
        (StatusCode::TOO_MANY_REQUESTS, RetryErrorCode::TooManyRequests),
        (StatusCode::SERVICE_UNAVAILABLE, RetryErrorCode::ServiceUnavailable),
        (StatusCode::GATEWAY_TIMEOUT, RetryErrorCode::Timeout),
    ] {
        let mock = mock("GET", "/feeder_gateway/get_block")
            .with_status(status_code.as_u16().into())
            .expect(5)
            .create();
        let error = starknet_client.block_number().await.unwrap_err();
        assert_matches!(error, ClientError::RetryError { code } if code == error_code);
        mock.assert();
    }
}
