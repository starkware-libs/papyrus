use std::fs;
use std::path::PathBuf;

use jsonrpsee::http_server::RpcModule;
use jsonschema::JSONSchema;

use papyrus_storage::test_utils::get_test_storage;
use papyrus_storage::StorageWriter;
use starknet_api::core::{ChainId, ContractAddress};
use starknet_api::hash::StarkFelt;

use crate::{GatewayConfig, JsonRpcServer, JsonRpcServerImpl};

pub const CONTRACT_INCREASE_CONTRACT_CLASS_HASH: &str = "0x111";
pub const CONTRACT_INCREASE_PATH: &str = "resources/contract_compiled.json";

pub fn get_test_gateway_config() -> GatewayConfig {
    GatewayConfig {
        chain_id: ChainId("SN_GOERLI".to_string()),
        server_address: String::from("127.0.0.1:0"),
        max_events_chunk_size: 10,
        max_events_keys: 10,
        fee_address: String::from(
            "0x049d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7",
        ),
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
            fee_token_address: ContractAddress::try_from(
                StarkFelt::try_from(config.fee_address.as_str()).unwrap(),
            )
            .unwrap(),
        }
        .into_rpc(),
        storage_writer,
    )
}

pub(crate) fn get_deprecated_contract_class(
    contract_json_path: &str,
) -> starknet_api::deprecated_contract_class::ContractClass {
    let path: PathBuf = [env!("CARGO_MANIFEST_DIR"), contract_json_path].iter().collect();
    let raw_contract_class = fs::read_to_string(path).unwrap();
    serde_json::from_str(&raw_contract_class).unwrap()
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
