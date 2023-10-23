use std::error::Error as StdError;
use std::{panic, vec};

use assert_matches::assert_matches;
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
use pretty_assertions::assert_eq;
use rand::seq::SliceRandom;
use starknet_api::block::{BlockHash, BlockHeader, BlockNumber, BlockStatus};
use test_utils::get_rng;
use tower::BoxError;

use crate::middleware::proxy_rpc_request;
use crate::test_utils::{
    get_test_highest_block,
    get_test_pending_classes,
    get_test_pending_data,
    get_test_rpc_config,
};
use crate::version_config::VERSION_CONFIG;
use crate::{get_block_status, run_server, SERVER_MAX_BODY_SIZE};

#[tokio::test]
async fn run_server_no_blocks() {
    let ((storage_reader, _), _temp_dir) = get_test_storage();
    let gateway_config = get_test_rpc_config();
    let shared_highest_block = get_test_highest_block();
    let pending_data = get_test_pending_data();
    let pending_classes = get_test_pending_classes();
    let (addr, _handle) = run_server(
        &gateway_config,
        shared_highest_block,
        pending_data,
        pending_classes,
        storage_reader,
        "NODE VERSION",
    )
    .await
    .unwrap();
    let client = HttpClientBuilder::default().build(format!("http://{addr:?}")).unwrap();
    let res: Result<RpcResult<BlockNumber>, Error> =
        client.request("starknet_blockNumber", [""]).await;
    let _expected_error = ErrorObjectOwned::owned(32, "There are no blocks", None::<u8>);
    match res {
        Err(err) => assert_matches!(err, _expected_error),
        Ok(_) => panic!("should error with no blocks"),
    };
}

/// Given an HTTP request, using the "read_body" function from jsonrpsee library,
/// parse the body, make sure it's a formatted JSON and within the MAX_BODY_SIZE length.
async fn get_json_rpc_body(request: Request<Body>) -> Vec<u8> {
    let (res_parts, res_body) = request.into_parts();
    let (body_bytes, _is_single) =
        read_body(&res_parts.headers, res_body, SERVER_MAX_BODY_SIZE).await.unwrap();
    body_bytes
}

async fn call_proxy_request_get_method_in_out(
    uri: String,
    is_batch_request: bool,
) -> Result<(String, String), BoxError> {
    let method_name = "myMethod";
    let params = serde_json::from_str(r#"[{"myParam": "myValue"}]"#).unwrap();
    let request_body = get_request_body(is_batch_request, params, method_name);
    let req_no_version = Request::post(uri.clone())
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(request_body.unwrap()))
        .unwrap();
    let res = proxy_rpc_request(req_no_version).await?;
    let body_bytes = get_json_rpc_body(res).await;
    digest_body_and_assert(is_batch_request, body_bytes, params, method_name)
}

fn digest_body_and_assert(
    is_batch_request: bool,
    body_bytes: Vec<u8>,
    params: &serde_json::value::RawValue,
    method_name: &str,
) -> Result<(String, String), Box<dyn StdError + Send + Sync>> {
    match is_batch_request {
        false => {
            let body =
                serde_json::from_slice::<jsonrpsee::types::Request<'_>>(&body_bytes).unwrap();
            // assert params not altered by proxy middleware
            assert_eq!(params.to_string(), body.params.unwrap().to_string());
            Ok((method_name.to_string(), body.method.to_string()))
        }
        true => {
            let body_batch =
                serde_json::from_slice::<Vec<jsonrpsee::types::Request<'_>>>(&body_bytes).unwrap();
            // assert params not altered by proxy middleware for all requests in batch
            body_batch.iter().for_each(|body| {
                assert_eq!(params.to_string(), body.params.unwrap().to_string());
            });
            Ok((method_name.to_string(), body_batch[0].method.to_string()))
        }
    }
}

fn get_request_body(
    is_batch_request: bool,
    params: &serde_json::value::RawValue,
    method_name: &str,
) -> Result<String, serde_json::Error> {
    match is_batch_request {
        false => serde_json::to_string(&jsonrpsee::types::Request::new(
            format!("starknet_{method_name}").into(),
            Some(params),
            jsonrpsee::types::Id::Number(0),
        )),
        true => serde_json::to_string(&vec![
            jsonrpsee::types::Request::new(
                format!("starknet_{method_name}_1").into(),
                Some(params),
                jsonrpsee::types::Id::Number(0),
            ),
            jsonrpsee::types::Request::new(
                format!("starknet_{method_name}_2").into(),
                Some(params),
                jsonrpsee::types::Id::Number(0),
            ),
        ]),
    }
}

// TODO: nevo - add middleware negative cases tests

#[tokio::test]
async fn test_version_middleware() {
    let base_uri = "http://localhost:8080/rpc/";
    let mut path_options = vec![];
    VERSION_CONFIG.iter().for_each(|(version_id, _)| {
        // add version name with capital V
        path_options.push((version_id.name.to_string(), version_id.name.to_string()));
        // add version name with lower case v
        path_options.push((version_id.name.to_lowercase(), version_id.name.to_string()));
        // add version name with patch version
        path_options.push((version_id.to_string(), version_id.name.to_string()));
    });

    // test all versions with single and batch requests
    let mut handles = Vec::new();
    for (path, expected_version) in path_options {
        let future = async move {
            let uri = format!("{base_uri}{path}");
            let (in_method, out_method) =
                call_proxy_request_get_method_in_out(uri.clone(), false).await.unwrap();
            {
                assert_eq!(format!("starknet_{expected_version}_{in_method}"), out_method);
            };
            let (in_method, out_method) =
                call_proxy_request_get_method_in_out(uri, true).await.unwrap();
            {
                assert_eq!(format!("starknet_{expected_version}_{in_method}"), out_method);
            };
        };
        let handle = tokio::spawn(future);
        handles.push(handle);
    }
    let join_res = join_all(handles).await;
    join_res.into_iter().for_each(|res| {
        if let Err(err) = res {
            panic!("expected success got: {err}");
        }
    });
    let unknown_version = "not_a_valid_version";
    let bad_uri = format!("{base_uri}{unknown_version}");
    if let Ok(res) = call_proxy_request_get_method_in_out(bad_uri, false).await {
        panic!("expected failure got: {res:?}");
    };
    let mut rng = get_rng();
    let version_id = VERSION_CONFIG.choose(&mut rng).unwrap().0;
    let newer_version_then_we_have = format!("{}_{}", version_id.name, version_id.patch + 1);
    let bad_uri = format!("{base_uri}{newer_version_then_we_have}");
    if let Ok(res) = call_proxy_request_get_method_in_out(bad_uri, false).await {
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
        .update_base_layer_block_marker(&BlockNumber(1))
        .unwrap()
        .commit()
        .unwrap();

    let txn = reader.begin_ro_txn().unwrap();
    assert_eq!(get_block_status(&txn, BlockNumber(0)).unwrap(), BlockStatus::AcceptedOnL1);
    assert_eq!(get_block_status(&txn, BlockNumber(1)).unwrap(), BlockStatus::AcceptedOnL2);
    assert_eq!(get_block_status(&txn, BlockNumber(2)).unwrap(), BlockStatus::AcceptedOnL2);
}

#[test]
fn serialization_precision() {
    let input =
        "{\"value\":244116128358498188146337218061232635775543270890529169229936851982759783745}";
    let serialized = serde_json::from_str::<serde_json::Value>(input).unwrap();
    let deserialized = serde_json::to_string(&serialized).unwrap();
    assert_eq!(input, deserialized);
}
