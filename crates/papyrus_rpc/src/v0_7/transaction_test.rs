use pretty_assertions::assert_eq;
use starknet_api::core::{ClassHash, ContractAddress, EntryPointSelector, Nonce, PatriciaKey};
use starknet_api::data_availability::DataAvailabilityMode;
use starknet_api::hash::{StarkFelt, StarkHash};
use starknet_api::transaction::{
    AccountDeploymentData,
    Calldata,
    ContractAddressSalt,
    Fee,
    L1HandlerTransaction,
    PaymasterData,
    Tip,
    Transaction,
    TransactionSignature,
    TransactionVersion,
};
use starknet_api::{calldata, contract_address, patricia_key, stark_felt};
use starknet_client::writer::objects::transaction as client_transaction;
use test_utils::{auto_impl_get_test_instance, get_number_of_variants, get_rng, GetTestInstance};

use super::super::transaction::{L1HandlerMsgHash, L1L2MsgHash};
use super::{
    DeployAccountTransaction,
    DeployAccountTransactionV1,
    DeployAccountTransactionV3,
    InvokeTransaction,
    InvokeTransactionV0,
    InvokeTransactionV1,
    InvokeTransactionV3,
    ResourceBoundsMapping,
    TransactionVersion0,
    TransactionVersion1,
    TransactionVersion3,
};

lazy_static::lazy_static! {
    // A transaction from MAINNET with tx hash 0x439e12f67962c353182d72b4af12c3f11eaba4b36e552aebcdcd6db66971bdb.
    static ref L1_HANDLER_TX: L1HandlerTransaction = L1HandlerTransaction {
        version: TransactionVersion::ZERO,
        nonce: Nonce(stark_felt!("0x18e94d")),
        contract_address: contract_address!(
            "0x73314940630fd6dcda0d772d4c972c4e0a9946bef9dabf4ef84eda8ef542b82"
        ),
        entry_point_selector: EntryPointSelector(stark_felt!(
            "0x1b64b1b3b690b43b9b514fb81377518f4039cd3e4f4914d8a6bdf01d679fb19"
        )),
        calldata: calldata![
            stark_felt!("0xae0ee0a63a2ce6baeeffe56e7714fb4efe48d419"),
            stark_felt!("0x455448"),
            stark_felt!("0xc27947400e26e534e677afc2e9b2ec1bab14fc89"),
            stark_felt!("0x4af4754baf89f1b8b449215a8ea7ce558824a33a5393eaa3829658549f2bfa2"),
            stark_felt!("0x9184e72a000"),
            stark_felt!("0x0")
        ],
    };
}

// The msg hash of the L1Handler transaction.
const MSG_HASH: &str = "0x99b2a7830e1c860734b308d90bb05b0e09ecda0a2b243ecddb12c50bdebaa3a9";

auto_impl_get_test_instance! {
    pub enum DeployAccountTransaction {
        Version1(DeployAccountTransactionV1) = 0,
        Version3(DeployAccountTransactionV3) = 1,
    }
    pub struct DeployAccountTransactionV1 {
        pub max_fee: Fee,
        pub signature: TransactionSignature,
        pub nonce: Nonce,
        pub class_hash: ClassHash,
        pub contract_address_salt: ContractAddressSalt,
        pub constructor_calldata: Calldata,
        pub version: TransactionVersion1,
    }
    pub struct DeployAccountTransactionV3 {
        pub signature: TransactionSignature,
        pub nonce: Nonce,
        pub class_hash: ClassHash,
        pub contract_address_salt: ContractAddressSalt,
        pub constructor_calldata: Calldata,
        pub version: TransactionVersion3,
        pub resource_bounds: ResourceBoundsMapping,
        pub tip: Tip,
        pub paymaster_data: PaymasterData,
        pub nonce_data_availability_mode: DataAvailabilityMode,
        pub fee_data_availability_mode: DataAvailabilityMode,
    }
    pub enum InvokeTransaction {
        Version0(InvokeTransactionV0) = 0,
        Version1(InvokeTransactionV1) = 1,
        Version3(InvokeTransactionV3) = 2,
    }
    pub struct InvokeTransactionV0 {
        pub max_fee: Fee,
        pub version: TransactionVersion0,
        pub signature: TransactionSignature,
        pub contract_address: ContractAddress,
        pub entry_point_selector: EntryPointSelector,
        pub calldata: Calldata,
    }
    pub struct InvokeTransactionV1 {
        pub max_fee: Fee,
        pub version: TransactionVersion1,
        pub signature: TransactionSignature,
        pub nonce: Nonce,
        pub sender_address: ContractAddress,
        pub calldata: Calldata,
    }
    pub struct InvokeTransactionV3 {
        pub sender_address: ContractAddress,
        pub calldata: Calldata,
        pub version: TransactionVersion3,
        pub signature: TransactionSignature,
        pub nonce: Nonce,
        pub resource_bounds: ResourceBoundsMapping,
        pub tip: Tip,
        pub paymaster_data: PaymasterData,
        pub account_deployment_data: AccountDeploymentData,
        pub nonce_data_availability_mode: DataAvailabilityMode,
        pub fee_data_availability_mode: DataAvailabilityMode,
    }
    pub enum TransactionVersion0 {
        Version0 = 0,
    }
    pub enum TransactionVersion1 {
        Version1 = 0,
    }
    pub enum TransactionVersion3 {
        Version3 = 0,
    }
}

// TODO: check the conversion against the expected GW transaction.
#[test]
fn test_gateway_trascation_from_starknet_api_transaction() {
    let mut rng = get_rng();

    let inner_transaction = starknet_api::transaction::DeclareTransactionV0V1::default();
    let _transaction: super::Transaction =
        Transaction::Declare(starknet_api::transaction::DeclareTransaction::V0(inner_transaction))
            .try_into()
            .unwrap();

    let inner_transaction = starknet_api::transaction::DeclareTransactionV0V1::default();
    let _transaction: super::Transaction =
        Transaction::Declare(starknet_api::transaction::DeclareTransaction::V1(inner_transaction))
            .try_into()
            .unwrap();

    let inner_transaction =
        starknet_api::transaction::DeclareTransactionV3::get_test_instance(&mut rng);
    let _transaction: super::Transaction =
        Transaction::Declare(starknet_api::transaction::DeclareTransaction::V3(inner_transaction))
            .try_into()
            .unwrap();

    let inner_transaction = starknet_api::transaction::DeclareTransactionV2::default();
    let _transaction: super::Transaction =
        Transaction::Declare(starknet_api::transaction::DeclareTransaction::V2(inner_transaction))
            .try_into()
            .unwrap();

    let inner_transaction = starknet_api::transaction::InvokeTransactionV0::default();
    let _transaction: super::Transaction =
        Transaction::Invoke(starknet_api::transaction::InvokeTransaction::V0(inner_transaction))
            .try_into()
            .unwrap();

    let inner_transaction = starknet_api::transaction::InvokeTransactionV1::default();
    let _transaction: super::Transaction =
        Transaction::Invoke(starknet_api::transaction::InvokeTransaction::V1(inner_transaction))
            .try_into()
            .unwrap();

    let inner_transaction =
        starknet_api::transaction::InvokeTransactionV3::get_test_instance(&mut rng);
    let _transaction: super::Transaction =
        Transaction::Invoke(starknet_api::transaction::InvokeTransaction::V3(inner_transaction))
            .try_into()
            .unwrap();

    let inner_transaction =
        starknet_api::transaction::L1HandlerTransaction::get_test_instance(&mut rng);
    let _transaction: super::Transaction =
        Transaction::L1Handler(inner_transaction).try_into().unwrap();

    let inner_transaction =
        starknet_api::transaction::DeployTransaction::get_test_instance(&mut rng);
    let _transaction: super::Transaction =
        Transaction::Deploy(inner_transaction).try_into().unwrap();

    let inner_transaction =
        starknet_api::transaction::DeployAccountTransactionV1::get_test_instance(&mut rng);
    let _transaction: super::Transaction = Transaction::DeployAccount(
        starknet_api::transaction::DeployAccountTransaction::V1(inner_transaction),
    )
    .try_into()
    .unwrap();

    let inner_transaction =
        starknet_api::transaction::DeployAccountTransactionV3::get_test_instance(&mut rng);
    let _transaction: super::Transaction = Transaction::DeployAccount(
        starknet_api::transaction::DeployAccountTransaction::V3(inner_transaction),
    )
    .try_into()
    .unwrap();
}

#[test]
fn test_invoke_transaction_to_client_transaction() {
    let _invoke_transaction: client_transaction::InvokeTransaction =
        InvokeTransactionV1::get_test_instance(&mut get_rng()).into();

    let _invoke_transaction: client_transaction::InvokeTransaction =
        InvokeTransactionV3::get_test_instance(&mut get_rng()).into();
}

#[test]
fn l1handler_msg_hash() {
    let msg_hash = format!("{}", L1_HANDLER_TX.calc_msg_hash());
    assert_eq!(msg_hash, MSG_HASH);
}

#[test]
fn l1handler_msg_hash_serde() {
    let ser = serde_json::to_string(MSG_HASH).unwrap();
    assert_eq!(ser, "\"0x99b2a7830e1c860734b308d90bb05b0e09ecda0a2b243ecddb12c50bdebaa3a9\"");
    let des = serde_json::from_str::<L1L2MsgHash>(&ser).unwrap();
    let expected_hash = L1_HANDLER_TX.calc_msg_hash();
    assert_eq!(des, expected_hash);
}
