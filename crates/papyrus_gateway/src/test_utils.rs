use std::sync::Arc;

use jsonrpsee::server::RpcModule;
use jsonschema::JSONSchema;
use papyrus_common::SyncingState;
use papyrus_storage::test_utils::get_test_storage;
use papyrus_storage::StorageWriter;
use starknet_api::core::ChainId;
use tokio::sync::Mutex;

use crate::api::JsonRpcServerImpl;
use crate::GatewayConfig;

pub fn get_test_gateway_config() -> GatewayConfig {
    GatewayConfig {
        chain_id: ChainId("SN_GOERLI".to_string()),
        server_address: String::from("127.0.0.1:0"),
        max_events_chunk_size: 10,
        max_events_keys: 10,
        collect_metrics: false,
    }
}

pub(crate) fn get_test_sync_status() -> Arc<Mutex<Option<SyncingState>>> {
    Arc::new(Mutex::new(Option::<SyncingState>::None))
}

pub(crate) fn get_test_rpc_server_and_storage_writer<T: JsonRpcServerImpl>()
-> (RpcModule<T>, StorageWriter) {
    let ((storage_reader, storage_writer), _temp_dir) = get_test_storage();
    let config = get_test_gateway_config();
    let shared_sync_status = get_test_sync_status();
    (
        T::new(
            config.chain_id,
            storage_reader,
            config.max_events_chunk_size,
            config.max_events_keys,
            shared_sync_status,
        )
        .into(),
        storage_writer,
    )
}

pub async fn get_starknet_spec_api_schema(
    component_names: &[&str],
    version_id: &str,
) -> JSONSchema {
    let target = format!("./resources/{version_id}_starknet_api_openrpc.json");
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
