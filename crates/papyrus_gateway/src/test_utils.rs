use std::path::Path;
use std::sync::Arc;

use derive_more::Display;
use jsonrpsee::server::RpcModule;
use jsonschema::JSONSchema;
use papyrus_common::BlockHashAndNumber;
use papyrus_storage::test_utils::get_test_storage;
use papyrus_storage::StorageWriter;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use starknet_api::core::{ChainId, ContractAddress, PatriciaKey};
use starknet_api::hash::StarkHash;
use starknet_api::{contract_address, patricia_key};
use starknet_client::writer::MockStarknetWriter;
use tokio::sync::RwLock;

use crate::api::JsonRpcServerImpl;
use crate::version_config::VersionId;
use crate::GatewayConfig;

pub fn get_test_gateway_config() -> GatewayConfig {
    GatewayConfig {
        chain_id: ChainId("SN_GOERLI".to_string()),
        fee_contract_address: contract_address!("0x1001"),
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
-> (RpcModule<T>, StorageWriter) {
    get_test_rpc_server_and_storage_writer_from_params(None, None)
}

pub(crate) fn get_test_rpc_server_and_storage_writer_from_params<T: JsonRpcServerImpl>(
    mock_client: Option<MockStarknetWriter>,
    shared_highest_block: Option<Arc<RwLock<Option<BlockHashAndNumber>>>>,
) -> (RpcModule<T>, StorageWriter) {
    let mock_client = mock_client.unwrap_or(MockStarknetWriter::new());
    let shared_highest_block = shared_highest_block.unwrap_or(get_test_highest_block());

    let ((storage_reader, storage_writer), _temp_dir) = get_test_storage();
    let config = get_test_gateway_config();
    let mock_client_arc = Arc::new(mock_client);
    (
        T::new(
            config.chain_id,
            config.fee_contract_address,
            storage_reader,
            config.max_events_chunk_size,
            config.max_events_keys,
            BlockHashAndNumber::default(),
            shared_highest_block,
            mock_client_arc,
        )
        .into_rpc_module(),
        storage_writer,
    )
}

// Call a method on the `RPC module` without having to spin up a server.
// Returns the raw `result field` in JSON-RPC response and the deserialized result if successful.
pub(crate) async fn raw_call<R: JsonRpcServerImpl, S: Serialize, T: for<'a> Deserialize<'a>>(
    module: &RpcModule<R>,
    method: &str,
    params_obj: &Option<S>, //&str,
) -> (Value, T) {
    let params_str = if params_obj.is_none() {
        "".to_string()
    } else {
        let params = serde_json::to_value(params_obj).unwrap().to_string();
        format!(r#","params":[{params}]"#)
    };
    let req = format!(r#"{{"jsonrpc":"2.0","id":"1","method":"{method}"{params_str}}}"#);
    let (resp_wrapper, _) = module.raw_json_request(req.as_str(), 1).await.expect("request format");
    assert!(resp_wrapper.success, "unsuccessful request");

    let json_resp: Value = serde_json::from_str(&resp_wrapper.result).unwrap();
    let result = serde_json::from_value::<T>(
        json_resp.get("result").expect("response should have result field").to_owned(),
    )
    .expect("result should match the target type");
    (json_resp, result)
}

// TODO(nevo): Schmea validates null as valid for an unknown reason.
// Investigate in the future and remove this function (use is_valid directly)
pub fn validate_schema(schema: &JSONSchema, result: &Value) -> bool {
    result != &Value::Null && schema.is_valid(result)
}

#[derive(Clone, Copy, Display)]
// TODO(yair): Remove Starknet prefix and remove allow.
#[allow(clippy::enum_variant_names)]
pub enum SpecFile {
    #[display(fmt = "starknet_api_openrpc.json")]
    StarknetApiOpenrpc,
    #[display(fmt = "starknet_write_api.json")]
    StarknetWriteApi,
    #[display(fmt = "starknet_trace_api_openrpc.json")]
    StarknetTraceApi,
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
                format!("file:///api/{file}#/methods/{index}/result/schema")
            })
        }),
        version_id,
    )
}

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
        let mut spec: serde_json::Value = serde_json::from_str(&spec_str).unwrap();
        fix_errors(&mut spec);
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

// This function will change the errors in components/errors into schemas that accept the error.
// It will change an error from the following json object:
// { "code": 1, "message": "an error occured" } into {
//     "properties": {
//         "code: {
//             "type": "integer",
//             "enum": [1]
//          },
//          "message": {
//              "type": "string",
//              "enum": ["an error occured"]
//          }
//      },
//      required: ["code", "message"]
//  }
// And it will change an error from the following json object:
// { "code": 1, "message": "an error occured", "data": "string" } into {
//     "properties": {
//         "code: {
//             "type": "integer",
//             "enum": [1]
//          },
//          "message": {
//              "type": "string",
//              "enum": ["an error occured"]
//          }
//          "data": {}
//      },
//      required: ["code", "message", "data"]
//  }
fn fix_errors(spec: &mut serde_json::Value) {
    let Some(errors) = spec
        .as_object_mut()
        .and_then(|obj| obj.get_mut("components"))
        .and_then(|components| components.as_object_mut())
        .and_then(|components| components.get_mut("errors"))
        .and_then(|errors| errors.as_object_mut())
    else {
        return;
    };
    for value in errors.values_mut() {
        let obj = value.as_object_mut().unwrap();
        let Some(code) = obj.get("code").map(|code_obj| (*code_obj).clone()) else {
            continue;
        };
        let Some(message) = obj.get("message").map(|message_obj| (*message_obj).clone()) else {
            continue;
        };
        let has_data = obj.contains_key("data");
        obj.clear();
        let mut properties = serde_json::Map::from_iter([
            (
                "code".to_string(),
                serde_json::Map::from_iter([
                    ("type".to_string(), "integer".into()),
                    ("enum".to_string(), vec![code].into()),
                ])
                .into(),
            ),
            (
                "message".to_string(),
                serde_json::Map::from_iter([
                    ("type".to_string(), "string".into()),
                    ("enum".to_string(), vec![message].into()),
                ])
                .into(),
            ),
        ]);
        let mut required: Vec<serde_json::Value> = vec!["code".into(), "message".into()];
        if has_data {
            properties.insert("data".to_string(), serde_json::Map::from_iter([]).into());
            required.push("data".into());
        }
        obj.insert("properties".to_string(), properties.into());
        obj.insert("required".to_string(), required.into());
    }
}
