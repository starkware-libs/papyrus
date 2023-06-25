use std::panic;

use assert_matches::assert_matches;
use camelpaste::paste;
use futures_util::future::join_all;
use hyper::{header, Body, Request};
use jsonrpsee::core::client::ClientT;
use jsonrpsee::core::http_helpers::read_body;
use jsonrpsee::core::{Error, RpcResult};
use jsonrpsee::http_client::HttpClientBuilder;
use jsonrpsee::types::ErrorObjectOwned;
use papyrus_storage::base_layer::BaseLayerStorageWriter;
use papyrus_storage::header::HeaderStorageWriter;
use papyrus_storage::test_utils::get_test_storage;
use starknet_api::block::{BlockHash, BlockHeader, BlockNumber, BlockStatus};
use starknet_api::deprecated_contract_class::{
    EventAbiEntry, FunctionAbiEntryWithType, StructAbiEntry,
};
use test_utils::{get_rng, GetTestInstance};
use tower::BoxError;

use crate::api::version_config::{LATEST_VERSION_ID, VERSION_CONFIG};
use crate::api::JsonRpcError;
use crate::deprecated_contract_class::ContractClassAbiEntryType;
use crate::middleware::proxy_request;
use crate::test_utils::get_test_gateway_config;
use crate::{get_block_status, run_server, SERVER_MAX_BODY_SIZE};

#[tokio::test]
async fn run_server_no_blocks() {
    let ((storage_reader, _), _temp_dir) = get_test_storage();
    let gateway_config = get_test_gateway_config();
    let (addr, _handle) = run_server(&gateway_config, storage_reader).await.unwrap();
    let client = HttpClientBuilder::default().build(format!("http://{addr:?}")).unwrap();
    let res: Result<RpcResult<BlockNumber>, Error> =
        client.request("starknet_blockNumber", [""]).await;
    let _expected_error = ErrorObjectOwned::from(JsonRpcError::NoBlocks);
    match res {
        Err(err) => assert_matches!(err, _expected_error),
        Ok(_) => panic!("should error with no blocks"),
    };
}

/// Given an HTTP request, using the "read_body" function from jsonrpsee library,
/// parse the body, make sure it's a formatted JSON and within the MAX_BODY_SIZE length.
async fn get_json_rpc_body(request: Request<Body>) -> Vec<u8> {
    let (res_parts, res_body) = request.into_parts();
    let (body_bytes, is_single) =
        read_body(&res_parts.headers, res_body, SERVER_MAX_BODY_SIZE).await.unwrap();
    assert!(is_single);
    body_bytes
}

async fn call_proxy_request_get_method_in_out(uri: String) -> Result<(String, String), BoxError> {
    let method_name = "myMethod";
    let params = serde_json::from_str(r#"[{"myParam": "myValue"}]"#).unwrap();
    let request_body = jsonrpsee::types::Request::new(
        format!("starknet_{method_name}").into(),
        Some(params),
        jsonrpsee::types::Id::Number(0),
    );
    let req_no_version = Request::post(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();
    match proxy_request(req_no_version).await {
        Ok(res) => {
            let get_json_rpc_body = get_json_rpc_body(res).await;
            let body = serde_json::from_slice::<jsonrpsee::types::Request<'_>>(&get_json_rpc_body)
                .unwrap();
            // assert params not altered by proxy middleware
            assert_eq!(params.to_string(), body.params.unwrap().to_string());
            Ok((method_name.to_string(), body.method.to_string()))
        }
        Err(err) => Err(err),
    }
}

// TODO: nevo - add middleware negative cases tests

#[tokio::test]
async fn test_version_middleware() {
    let base_uri = "http://localhost:8080";
    let latest_version = LATEST_VERSION_ID.to_string();
    let mut path_options =
        vec![("".to_string(), latest_version.clone()), ("/".to_string(), latest_version.clone())];
    VERSION_CONFIG.iter().for_each(|(version_id, _)| {
        path_options.push((format!("/{}", *version_id), (*version_id).to_string()))
    });
    let mut handles = Vec::new();
    for (path, expected_version) in path_options {
        let future = async move {
            let uri = format!("{base_uri}{path}");
            let (in_method, out_method) = call_proxy_request_get_method_in_out(uri).await.unwrap();
            {
                assert_eq!(format!("starknet_{expected_version}_{in_method}"), out_method);
            };
        };
        let handle = tokio::spawn(future);
        handles.push(handle);
    }
    let _res = join_all(handles).await;
    let unknown_version = "not_a_valid_version";
    let bad_uri = format!("{base_uri}/{unknown_version}");
    if let Ok(res) = call_proxy_request_get_method_in_out(bad_uri).await {
        panic!("expected failure got: {res:?}");
    };
}

#[test]
fn get_block_status_test() {
    let (reader, mut writer) = get_test_storage().0;

    for block_number in 0..2 {
        let header = BlockHeader {
            block_number: BlockNumber(block_number),
            block_hash: BlockHash(block_number.into()),
            ..Default::default()
        };
        writer
            .begin_rw_txn()
            .unwrap()
            .append_header(header.block_number, &header)
            .unwrap()
            .commit()
            .unwrap();
    }

    // update the base_layer_tip_marker to BlockNumber(1).
    writer
        .begin_rw_txn()
        .unwrap()
        .update_base_layer_tip_marker(&BlockNumber(1))
        .unwrap()
        .commit()
        .unwrap();

    let txn = reader.begin_ro_txn().unwrap();
    assert_eq!(get_block_status(&txn, BlockNumber(0)).unwrap(), BlockStatus::AcceptedOnL1);
    assert_eq!(get_block_status(&txn, BlockNumber(1)).unwrap(), BlockStatus::AcceptedOnL2);
    assert_eq!(get_block_status(&txn, BlockNumber(2)).unwrap(), BlockStatus::AcceptedOnL2);

#[tokio::test]
async fn test_contractclassabientrywithtype_from_api_contractclassabientry() {
    let mut rng = get_rng();
    let _: ContractClassAbiEntryWithType =
        starknet_api::deprecated_contract_class::ContractClassAbiEntry::Event(
            EventAbiEntry::get_test_instance(&mut rng),
        )
        .try_into()
        .unwrap();
    let _: ContractClassAbiEntryWithType =
        starknet_api::deprecated_contract_class::ContractClassAbiEntry::Function(
            FunctionAbiEntryWithType::get_test_instance(&mut rng),
        )
        .try_into()
        .unwrap();
    let _: ContractClassAbiEntryWithType =
        starknet_api::deprecated_contract_class::ContractClassAbiEntry::Struct(
            StructAbiEntry::get_test_instance(&mut rng),
        )
        .try_into()
        .unwrap();

macro_rules! test_ContractClassAbiEntryType_from_FunctionAbiEntryType {
    ($variant:ident) => {
        paste! {
            #[tokio::test]
            #[allow(non_snake_case)]
            async fn [< ContractClassAbiEntryType_from_FunctionAbiEntryType_ $variant:lower>]() {
                let _: ContractClassAbiEntryType =
                starknet_api::deprecated_contract_class::FunctionAbiEntryType::$variant
                    .try_into()
                    .unwrap();
            }
        }
    };
}
test_ContractClassAbiEntryType_from_FunctionAbiEntryType!(Constructor);
test_ContractClassAbiEntryType_from_FunctionAbiEntryType!(L1Handler);
test_ContractClassAbiEntryType_from_FunctionAbiEntryType!(Function);

// macro to generate a test that creates a ContractClassAbiEntry with a variant based on the given
// variant input and call try_into().unwrap()
macro_rules! test_contract_class_abi_entry_with_type {
    ($variant:ident, $variant_inner:ident) => {
        paste! {
            #[tokio::test]
            #[allow(non_snake_case)]
            async fn [<ContractClassAbiEntryWithType_from_api_ContractClassAbiEntry_ $variant:lower>]() {
                let mut rng = get_rng();
                let _: ContractClassAbiEntryWithType =
                    starknet_api::deprecated_contract_class::ContractClassAbiEntry::$variant(
                        $variant_inner::get_test_instance(&mut rng),
                    )
                    .try_into()
                    .unwrap();
            }
        }
    };
}

test_contract_class_abi_entry_with_type!(Event, EventAbiEntry);
test_contract_class_abi_entry_with_type!(Function, FunctionAbiEntryWithType);
test_contract_class_abi_entry_with_type!(Struct, StructAbiEntry);
