use std::path::Path;
use std::sync::Arc;

use derive_more::Display;
use jsonrpsee::server::RpcModule;
use jsonschema::JSONSchema;
use papyrus_common::BlockHashAndNumber;
use papyrus_storage::test_utils::get_test_storage;
use papyrus_storage::StorageWriter;
use serde_json::Value;
use starknet_api::core::ChainId;
use starknet_client::writer::MockStarknetWriter;
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
        ..Default::default()
    }
}

pub(crate) fn get_test_highest_block() -> Arc<RwLock<Option<BlockHashAndNumber>>> {
    Arc::new(RwLock::new(None))
}

pub(crate) fn get_test_rpc_server_and_storage_writer<T: JsonRpcServerImpl>()
-> (RpcModule<T>, StorageWriter, Arc<MockStarknetWriter>) {
    let ((storage_reader, storage_writer), _temp_dir) = get_test_storage();
    let config = get_test_gateway_config();
    let shared_highest_block = get_test_highest_block();
    let mock_starknet_writer = Arc::new(MockStarknetWriter::new());
    (
        T::new(
            config.chain_id,
            storage_reader,
            config.max_events_chunk_size,
            config.max_events_keys,
            shared_highest_block,
            mock_starknet_writer.clone(),
        )
        .into_rpc_module(),
        storage_writer,
        mock_starknet_writer,
    )
}

// TODO(nevo): Schmea validates null as valid for an unknown reason.
// Investigate in the future and remove this function (use is_valid directly)
pub fn validate_schema(schema: &JSONSchema, result: &Value) -> bool {
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

pub fn get_starknet_spec_api_schema_for_components(
    file_to_component_names: &[(SpecFile, &[&str])],
    version_id: &VersionId,
) -> JSONSchema {
    get_starknet_spec_api_schema(
        file_to_component_names.iter().flat_map(|(file, component_names)| {
            component_names
                .iter()
                .map(move |component| format!("file:///api/{file}#/components/schemas/{component}"))
        }),
        version_id,
    )
}

pub fn get_starknet_spec_api_schema_for_method_results(
    file_to_methods: &[(SpecFile, &[&str])],
    version_id: &VersionId,
) -> JSONSchema {
    get_starknet_spec_api_schema(
        file_to_methods.iter().flat_map(|(file, methods)| {
            let spec: serde_json::Value = read_spec(format!("./resources/{version_id}/{file}"));

            methods.iter().map(move |method| {
                let index = get_method_index(&spec, method);
                format!("file:///api/{file}#/methods/{index}/result")
            })
        }),
        version_id,
    )
}

// TODO(shahak): Remove allow(dead_code) once we use this function.
#[allow(dead_code)]
pub fn get_starknet_spec_api_schema_for_method_errors(
    file_to_methods: &[(SpecFile, &[&str])],
    version_id: &VersionId,
) -> JSONSchema {
    get_starknet_spec_api_schema(
        file_to_methods.iter().flat_map(|(file, methods)| {
            let spec: serde_json::Value = read_spec(format!("./resources/{version_id}/{file}"));

            methods.iter().flat_map(move |method| {
                let index = get_method_index(&spec, method);
                let method_json_obj =
                    spec.as_object().unwrap().get("methods").unwrap().as_array().unwrap()[index]
                        .as_object()
                        .unwrap();
                let errors_len = method_json_obj.get("errors").unwrap().as_array().unwrap().len();

                (0..errors_len).map(move |error_index| {
                    format!("file:///api/{file}#/methods/{index}/errors/{error_index}")
                })
            })
        }),
        version_id,
    )
}

fn get_starknet_spec_api_schema<Refs: IntoIterator<Item = String>>(
    refs: Refs,
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

    let mut refs_schema_str = String::from(r#"{"anyOf": ["#);
    const SEPARATOR: &str = ", ";
    for ref_str in refs {
        refs_schema_str += &format!(r##"{{"$ref": "{ref_str}"}}"##,);
        refs_schema_str += SEPARATOR;
    }
    // Remove the last separator.
    refs_schema_str.truncate(refs_schema_str.len() - SEPARATOR.len());
    refs_schema_str += r#"], "unevaluatedProperties": false}"#;
    let refs_schema = serde_json::from_str(&refs_schema_str).unwrap();

    options.compile(&refs_schema).unwrap()
}

fn read_spec<P: AsRef<Path>>(path: P) -> serde_json::Value {
    let spec_str = std::fs::read_to_string(path).unwrap();
    serde_json::from_str(&spec_str).unwrap()
}

fn get_method_index(spec: &serde_json::Value, method: &str) -> usize {
    let methods_json_arr = spec.as_object().unwrap().get("methods").unwrap().as_array().unwrap();
    for (i, method_object) in methods_json_arr.iter().enumerate() {
        if method_object.as_object().unwrap().get("name").unwrap() == method {
            return i;
        }
    }
    panic!("Method {method} doesn't exist");
}
