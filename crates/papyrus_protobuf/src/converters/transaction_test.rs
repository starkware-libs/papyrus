use lazy_static::lazy_static;
use starknet_api::transaction::{
    Builtin,
    DeclareTransaction,
    DeclareTransactionOutput,
    DeployAccountTransaction,
    DeployAccountTransactionOutput,
    DeployTransactionOutput,
    ExecutionResources,
    GasVector,
    InvokeTransaction,
    InvokeTransactionOutput,
    L1HandlerTransactionOutput,
    Resource,
    ResourceBounds,
    ResourceBoundsMapping,
    Transaction as StarknetApiTransaction,
    TransactionOutput,
};
use test_utils::{get_rng, GetTestInstance};

use crate::sync::DataOrFin;

macro_rules! create_transaction_output {
    ($tx_output_type:ty, $tx_output_enum_variant:ident) => {{
        let mut rng = get_rng();
        let mut transaction_output = <$tx_output_type>::get_test_instance(&mut rng);
        transaction_output.execution_resources = EXECUTION_RESOURCES.clone();
        transaction_output.events = vec![];
        TransactionOutput::$tx_output_enum_variant(transaction_output)
    }};
}

#[test]
fn convert_l1_handler_transaction_to_vec_u8_and_back() {
    let mut rng = get_rng();
    let transaction = starknet_api::transaction::L1HandlerTransaction::get_test_instance(&mut rng);
    let transaction = StarknetApiTransaction::L1Handler(transaction);

    let transaction_output = create_transaction_output!(L1HandlerTransactionOutput, L1Handler);
    convert_transaction_to_vec_u8_and_back(transaction, transaction_output);
}

#[test]
fn convert_deploy_transaction_to_vec_u8_and_back() {
    let mut rng = get_rng();
    let transaction = starknet_api::transaction::DeployTransaction::get_test_instance(&mut rng);
    let transaction = StarknetApiTransaction::Deploy(transaction);

    let transaction_output = create_transaction_output!(DeployTransactionOutput, Deploy);
    convert_transaction_to_vec_u8_and_back(transaction, transaction_output);
}

#[test]
fn convert_declare_transaction_v0_to_vec_u8_and_back() {
    let mut rng = get_rng();
    let transaction =
        starknet_api::transaction::DeclareTransactionV0V1::get_test_instance(&mut rng);
    let transaction = StarknetApiTransaction::Declare(DeclareTransaction::V0(transaction));

    let transaction_output = create_transaction_output!(DeclareTransactionOutput, Declare);
    convert_transaction_to_vec_u8_and_back(transaction, transaction_output);
}

#[test]
fn convert_declare_transaction_v1_to_vec_u8_and_back() {
    let mut rng = get_rng();
    let transaction =
        starknet_api::transaction::DeclareTransactionV0V1::get_test_instance(&mut rng);
    let transaction = StarknetApiTransaction::Declare(DeclareTransaction::V1(transaction));

    let transaction_output = create_transaction_output!(DeclareTransactionOutput, Declare);
    convert_transaction_to_vec_u8_and_back(transaction, transaction_output);
}

#[test]
fn convert_declare_transaction_v2_to_vec_u8_and_back() {
    let mut rng = get_rng();
    let transaction = starknet_api::transaction::DeclareTransactionV2::get_test_instance(&mut rng);
    let transaction = StarknetApiTransaction::Declare(DeclareTransaction::V2(transaction));

    let transaction_output = create_transaction_output!(DeclareTransactionOutput, Declare);
    convert_transaction_to_vec_u8_and_back(transaction, transaction_output);
}

#[test]
fn convert_declare_transaction_v3_to_vec_u8_and_back() {
    let mut rng = get_rng();
    let mut transaction =
        starknet_api::transaction::DeclareTransactionV3::get_test_instance(&mut rng);
    transaction.resource_bounds = RESOURCE_BOUNDS_MAPPING.clone();
    let transaction = StarknetApiTransaction::Declare(DeclareTransaction::V3(transaction));

    let transaction_output = create_transaction_output!(DeclareTransactionOutput, Declare);
    convert_transaction_to_vec_u8_and_back(transaction, transaction_output);
}

#[test]
fn convert_invoke_transaction_v0_to_vec_u8_and_back() {
    let mut rng = get_rng();
    let transaction = starknet_api::transaction::InvokeTransactionV0::get_test_instance(&mut rng);
    let transaction = StarknetApiTransaction::Invoke(InvokeTransaction::V0(transaction));

    let transaction_output = create_transaction_output!(InvokeTransactionOutput, Invoke);
    convert_transaction_to_vec_u8_and_back(transaction, transaction_output);
}

#[test]
fn convert_invoke_transaction_v1_to_vec_u8_and_back() {
    let mut rng = get_rng();
    let transaction = starknet_api::transaction::InvokeTransactionV1::get_test_instance(&mut rng);
    let transaction = StarknetApiTransaction::Invoke(InvokeTransaction::V1(transaction));

    let transaction_output = create_transaction_output!(InvokeTransactionOutput, Invoke);
    convert_transaction_to_vec_u8_and_back(transaction, transaction_output);
}

#[test]
fn convert_invoke_transaction_v3_to_vec_u8_and_back() {
    let mut rng = get_rng();
    let mut transaction =
        starknet_api::transaction::InvokeTransactionV3::get_test_instance(&mut rng);
    transaction.resource_bounds = RESOURCE_BOUNDS_MAPPING.clone();
    let transaction = StarknetApiTransaction::Invoke(InvokeTransaction::V3(transaction));

    let transaction_output = create_transaction_output!(InvokeTransactionOutput, Invoke);
    convert_transaction_to_vec_u8_and_back(transaction, transaction_output);
}

#[test]
fn convert_deploy_account_transaction_v1_to_vec_u8_and_back() {
    let mut rng = get_rng();
    let transaction =
        starknet_api::transaction::DeployAccountTransactionV1::get_test_instance(&mut rng);
    let transaction =
        StarknetApiTransaction::DeployAccount(DeployAccountTransaction::V1(transaction));

    let transaction_output =
        create_transaction_output!(DeployAccountTransactionOutput, DeployAccount);
    convert_transaction_to_vec_u8_and_back(transaction, transaction_output);
}

#[test]
fn convert_deploy_account_transaction_v3_to_vec_u8_and_back() {
    let mut rng = get_rng();
    let mut transaction =
        starknet_api::transaction::DeployAccountTransactionV3::get_test_instance(&mut rng);
    transaction.resource_bounds = RESOURCE_BOUNDS_MAPPING.clone();
    let transaction =
        StarknetApiTransaction::DeployAccount(DeployAccountTransaction::V3(transaction));

    let transaction_output =
        create_transaction_output!(DeployAccountTransactionOutput, DeployAccount);
    convert_transaction_to_vec_u8_and_back(transaction, transaction_output);
}

#[test]
fn fin_transaction_to_bytes_and_back() {
    let bytes_data =
        Vec::<u8>::from(DataOrFin::<(StarknetApiTransaction, TransactionOutput)>(None));

    let res_data =
        DataOrFin::<(StarknetApiTransaction, TransactionOutput)>::try_from(bytes_data).unwrap();
    assert!(res_data.0.is_none());
}

fn convert_transaction_to_vec_u8_and_back(
    transaction: StarknetApiTransaction,
    transaction_output: TransactionOutput,
) {
    let data = DataOrFin(Some((transaction, transaction_output)));
    let bytes_data = Vec::<u8>::from(data.clone());
    let res_data = DataOrFin::try_from(bytes_data).unwrap();
    assert_eq!(data, res_data);
}

lazy_static! {
    static ref EXECUTION_RESOURCES: ExecutionResources = ExecutionResources {
        steps: 0,
        builtin_instance_counter: std::collections::HashMap::from([
            (Builtin::RangeCheck, 1),
            (Builtin::Pedersen, 2),
            (Builtin::Poseidon, 3),
            (Builtin::EcOp, 4),
            (Builtin::Ecdsa, 5),
            (Builtin::Bitwise, 6),
            (Builtin::Keccak, 7),
            (Builtin::SegmentArena, 0),
        ]),
        memory_holes: 0,
        da_gas_consumed: GasVector::default(),
        gas_consumed: GasVector::default(),
    };
    static ref RESOURCE_BOUNDS_MAPPING: ResourceBoundsMapping = ResourceBoundsMapping(
        [
            (Resource::L1Gas, ResourceBounds { max_amount: 0x5, max_price_per_unit: 0x6 }),
            (Resource::L2Gas, ResourceBounds { max_amount: 0x5, max_price_per_unit: 0x6 }),
        ]
        .into_iter()
        .collect(),
    );
}
