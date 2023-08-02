use std::sync::Arc;

use derive_more::Display;
use jsonrpsee::server::RpcModule;
use jsonschema::JSONSchema;
use papyrus_common::SyncingState;
use papyrus_storage::test_utils::get_test_storage;
use papyrus_storage::StorageWriter;
use serde_json::Value;
use starknet_api::core::ChainId;
use tokio::sync::RwLock;

use crate::api::JsonRpcServerImpl;
use crate::version_config::VersionId;
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

pub(crate) fn get_test_syncing_state() -> Arc<RwLock<SyncingState>> {
    Arc::new(RwLock::new(SyncingState::default()))
}

pub(crate) fn get_test_rpc_server_and_storage_writer<T: JsonRpcServerImpl>()
-> (RpcModule<T>, StorageWriter) {
    let ((storage_reader, storage_writer), _temp_dir) = get_test_storage();
    let config = get_test_gateway_config();
    let shared_syncing_state = get_test_syncing_state();
    (
        T::new(
            config.chain_id,
            storage_reader,
            config.max_events_chunk_size,
            config.max_events_keys,
            shared_syncing_state,
        )
        .into_rpc_module(),
        storage_writer,
    )
}

// TODO(nevo): Schmea validates null as valid for an unknown reason.
// Investigate in the future and remove this function (use is_valid directly)
pub fn validate_schema(schema: &JSONSchema, res: Value) -> bool {
    let result = &res["result"];
    result != &Value::Null && schema.is_valid(result)
}

#[derive(Clone, Copy, Display)]
pub enum SpecFile {
    #[display(fmt = "starknet_api_openrpc.json")]
    StarknetApiOpenrpc,
    // TODO(shahak): Remove allow(dead_code) once we use this variant.
    #[allow(dead_code)]
    #[display(fmt = "starknet_write_api.json")]
    StarknetWriteApi,
}


pub async fn get_starknet_spec_api_schema(
    file_to_component_names: &[(SpecFile, &[&str])],
    version_id: &VersionId,
) -> JSONSchema {
    let mut options = JSONSchema::options();
    for entry in std::fs::read_dir(format!("./resources/{version_id}")).unwrap() {
        let path = entry.unwrap().path();
        let spec_str = std::fs::read_to_string(path.clone()).unwrap();
        let spec: serde_json::Value = serde_json::from_str(&spec_str).unwrap();
        let file_name = path.file_name().unwrap().to_str().unwrap();
        options.with_document(format!("file:///api/{file_name}"), spec);
    }

    let mut components = String::from(r#"{"anyOf": ["#);
    const SEPARATOR: &str = ", ";
    for (file_name, component_names) in file_to_component_names {
        for component in *component_names {
            components += &format!(
                r##"{{"$ref": "file:///api/{file_name}#/components/schemas/{component}"}}"##,
            );
            components += SEPARATOR;
        }
    }
    // Remove the last separator.
    components.truncate(components.len() - SEPARATOR.len());
    components += r#"], "unevaluatedProperties": false}"#;
    let schema = serde_json::from_str(&components).unwrap();

    options.compile(&schema).unwrap()
}
