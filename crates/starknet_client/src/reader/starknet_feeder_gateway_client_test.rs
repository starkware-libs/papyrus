use std::collections::HashMap;
use std::fmt::Debug;
use std::future::Future;

use assert_matches::assert_matches;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use indexmap::indexmap;
use mockito::mock;
use pretty_assertions::assert_eq;
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::core::{
    ClassHash,
    ContractAddress,
    EntryPointSelector,
    GlobalRoot,
    Nonce,
    PatriciaKey,
    SequencerPublicKey,
};
use starknet_api::crypto::utils::PublicKey;
use starknet_api::deprecated_contract_class::{
    ConstructorType,
    ContractClass as DeprecatedContractClass,
    ContractClassAbiEntry,
    EntryPoint as DeprecatedEntryPoint,
    EntryPointOffset,
    EntryPointType as DeprecatedEntryPointType,
    FunctionAbiEntry,
    Program,
    TypedParameter,
};
use starknet_api::state::{EntryPoint, EntryPointType, FunctionIndex};
use starknet_api::transaction::{Fee, TransactionHash, TransactionSignature, TransactionVersion};
use starknet_api::{felt, patricia_key};

use super::objects::state::StateUpdate;
use super::objects::transaction::IntermediateDeclareTransaction;
use super::{
    ContractClass,
    GenericContractClass,
    PendingData,
    ReaderClientError,
    ReaderClientResult,
    StarknetFeederGatewayClient,
    StarknetReader,
    BLOCK_NUMBER_QUERY,
    CLASS_HASH_QUERY,
    GET_BLOCK_URL,
    GET_STATE_UPDATE_URL,
};
use crate::reader::objects::block::{BlockSignatureData, BlockSignatureMessage};
use crate::reader::Block;
use crate::test_utils::read_resource::read_resource_file;
use crate::test_utils::retry::get_test_config;

const NODE_VERSION: &str = "NODE VERSION";
const FEEDER_GATEWAY_ALIVE_RESPONSE: &str = "FeederGateway is alive!";

#[test]
fn new_urls() {
    let url_base_str = "https://url";
    let starknet_client =
        StarknetFeederGatewayClient::new(url_base_str, None, NODE_VERSION, get_test_config())
            .unwrap();
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
    let starknet_client = StarknetFeederGatewayClient::new(
        &mockito::server_url(),
        None,
        NODE_VERSION,
        get_test_config(),
    )
    .unwrap();
    // There are blocks in Starknet.
    let mock_block = mock("GET", "/feeder_gateway/get_block?blockNumber=latest")
        .with_status(200)
        .with_body(read_resource_file("reader/block_post_0_13_1.json"))
        .create();
    let latest_block = starknet_client.latest_block().await.unwrap();
    mock_block.assert();
    assert_eq!(latest_block.unwrap().block_number(), BlockNumber(329525));

    // There are no blocks in Starknet.
    let body = r#"{"code": "StarknetErrorCode.BLOCK_NOT_FOUND", "message": "Block number -1 was not found."}"#;
    let mock_no_block = mock("GET", "/feeder_gateway/get_block?blockNumber=latest")
        .with_status(400)
        .with_body(body)
        .create();
    let latest_block = starknet_client.latest_block().await.unwrap();
    mock_no_block.assert();
    assert!(latest_block.is_none());
}

#[tokio::test]
async fn declare_tx_serde() {
    let declare_tx = IntermediateDeclareTransaction {
        class_hash: ClassHash(felt!(
            "0x7319e2f01b0947afd86c0bb0e95029551b32f6dc192c47b2e8b08415eebbc25"
        )),
        compiled_class_hash: None,
        sender_address: ContractAddress(patricia_key!("0x1")),
        nonce: Nonce(felt!("0x0")),
        max_fee: Some(Fee(0)),
        version: TransactionVersion::ONE,
        resource_bounds: None,
        tip: None,
        nonce_data_availability_mode: None,
        fee_data_availability_mode: None,
        paymaster_data: None,
        account_deployment_data: None,
        transaction_hash: TransactionHash(felt!(
            "0x2f2ef64daffdc72bf33b34ad024891691b8eb1d0ab70cc7f8fb71f6fd5e1f22"
        )),
        signature: TransactionSignature(vec![]),
    };
    let raw_declare_tx = serde_json::to_string(&declare_tx).unwrap();
    assert_eq!(declare_tx, serde_json::from_str(&raw_declare_tx).unwrap());
}

#[tokio::test]
async fn state_update() {
    let starknet_client = StarknetFeederGatewayClient::new(
        &mockito::server_url(),
        None,
        NODE_VERSION,
        get_test_config(),
    )
    .unwrap();
    let raw_state_update = read_resource_file("reader/block_state_update.json");
    let mock_state_update =
        mock("GET", &format!("/feeder_gateway/get_state_update?{BLOCK_NUMBER_QUERY}=123456")[..])
            .with_status(200)
            .with_body(&raw_state_update)
            .create();
    let state_update = starknet_client.state_update(BlockNumber(123456)).await.unwrap();
    mock_state_update.assert();
    let expected_state_update: StateUpdate = serde_json::from_str(&raw_state_update).unwrap();
    assert_eq!(state_update.unwrap(), expected_state_update);

    let body = r#"{"code": "StarknetErrorCode.BLOCK_NOT_FOUND", "message": "Block number -1 was not found."}"#;
    let mock_no_block =
        mock("GET", &format!("/feeder_gateway/get_state_update?{BLOCK_NUMBER_QUERY}=999999")[..])
            .with_status(400)
            .with_body(body)
            .create();
    let state_update = starknet_client.state_update(BlockNumber(999999)).await.unwrap();
    assert!(state_update.is_none());
    mock_no_block.assert();
}

#[tokio::test]
async fn serialization_precision() {
    let input =
        "{\"value\":244116128358498188146337218061232635775543270890529169229936851982759783745}";
    let serialized = serde_json::from_str::<serde_json::Value>(input).unwrap();
    let deserialized = serde_json::to_string(&serialized).unwrap();
    assert_eq!(input, deserialized);
}

#[tokio::test]
async fn contract_class() {
    let starknet_client = StarknetFeederGatewayClient::new(
        &mockito::server_url(),
        None,
        NODE_VERSION,
        get_test_config(),
    )
    .unwrap();
    let expected_contract_class = ContractClass {
        sierra_program: vec![
            felt!("0x302e312e30"),
            felt!("0x1c"),
            felt!("0x52616e6765436865636b"),
        ],
        entry_points_by_type: HashMap::from([(
            EntryPointType::External,
            vec! [EntryPoint {
                function_idx: FunctionIndex(0),
                selector: EntryPointSelector(felt!(
                    "0x22ff5f21f0b81b113e63f7db6da94fedef11b2119b4088b89664fb9a3cb658"
                )),
            }],
        ),
        (EntryPointType::Constructor, vec![]),
        (EntryPointType::L1Handler, vec![]),
        ]),
        contract_class_version: String::from("0.1.0"),
        abi: String::from("[\n  {\n    \"type\": \"function\",\n    \"name\": \"test\",\n    \"inputs\": [\n      {\n        \"name\": \"arg\",\n        \"ty\": \"core::felt\"\n      },\n      {\n        \"name\": \"arg1\",\n        \"ty\": \"core::felt\"\n      },\n      {\n        \"name\": \"arg2\",\n        \"ty\": \"core::felt\"\n      }\n    ],\n    \"output_ty\": \"core::felt\",\n    \"state_mutability\": \"external\"\n  },\n  {\n    \"type\": \"function\",\n    \"name\": \"empty\",\n    \"inputs\": [],\n    \"output_ty\": \"()\",\n    \"state_mutability\": \"external\"\n  },\n  {\n    \"type\": \"function\",\n    \"name\": \"call_foo\",\n    \"inputs\": [\n      {\n        \"name\": \"a\",\n        \"ty\": \"core::integer::u128\"\n      }\n    ],\n    \"output_ty\": \"core::integer::u128\",\n    \"state_mutability\": \"external\"\n  }\n]"),
    };

    let mock_by_hash =
        mock(
            "GET",
            &format!("/feeder_gateway/get_class_by_hash?blockNumber=pending&\
         {CLASS_HASH_QUERY}=0x4e70b19333ae94bd958625f7b61ce9eec631653597e68645e13780061b2136c")[..],
        )
        .with_status(200)
        .with_body(read_resource_file("reader/contract_class.json"))
        .create();
    let contract_class = starknet_client
        .class_by_hash(ClassHash(felt!(
            "0x4e70b19333ae94bd958625f7b61ce9eec631653597e68645e13780061b2136c"
        )))
        .await
        .unwrap()
        .unwrap();

    let contract_class = match contract_class {
        GenericContractClass::Cairo1ContractClass(class) => class,
        _ => unreachable!("Expecting Cairo0ContractClass."),
    };
    mock_by_hash.assert();
    assert_eq!(contract_class, expected_contract_class);
}

#[tokio::test]
async fn deprecated_contract_class() {
    let starknet_client = StarknetFeederGatewayClient::new(
        &mockito::server_url(),
        None,
        NODE_VERSION,
        get_test_config(),
    )
    .unwrap();
    let expected_contract_class = DeprecatedContractClass {
        abi: Some(vec![ContractClassAbiEntry::Constructor(FunctionAbiEntry::<ConstructorType> {
            name: "constructor".to_string(),
            inputs: vec![TypedParameter {
                name: "implementation".to_string(),
                r#type: "felt".to_string(),
            }],
            outputs: vec![],
            state_mutability: None,
            r#type: ConstructorType::Constructor,
        })]),
        program: Program {
            attributes: serde_json::Value::Array(vec![serde_json::json!(1234)]),
            builtins: serde_json::Value::Array(Vec::new()),
            compiler_version: serde_json::Value::Null,
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
            (DeprecatedEntryPointType::L1Handler, vec![]),
            (
                DeprecatedEntryPointType::Constructor,
                vec![DeprecatedEntryPoint {
                    selector: EntryPointSelector(felt!(
                        "0x028ffe4ff0f226a9107253e17a904099aa4f63a02a5621de0576e5aa71bc5194"
                    )),
                    offset: EntryPointOffset(62),
                }],
            ),
            (
                DeprecatedEntryPointType::External,
                vec![DeprecatedEntryPoint {
                    selector: EntryPointSelector(felt!(
                        "0x0000000000000000000000000000000000000000000000000000000000000000"
                    )),
                    offset: EntryPointOffset(86),
                }],
            ),
        ]),
    };
    let mock_by_hash =
        mock(
            "GET",
            &format!("/feeder_gateway/get_class_by_hash?blockNumber=pending&\
         {CLASS_HASH_QUERY}=0x7af612493193c771c1b12f511a8b4d3b0c6d0648242af4680c7cd0d06186f17")[..],
        )
        .with_status(200)
        .with_body(read_resource_file("reader/deprecated_contract_class.json"))
        .create();
    let contract_class = starknet_client
        .class_by_hash(ClassHash(felt!(
            "0x7af612493193c771c1b12f511a8b4d3b0c6d0648242af4680c7cd0d06186f17"
        )))
        .await
        .unwrap()
        .unwrap();
    let contract_class = match contract_class {
        GenericContractClass::Cairo0ContractClass(class) => class,
        _ => unreachable!("Expecting deprecated contract class."),
    };
    mock_by_hash.assert();
    assert_eq!(contract_class, expected_contract_class);

    // Undeclared class.
    let body = r#"{"code": "StarknetErrorCode.UNDECLARED_CLASS", "message": "Class with hash 0x7 is not declared."}"#;
    let mock_by_hash =
        mock(
            "GET",
            &format!(
                "/feeder_gateway/get_class_by_hash?blockNumber=pending&{CLASS_HASH_QUERY}=0x7"
            )[..],
        )
        .with_status(400)
        .with_body(body)
        .create();
    let class = starknet_client.class_by_hash(ClassHash(felt!("0x7"))).await.unwrap();
    mock_by_hash.assert();
    assert!(class.is_none());
}

// TODO: Add test for pending_data.

#[tokio::test]
async fn deprecated_pending_data() {
    let starknet_client = StarknetFeederGatewayClient::new(
        &mockito::server_url(),
        None,
        NODE_VERSION,
        get_test_config(),
    )
    .unwrap();

    // Pending
    let raw_pending_data = read_resource_file("reader/deprecated_pending_data.json");
    let mock_pending =
        mock("GET", "/feeder_gateway/get_state_update?blockNumber=pending&includeBlock=true")
            .with_status(200)
            .with_body(&raw_pending_data)
            .create();
    let pending_data = starknet_client.pending_data().await;
    mock_pending.assert();
    let expected_pending_data: PendingData = serde_json::from_str(&raw_pending_data).unwrap();
    assert_eq!(pending_data.unwrap().unwrap(), expected_pending_data);

    // Accepted on L2.
    let raw_pending_data = read_resource_file("reader/accepted_on_l2_deprecated_data.json");
    let mock_accepted =
        mock("GET", "/feeder_gateway/get_state_update?blockNumber=pending&includeBlock=true")
            .with_status(200)
            .with_body(&raw_pending_data)
            .create();
    let pending_data = starknet_client.pending_data().await;
    mock_accepted.assert();
    let expected_pending_data: PendingData = serde_json::from_str(&raw_pending_data).unwrap();
    assert_eq!(pending_data.unwrap().unwrap(), expected_pending_data);
}

#[tokio::test]
async fn get_block() {
    let starknet_client = StarknetFeederGatewayClient::new(
        &mockito::server_url(),
        None,
        NODE_VERSION,
        get_test_config(),
    )
    .unwrap();
    let raw_block = read_resource_file("reader/block_post_0_13_1.json");
    let mock_block = mock("GET", &format!("/feeder_gateway/get_block?{BLOCK_NUMBER_QUERY}=20")[..])
        .with_status(200)
        .with_body(&raw_block)
        .create();
    let block = starknet_client.block(BlockNumber(20)).await.unwrap().unwrap();
    mock_block.assert();
    let expected_block: Block = serde_json::from_str(&raw_block).unwrap();
    assert_eq!(block, expected_block);

    // Non-existing block.
    let body = r#"{"code": "StarknetErrorCode.BLOCK_NOT_FOUND", "message": "Block 9999999999 was not found."}"#;
    let mock_no_block =
        mock("GET", &format!("/feeder_gateway/get_block?{BLOCK_NUMBER_QUERY}=9999999999")[..])
            .with_status(400)
            .with_body(body)
            .create();
    let block = starknet_client.block(BlockNumber(9999999999)).await.unwrap();
    mock_no_block.assert();
    assert!(block.is_none());
}

#[tokio::test]
async fn compiled_class_by_hash() {
    let starknet_client = StarknetFeederGatewayClient::new(
        &mockito::server_url(),
        None,
        NODE_VERSION,
        get_test_config(),
    )
    .unwrap();
    let raw_casm_contract_class = read_resource_file("reader/casm_contract_class.json");
    let mock_casm_contract_class = mock(
        "GET",
        &format!(
            "/feeder_gateway/get_compiled_class_by_class_hash?blockNumber=pending&\
             {CLASS_HASH_QUERY}=0x7"
        )[..],
    )
    .with_status(200)
    .with_body(&raw_casm_contract_class)
    .create();
    let casm_contract_class =
        starknet_client.compiled_class_by_hash(ClassHash(felt!("0x7"))).await.unwrap().unwrap();
    mock_casm_contract_class.assert();
    let expected_casm_contract_class: CasmContractClass =
        serde_json::from_str(&raw_casm_contract_class).unwrap();
    assert_eq!(casm_contract_class, expected_casm_contract_class);

    let body = r#"{"code": "StarknetErrorCode.UNDECLARED_CLASS", "message": "Class with hash 0x7 is not declared."}"#;
    let mock_undeclared = mock(
        "GET",
        &format!(
            "/feeder_gateway/get_compiled_class_by_class_hash?blockNumber=pending&\
             {CLASS_HASH_QUERY}=0x0"
        )[..],
    )
    .with_status(400)
    .with_body(body)
    .create();
    let class = starknet_client.compiled_class_by_hash(ClassHash(felt!("0x0"))).await.unwrap();
    mock_undeclared.assert();
    assert!(class.is_none());
}

#[tokio::test]
async fn is_alive() {
    let starknet_client = StarknetFeederGatewayClient::new(
        &mockito::server_url(),
        None,
        NODE_VERSION,
        get_test_config(),
    )
    .unwrap();
    let mock_is_alive = mock("GET", "/feeder_gateway/is_alive")
        .with_status(200)
        .with_body(FEEDER_GATEWAY_ALIVE_RESPONSE)
        .create();
    let response = starknet_client.is_alive().await;
    mock_is_alive.assert();
    assert!(response);
}

// Empty storage diffs were filtered out in the past, but should not anymore (part of the inputs to
// the state diff commitment).
#[tokio::test]
async fn state_update_with_empty_storage_diff() {
    let starknet_client = StarknetFeederGatewayClient::new(
        &mockito::server_url(),
        None,
        NODE_VERSION,
        get_test_config(),
    )
    .unwrap();
    let mut state_update = StateUpdate::default();
    let empty_storage_diff = indexmap!(ContractAddress::default() => vec![]);
    state_update.state_diff.storage_diffs.clone_from(&empty_storage_diff);

    let mock =
        mock("GET", &format!("/feeder_gateway/get_state_update?{BLOCK_NUMBER_QUERY}=123456")[..])
            .with_status(200)
            .with_body(serde_json::to_string(&state_update).unwrap())
            .create();
    let state_update = starknet_client.state_update(BlockNumber(123456)).await.unwrap().unwrap();
    mock.assert();
    assert_eq!(state_update.state_diff.storage_diffs, empty_storage_diff);
}

async fn test_unserializable<
    Output: Send + Debug,
    Fut: Future<Output = ReaderClientResult<Output>>,
    F: FnOnce(StarknetFeederGatewayClient) -> Fut,
>(
    url_suffix: &str,
    call_method: F,
) {
    let starknet_client = StarknetFeederGatewayClient::new(
        &mockito::server_url(),
        None,
        NODE_VERSION,
        get_test_config(),
    )
    .unwrap();
    let body = "body";
    let mock = mock("GET", url_suffix).with_status(200).with_body(body).create();
    let error = call_method(starknet_client).await.unwrap_err();
    mock.assert();
    assert_matches!(error, ReaderClientError::SerdeError(_));
}

#[tokio::test]
async fn latest_block_unserializable() {
    test_unserializable(
        "/feeder_gateway/get_block?blockNumber=latest",
        |starknet_client| async move { starknet_client.latest_block().await },
    )
    .await
}

#[tokio::test]
async fn block_unserializable() {
    test_unserializable("/feeder_gateway/get_block?blockNumber=20", |starknet_client| async move {
        starknet_client.block(BlockNumber(20)).await
    })
    .await
}

#[tokio::test]
async fn class_by_hash_unserializable() {
    test_unserializable(
        &format!("/feeder_gateway/get_class_by_hash?blockNumber=pending&{CLASS_HASH_QUERY}=0x1")[..],
        |starknet_client| async move {
            starknet_client.class_by_hash(ClassHash(felt!("0x1"))).await
        },
    )
    .await
}

#[tokio::test]
async fn state_update_unserializable() {
    test_unserializable(
        &format!("/feeder_gateway/get_state_update?{BLOCK_NUMBER_QUERY}=123456")[..],
        |starknet_client| async move { starknet_client.state_update(BlockNumber(123456)).await },
    )
    .await
}

#[tokio::test]
async fn compiled_class_by_hash_unserializable() {
    test_unserializable(
        &format!(
            "/feeder_gateway/get_compiled_class_by_class_hash?blockNumber=pending&\
             {CLASS_HASH_QUERY}=0x7"
        )[..],
        |starknet_client| async move {
            starknet_client.compiled_class_by_hash(ClassHash(felt!("0x7"))).await
        },
    )
    .await
}

#[tokio::test]
async fn get_block_signature() {
    let starknet_client = StarknetFeederGatewayClient::new(
        &mockito::server_url(),
        None,
        NODE_VERSION,
        get_test_config(),
    )
    .unwrap();

    let expected_block_signature = BlockSignatureData::Deprecated {
        block_number: BlockNumber(20),
        signature: [felt!("0x1"), felt!("0x2")],
        signature_input: BlockSignatureMessage {
            block_hash: BlockHash(felt!("0x20")),
            state_diff_commitment: GlobalRoot(felt!("0x1234")),
        },
    };

    let mock_block_signature =
        mock("GET", &format!("/feeder_gateway/get_signature?{BLOCK_NUMBER_QUERY}=20")[..])
            .with_status(200)
            .with_body(serde_json::to_string(&expected_block_signature).unwrap())
            .create();

    let block_signature = starknet_client.block_signature(BlockNumber(20)).await.unwrap().unwrap();
    mock_block_signature.assert();
    assert_eq!(block_signature, expected_block_signature);
}

#[tokio::test]
async fn get_block_signature_unknown_block() {
    let starknet_client = StarknetFeederGatewayClient::new(
        &mockito::server_url(),
        None,
        NODE_VERSION,
        get_test_config(),
    )
    .unwrap();

    let body = r#"{"code": "StarknetErrorCode.BLOCK_NOT_FOUND", "message": "Block number 999999 was not found."}"#;
    let mock_no_block =
        mock("GET", &format!("/feeder_gateway/get_signature?{BLOCK_NUMBER_QUERY}=999999")[..])
            .with_status(400)
            .with_body(body)
            .create();
    let block_signature = starknet_client.block_signature(BlockNumber(999999)).await.unwrap();
    mock_no_block.assert();
    assert!(block_signature.is_none());
}

#[tokio::test]
async fn get_sequencer_public_key() {
    let starknet_client = StarknetFeederGatewayClient::new(
        &mockito::server_url(),
        None,
        NODE_VERSION,
        get_test_config(),
    )
    .unwrap();

    let expected_sequencer_pub_key = SequencerPublicKey(PublicKey(felt!("0x1")));

    let mock_key = mock("GET", "/feeder_gateway/get_public_key")
        .with_status(200)
        .with_body(serde_json::to_string(&expected_sequencer_pub_key).unwrap())
        .create();

    let pub_key = starknet_client.sequencer_pub_key().await.unwrap();
    mock_key.assert();
    assert_eq!(pub_key, expected_sequencer_pub_key);
}
