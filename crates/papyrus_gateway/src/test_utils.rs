use std::net::SocketAddr;

use jsonrpsee::http_server::RpcModule;
use jsonschema::JSONSchema;
use papyrus_storage::test_utils::get_test_storage;
use papyrus_storage::StorageWriter;
use reqwest::Client;
use starknet_api::block::{
    Block, BlockBody, BlockHash, BlockHeader, BlockNumber, BlockTimestamp, GasPrice,
};
use starknet_api::core::{
    ChainId, ClassHash, ContractAddress, EntryPointSelector, GlobalRoot, Nonce, PatriciaKey,
};
use starknet_api::hash::StarkHash;
use starknet_api::serde_utils::bytes_from_hex_str;
use starknet_api::transaction::{
    CallData, ContractAddressSalt, DeployTransaction, DeployTransactionOutput, EthAddress, Fee,
    InvokeTransaction, InvokeTransactionOutput, L2ToL1Payload, MessageToL1, Transaction,
    TransactionHash, TransactionOutput, TransactionSignature, TransactionVersion,
};
use starknet_api::{patky, shash};
use web3::types::H160;

use crate::{GatewayConfig, JsonRpcServer, JsonRpcServerImpl};

// TODO(anatg): See if this can be usefull for the benchmark testing as well.
pub async fn send_request(address: SocketAddr, method: &str, params: &str) -> serde_json::Value {
    let client = Client::new();
    let res_str = client
        .post(format!("http://{:?}", address))
        .header("Content-Type", "application/json")
        .body(format!(
            r#"{{"jsonrpc":"2.0","id":"1","method":"{}","params":[{}]}}"#,
            method, params
        ))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    serde_json::from_str(&res_str).unwrap()
}

pub fn get_test_chain_id() -> ChainId {
    ChainId("SN_GOERLI".to_string())
}

pub fn get_test_gateway_config() -> GatewayConfig {
    GatewayConfig {
        chain_id: get_test_chain_id(),
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

    let mut components = String::from(r#"{"oneOf": ["#);
    for component in component_names {
        components +=
            &format!(r##"{{"$ref": "file:///spec.json#/components/schemas/{}"}}"##, component);
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

pub fn get_body_to_match_json_file() -> BlockBody {
    let transactions = vec![
        Transaction::Deploy(DeployTransaction {
            transaction_hash: TransactionHash(shash!(
                "0x4dd12d3b82c3d0b216503c6abf63f1ccad222461582eac82057d46c327331d2"
            )),
            version: TransactionVersion::default(),
            class_hash: ClassHash(shash!(
                "0x10455c752b86932ce552f2b0fe81a880746649b9aee7e0d842bf3f52378f9f8"
            )),
            contract_address: ContractAddress(patky!(
                "0x543e54f26ae33686f57da2ceebed98b340c3a78e9390931bd84fb711d5caabc"
            )),
            contract_address_salt: ContractAddressSalt(shash!(
                "0x25ad1e011d139412b19ec5284fe6e95f4e53d319056c5650042eb3322cc370d"
            )),
            constructor_calldata: CallData(vec![
                shash!("0x70be09c520814c13480a220ad31eb94bf37f0259e002b0275e55f3c309ee823"),
                shash!("0x1dc19dce5326f42f2b319d78b237148d1e582efbf700efd6eb2c9fcbc451327"),
            ]),
        }),
        Transaction::Deploy(DeployTransaction {
            transaction_hash: TransactionHash(shash!(
                "0x1a5f7247cc207f5b5c2e48b7605e46b872b83a2fa842955aea42d3cd80dbff"
            )),
            version: TransactionVersion::default(),
            class_hash: ClassHash(shash!(
                "0x10455c752b86932ce552f2b0fe81a880746649b9aee7e0d842bf3f52378f9f8"
            )),
            contract_address: ContractAddress(patky!(
                "0x2fb7ff5b1b474e8e691f5bebad9aa7aa3009f6ef22ccc2816f96cdfe217604d"
            )),
            contract_address_salt: ContractAddressSalt(shash!(
                "0x3a27aed698130e1817544c060261e8aede51a02f4da510c67ff26c5fbae850e"
            )),
            constructor_calldata: CallData(vec![
                shash!("0x420eefdc029d53134b57551d676c9a450e5f75f9f017ca75f6fb28350f60d54"),
                shash!("0x7c7139d51f4642ec66088959e69eb890e2e6e87c08dad2a223da9161c99c939"),
            ]),
        }),
        Transaction::Deploy(DeployTransaction {
            transaction_hash: TransactionHash(shash!(
                "0x5ea9bca61575eeb4ed38a16cefcbf66ba1ed642642df1a1c07b44316791b378"
            )),
            version: TransactionVersion::default(),
            class_hash: ClassHash(shash!(
                "0x7b40f8e4afe1316fce16375ac7a06d4dd27c7a4e3bcd6c28afdd208c5db433d"
            )),
            contract_address: ContractAddress(patky!(
                "0x1bb929cc5e6d80f0c71e90365ab77e9cbb2e0a290d72255a3f4d34060b5ed52"
            )),
            contract_address_salt: ContractAddressSalt::default(),
            constructor_calldata: CallData(vec![]),
        }),
        Transaction::Invoke(InvokeTransaction {
            transaction_hash: TransactionHash(shash!(
                "0x6525d9aa309e5c80abbdafcc434d53202e06866597cd6dbbc91e5894fad7155"
            )),
            max_fee: Fee::default(),
            version: TransactionVersion::default(),
            signature: TransactionSignature::default(),
            nonce: Nonce::default(),
            sender_address: ContractAddress(patky!(
                "0x2fb7ff5b1b474e8e691f5bebad9aa7aa3009f6ef22ccc2816f96cdfe217604d"
            )),
            entry_point_selector: Some(EntryPointSelector(shash!(
                "0x12ead94ae9d3f9d2bdb6b847cf255f1f398193a1f88884a0ae8e18f24a037b6"
            ))),
            calldata: CallData(vec![shash!("0xe3402af6cc1bca3f22d738ab935a5dd8ad1fb230")]),
        }),
    ];

    let transaction_outputs = vec![
        TransactionOutput::Deploy(DeployTransactionOutput {
            actual_fee: Fee::default(),
            messages_sent: vec![],
            events: vec![],
        }),
        TransactionOutput::Deploy(DeployTransactionOutput {
            actual_fee: Fee::default(),
            messages_sent: vec![],
            events: vec![],
        }),
        TransactionOutput::Deploy(DeployTransactionOutput {
            actual_fee: Fee::default(),
            messages_sent: vec![],
            events: vec![],
        }),
        TransactionOutput::Invoke(InvokeTransactionOutput {
            actual_fee: Fee::default(),
            messages_sent: vec![MessageToL1 {
                to_address: EthAddress(H160(
                    bytes_from_hex_str::<20, true>("0xe3402aF6cc1BCa3f22D738AB935a5Dd8AD1Fb230")
                        .unwrap(),
                )),
                payload: L2ToL1Payload(vec![shash!("0xc"), shash!("0x22")]),
            }],
            events: vec![],
        }),
    ];

    BlockBody { transactions, transaction_outputs }
}

pub fn get_block_to_match_json_file() -> Block {
    let header = BlockHeader {
        block_hash: BlockHash(shash!(
            "0x75e00250d4343326f322e370df4c9c73c7be105ad9f532eeb97891a34d9e4a5"
        )),
        parent_hash: BlockHash(shash!(
            "0x7d328a71faf48c5c3857e99f20a77b18522480956d1cd5bff1ff2df3c8b427b"
        )),
        block_number: BlockNumber(1),
        gas_price: GasPrice::default(),
        state_root: GlobalRoot(shash!(
            "0x3f04ffa63e188d602796505a2ee4f6e1f294ee29a914b057af8e75b17259d9f"
        )),
        sequencer: ContractAddress::default(),
        timestamp: BlockTimestamp(1636989916),
    };

    Block { header, body: get_body_to_match_json_file() }
}
