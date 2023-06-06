use std::collections::BTreeMap;
use std::panic;

use assert_matches::assert_matches;
use futures_util::future::join_all;
use hyper::{header, Body, Request};
use jsonrpsee::core::client::ClientT;
use jsonrpsee::core::http_helpers::read_body;
use jsonrpsee::core::{Error, RpcResult};
use jsonrpsee::http_client::HttpClientBuilder;
use jsonrpsee::types::ErrorObjectOwned;
use papyrus_config::{SerdeConfig, SerializedParam};
use papyrus_storage::test_utils::get_test_storage;
use serde_json::{Map, Value};
use starknet_api::block::BlockNumber;
use test_utils::get_absolute_path;
use tower::BoxError;

use crate::api::version_config::{LATEST_VERSION_ID, VERSION_CONFIG};
use crate::api::JsonRpcError;
use crate::middleware::proxy_request;
use crate::test_utils::get_test_gateway_config;
use crate::{run_server, GatewayConfig, SERVER_MAX_BODY_SIZE};

const DEFAULT_CONFIG_FILE: &str = "config/default_config.json";

#[tokio::test]
async fn run_server_no_blocks() {
    let (storage_reader, _) = get_test_storage();
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
        format!("starknet_{}", method_name).into(),
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
            let uri = format!("{}{}", base_uri, path);
            let (in_method, out_method) = call_proxy_request_get_method_in_out(uri).await.unwrap();
            {
                assert_eq!(format!("starknet_{}_{}", expected_version, in_method), out_method);
            };
        };
        let handle = tokio::spawn(future);
        handles.push(handle);
    }
    let _res = join_all(handles).await;
    let unknown_version = "not_a_valid_version";
    let bad_uri = format!("{}/{}", base_uri, unknown_version);
    if let Ok(res) = call_proxy_request_get_method_in_out(bad_uri).await {
        panic!("expected failure got: {:?}", res);
    };
}

#[test]
/// Regression test which checks that the default config hasn't changed as well as dumping/parsing
/// configs.
fn test_dump_default_config() {
    let dumped_default_gateway = GatewayConfig::default().dump_sub_config();
    insta::assert_json_snapshot!(dumped_default_gateway);

    let path = get_absolute_path(DEFAULT_CONFIG_FILE);
    let file = std::fs::File::open(path).unwrap();
    let deserialized_default_config: Map<String, Value> = serde_json::from_reader(file).unwrap();

    let mut deserialized_map: BTreeMap<String, SerializedParam> = BTreeMap::new();
    for (key, value) in deserialized_default_config {
        deserialized_map.insert(
            key.to_owned(),
            SerializedParam {
                description: value["description"].as_str().unwrap().to_owned(),
                value: value["value"].to_owned(),
            },
        );
    }

    assert_eq!(deserialized_map, dumped_default_gateway);
}

#[test]
fn test_dump_and_load() {
    let default_gateway = GatewayConfig::default();
    let loaded_sub_config =
        GatewayConfig::load_sub_config(&default_gateway.dump_sub_config()).unwrap();
    assert_eq!(loaded_sub_config, default_gateway);
    let loaded_config = GatewayConfig::load(&default_gateway.dump()).unwrap();
    assert_eq!(loaded_config, default_gateway);
}
