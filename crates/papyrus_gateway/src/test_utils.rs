use std::{net::SocketAddr, sync::Arc};

use jsonrpsee::http_server::RpcModule;
use jsonschema::JSONSchema;
use papyrus_storage::test_utils::get_test_storage;
use papyrus_storage::StorageWriter;
use starknet_api::core::ChainId;
use starknet_client::{RetryConfig, StarknetClient};
use reqwest::Client;

use crate::{GatewayConfig, JsonRpcServer, JsonRpcServerImpl, CentralSourceConfig};

// TODO(anatg): See if this can be usefull for the benchmark testing as well.
pub async fn send_request(address: SocketAddr, method: &str, params: &str) -> serde_json::Value {
    let client = Client::new();
    let res_str = client
        .post(format!("http://{address:?}"))
        .header("Content-Type", "application/json")
        .body(format!(r#"{{"jsonrpc":"2.0","id":"1","method":"{method}","params":[{params}]}}"#))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    serde_json::from_str(&res_str).unwrap()
}

pub fn get_test_gateway_config() -> GatewayConfig {
    GatewayConfig {
        chain_id: ChainId("SN_GOERLI".to_string()),
        server_address: String::from("127.0.0.1:0"),
        max_events_chunk_size: 10,
        max_events_keys: 10,
    }
}

pub fn get_test_starknet_source_config() -> CentralSourceConfig{
    CentralSourceConfig{
        url: String::from("http://127.0.0.1:1"),
        http_headers: Option::None,
        retry_config: RetryConfig{
            retry_base_millis: 0,
            retry_max_delay_millis: 0,
            max_retries: 0,
        },
    }
}

pub(crate) fn get_test_rpc_server_and_storage_writer()
-> (RpcModule<JsonRpcServerImpl>, StorageWriter) {
    let (storage_reader, storage_writer) = get_test_storage();
    let config = get_test_gateway_config();
    let starknet_source_config = get_test_starknet_source_config();
    (
        JsonRpcServerImpl {
            chain_id: config.chain_id,
            storage_reader,
            max_events_chunk_size: config.max_events_chunk_size,
            max_events_keys: config.max_events_keys,
            starknet_source: Arc::new(
                StarknetClient::new(
                    &starknet_source_config.url, 
                    starknet_source_config.http_headers, 
                    starknet_source_config.retry_config
                ).unwrap()
            )
        }
        .into_rpc(),
        storage_writer,
    )
}

pub async fn get_starknet_spec_api_schema(component_names: &[&str]) -> JSONSchema {
    let target = "./resources/starknet_api_openrpc.json";
    let text = std::fs::read_to_string(target).unwrap();
    let spec: serde_json::Value = serde_json::from_str(&text).unwrap();

    let mut components = String::from(r#"{"anyOf": ["#);
    for component in component_names {
        components +=
            &format!(r##"{{"$ref": "file:///spec.json#/components/schemas/{component}"}}"##);
        if Some(component) != component_names.last() {
            components += ", ";
        }
    }
    components += r#"], "unevaluatedProperties": false}"#;
    let schema = serde_json::from_str(&components).unwrap();

    JSONSchema::options()
        .with_document("file:///spec.json".to_owned(), spec)
        .compile(&schema)
        .unwrap()
}
