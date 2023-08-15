use std::env;
use std::fs::read_to_string;
use std::path::Path;

use assert_matches::assert_matches;
use cairo_lang_starknet::casm_contract_class::CasmContractClass;
use indexmap::indexmap;
use jsonrpsee::core::Error;
use papyrus_execution::execution_utils::selector_from_name;
use papyrus_execution::ExecutableTransactionInput;
use papyrus_storage::body::BodyStorageWriter;
use papyrus_storage::compiled_class::CasmStorageWriter;
use papyrus_storage::header::HeaderStorageWriter;
use papyrus_storage::state::StateStorageWriter;
use papyrus_storage::StorageWriter;
use pretty_assertions::assert_eq;
use starknet_api::block::{BlockBody, BlockHeader, BlockNumber, BlockTimestamp, GasPrice};
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce, PatriciaKey};
use starknet_api::deprecated_contract_class::ContractClass as SN_API_DeprecatedContractClass;
use starknet_api::hash::{StarkFelt, StarkHash};
use starknet_api::state::StateDiff;
use starknet_api::transaction::{Calldata, Fee, TransactionVersion};
use starknet_api::{calldata, patricia_key, stark_felt};
use test_utils::{auto_impl_get_test_instance, get_rng, read_json_file, GetTestInstance};

use super::api::{decompress_program, FeeEstimate};
use super::broadcasted_transaction::{
    BroadcastedDeclareTransaction,
    BroadcastedDeclareV1Transaction,
    BroadcastedTransaction,
};
use super::transaction::{DeployAccountTransaction, InvokeTransactionV1};
use crate::api::{BlockHashOrNumber, BlockId};
use crate::test_utils::{
    get_starknet_spec_api_schema_for_components,
    get_test_rpc_server_and_storage_writer,
    validate_schema,
    SpecFile,
};
use crate::v0_4_0::api::api_impl::JsonRpcServerV0_4Impl;
use crate::v0_4_0::error::{BLOCK_NOT_FOUND, CONTRACT_ERROR, CONTRACT_NOT_FOUND};
use crate::v0_4_0::transaction::InvokeTransaction;
use crate::version_config::VERSION_0_4;
const BLOCK_TIMESTAMP: BlockTimestamp = BlockTimestamp(1234);
const SEQUENCER_ADDRESS: &str = "0xa";
const GAS_PRICE: GasPrice = GasPrice(100 * u128::pow(10, 9)); // Given in units of wei.

#[tokio::test]
async fn execution_call() {
    let (module, storage_writer) =
        get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_4Impl>();

    prepare_storage_for_execution(storage_writer);

    let address = ContractAddress(patricia_key!("0x1"));
    let key = stark_felt!(1234_u16);
    let value = stark_felt!(18_u8);

    let res = module
        .call::<_, Vec<StarkFelt>>(
            "starknet_V0_4_call",
            (
                address,
                selector_from_name("test_storage_read_write"),
                calldata![key, value],
                BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(0))),
            ),
        )
        .await
        .unwrap();

    assert_eq!(res, vec![value]);

    // Calling a non-existent contract.
    let err = module
        .call::<_, Vec<StarkFelt>>(
            "starknet_V0_4_call",
            (
                ContractAddress(patricia_key!("0x1234")),
                selector_from_name("aaa"),
                calldata![key, value],
                BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(0))),
            ),
        )
        .await
        .unwrap_err();

    assert_matches!(err, Error::Call(err) if err == CONTRACT_NOT_FOUND.into());

    // Calling a non-existent block.
    let err = module
        .call::<_, Vec<StarkFelt>>(
            "starknet_V0_4_call",
            (
                ContractAddress(patricia_key!("0x1234")),
                selector_from_name("aaa"),
                calldata![key, value],
                BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(7))),
            ),
        )
        .await
        .unwrap_err();

    assert_matches!(err, Error::Call(err) if err == BLOCK_NOT_FOUND.into());

    // Calling a non-existent function (contract error).
    let err = module
        .call::<_, Vec<StarkFelt>>(
            "starknet_V0_4_call",
            (
                address,
                selector_from_name("aaa"),
                calldata![key, value],
                BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(0))),
            ),
        )
        .await
        .unwrap_err();

    assert_matches!(err, Error::Call(err) if err == CONTRACT_ERROR.into());
}

#[tokio::test]
async fn call_estimate_fee() {
    let (module, storage_writer) =
        get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_4Impl>();

    prepare_storage_for_execution(storage_writer);

    let address = ContractAddress(patricia_key!("0x1"));
    let account_address = ContractAddress(patricia_key!("0x444"));

    let invoke = BroadcastedTransaction::Invoke(InvokeTransaction::Version1(InvokeTransactionV1 {
        max_fee: Fee(1000000 * GAS_PRICE.0),
        version: TransactionVersion(stark_felt!("0x1")),
        sender_address: account_address,
        calldata: calldata![
            *address.0.key(),                      // Contract address.
            selector_from_name("return_result").0, // EP selector.
            stark_felt!(1_u8),                     // Calldata length.
            stark_felt!(2_u8)                      // Calldata: num.
        ],
        ..Default::default()
    }));

    let res = module
        .call::<_, Vec<FeeEstimate>>(
            "starknet_V0_4_estimateFee",
            (vec![invoke], BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(0)))),
        )
        .await
        .unwrap();

    // TODO(yair): verify this is the correct fee, got this value by printing the result of the
    // call.
    let expected_fee_estimate = vec![FeeEstimate {
        gas_consumed: stark_felt!("0x19a2"),
        gas_price: GAS_PRICE,
        overall_fee: Fee(656200000000000),
    }];

    assert_eq!(res, expected_fee_estimate);
}

#[test]
fn broadcasted_to_executable_declare_v1() {
    let mut rng = get_rng();
    let mut tx = BroadcastedDeclareV1Transaction::get_test_instance(&mut rng);
    tx.contract_class.compressed_program = get_test_compressed_program();
    let broadcasted_declare_v1 =
        BroadcastedTransaction::Declare(BroadcastedDeclareTransaction::V1(tx));
    assert_matches!(
        broadcasted_declare_v1.try_into(),
        Ok(ExecutableTransactionInput::DeclareV1(_tx, _class))
    );
}

#[test]
fn validate_fee_estimation_schema() {
    let mut rng = get_rng();
    let fee_estimate = FeeEstimate::get_test_instance(&mut rng);
    let schema = get_starknet_spec_api_schema_for_components(
        &[(SpecFile::StarknetApiOpenrpc, &["FEE_ESTIMATE"])],
        &VERSION_0_4,
    );
    dbg!(&schema);
    let serialized = serde_json::to_value(fee_estimate).unwrap();
    assert!(validate_schema(&schema, &serialized));
}

#[test]
fn broadcasted_to_executable_deploy_account() {
    let mut rng = get_rng();
    let broadcasted_deploy_account = BroadcastedTransaction::DeployAccount(
        DeployAccountTransaction::get_test_instance(&mut rng),
    );
    assert_matches!(
        broadcasted_deploy_account.try_into(),
        Ok(ExecutableTransactionInput::Deploy(_tx))
    );
}

#[test]
fn broadcasted_to_executable_invoke() {
    let mut rng = get_rng();
    let broadcasted_deploy_account =
        BroadcastedTransaction::Invoke(InvokeTransaction::get_test_instance(&mut rng));
    assert_matches!(
        broadcasted_deploy_account.try_into(),
        Ok(ExecutableTransactionInput::Invoke(_tx))
    );
}

#[test]
fn get_decompressed_program() {
    let compressed = get_test_compressed_program();
    let decompressed = decompress_program(&compressed);
    decompressed.expect("Couldn't decompress program");
}

fn get_test_compressed_program() -> String {
    let path = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("resources")
        .join("base64_compressed_program.txt");
    read_to_string(path).expect("Couldn't read compressed program")
}

auto_impl_get_test_instance! {
    pub struct FeeEstimate {
        pub gas_consumed: StarkFelt,
        pub gas_price: GasPrice,
        pub overall_fee: Fee,
    }
}

fn prepare_storage_for_execution(mut storage_writer: StorageWriter) {
    let class_hash1 = ClassHash(1u128.into());
    let class1 = serde_json::from_value::<SN_API_DeprecatedContractClass>(read_json_file(
        "deprecated_class.json",
    ))
    .unwrap();
    let address1 = ContractAddress(patricia_key!("0x1"));

    let class_hash2 = ClassHash(StarkFelt::from(2u128));
    let address2 = ContractAddress(patricia_key!("0x2"));
    // The class is not used in the execution, so it can be default.
    let class2 = starknet_api::state::ContractClass::default();
    let casm = serde_json::from_value::<CasmContractClass>(read_json_file("casm.json")).unwrap();
    let compiled_class_hash = CompiledClassHash(StarkHash::default());

    let account_class_hash = ClassHash(StarkHash::try_from("0x333").unwrap());
    let account_class = serde_json::from_value(read_json_file("account_class.json")).unwrap();
    let account_address = ContractAddress(patricia_key!("0x444"));

    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(
            BlockNumber(0),
            &BlockHeader {
                gas_price: GAS_PRICE,
                sequencer: ContractAddress(patricia_key!(SEQUENCER_ADDRESS)),
                timestamp: BLOCK_TIMESTAMP,
                ..Default::default()
            },
        )
        .unwrap()
        .append_body(BlockNumber(0), BlockBody::default())
        .unwrap()
        .append_state_diff(
            BlockNumber(0),
            StateDiff {
                deployed_contracts: indexmap!(
                    address1 => class_hash1,
                    address2 => class_hash2,
                    account_address => account_class_hash,
                ),
                storage_diffs: indexmap!(),
                declared_classes: indexmap!(
                    class_hash2 =>
                    (compiled_class_hash, class2)
                ),
                deprecated_declared_classes: indexmap!(
                    class_hash1 => class1,
                    account_class_hash => account_class
                ),
                nonces: indexmap!(
                    address1 => Nonce::default(),
                    address2 => Nonce::default(),
                    account_address => Nonce::default(),
                ),
                replaced_classes: indexmap!(),
            },
            indexmap!(),
        )
        .unwrap()
        .append_casm(&class_hash2, &casm)
        .unwrap()
        .commit()
        .unwrap();
}
