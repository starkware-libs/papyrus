use assert_matches::assert_matches;
use camelpaste::paste;
use papyrus_storage::body::events::{
    ThinDeclareTransactionOutput,
    ThinDeployAccountTransactionOutput,
    ThinDeployTransactionOutput,
    ThinInvokeTransactionOutput,
    ThinL1HandlerTransactionOutput,
    ThinTransactionOutput,
};
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
    TransactionOutput,
    TransactionVersion0,
    TransactionVersion1,
    TransactionVersion3,
};

lazy_static::lazy_static! {
    // A transaction from GOERLI with tx hash 0x7c9660754689dee9c6de773f1c4c9d94269ed678e7199298a9e1a19cda415ab.
    static ref L1_HANDLER_TX: L1HandlerTransaction = L1HandlerTransaction {
        version: TransactionVersion::ZERO,
        nonce: Nonce(stark_felt!("0xc01b3")),
        contract_address: contract_address!(
            "0x55350a859da02cb244c8c09f29bc38047cef93d38b72033a0e8be03d24c5756"
        ),
        entry_point_selector: EntryPointSelector(stark_felt!(
            "0x3fa70707d0e831418fb142ca8fb7483611b84e89c0c42bf1fc2a7a5c40890ad"
        )),
        calldata: calldata![
            stark_felt!("0x18e4a8e2badb5f5950758f46f8108e2c5d357b07"),
            stark_felt!("0x10ae809a95d34dd22538e6c30bec2e11"),
            stark_felt!("0x8eacfcd7b4046547e3cbe5ff4f08c1f9"),
            stark_felt!("0x99c3dd"),
            stark_felt!("0x0")
        ],
    };
}

// The msg hash of the L1Handler transaction.
const MSG_HASH: &str = "0xd667cda2d870b8146c115cc4e93d701b3e34313686e5925ddc421576a1c8bbd2";

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

macro_rules! gen_test_from_thin_transaction_output_macro {
    ($variant: ident) => {
        paste! {
            #[tokio::test]
            async fn [<from_thin_transaction_output_ $variant:lower>]() {
                    for tx_version in [TransactionVersion::ZERO, TransactionVersion::ONE, TransactionVersion::THREE] {
                    let thin_output = ThinTransactionOutput::$variant([<Thin $variant TransactionOutput>]::default());
                    let output = TransactionOutput::from_thin_transaction_output(thin_output, tx_version, vec![], None);
                    assert_matches!(output, TransactionOutput::$variant(_));
                }
            }
        }
    };
}

gen_test_from_thin_transaction_output_macro!(Declare);
gen_test_from_thin_transaction_output_macro!(Deploy);
gen_test_from_thin_transaction_output_macro!(DeployAccount);
gen_test_from_thin_transaction_output_macro!(Invoke);

#[tokio::test]
async fn from_thin_transaction_output_l1handler() {
    let thin_output = ThinTransactionOutput::L1Handler(ThinL1HandlerTransactionOutput::default());
    let msg_hash = L1L2MsgHash::default();
    let output = TransactionOutput::from_thin_transaction_output(
        thin_output,
        TransactionVersion::ZERO,
        vec![],
        Some(msg_hash),
    );
    assert_matches!(output, TransactionOutput::L1Handler(_));
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
    assert_eq!(ser, "\"0xd667cda2d870b8146c115cc4e93d701b3e34313686e5925ddc421576a1c8bbd2\"");
    let des = serde_json::from_str::<L1L2MsgHash>(&ser).unwrap();
    let expected_hash = L1_HANDLER_TX.calc_msg_hash();
    assert_eq!(des, expected_hash);
}
