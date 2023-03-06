use jsonrpsee::http_server::RpcModule;
use jsonschema::JSONSchema;
use papyrus_storage::test_utils::get_test_storage;
use papyrus_storage::StorageWriter;
use starknet_api::core::ChainId;

use crate::{GatewayConfig, JsonRpcServer, JsonRpcServerImpl};

pub fn get_test_gateway_config() -> GatewayConfig {
    GatewayConfig {
        chain_id: ChainId("SN_GOERLI".to_string()),
        server_address: String::from("127.0.0.1:0"),
        max_events_chunk_size: 10,
        max_events_keys: 10,
    }
}

pub(crate) fn get_test_rpc_server_and_storage_writer()
-> (RpcModule<JsonRpcServerImpl>, StorageWriter) {
    let (storage_reader, storage_writer) = get_test_storage();
    let config = get_test_gateway_config();
    (
        JsonRpcServerImpl {
            chain_id: config.chain_id,
            storage_reader,
            max_events_chunk_size: config.max_events_chunk_size,
            max_events_keys: config.max_events_keys,
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
