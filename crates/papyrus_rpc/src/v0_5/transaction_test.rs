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
use starknet_api::transaction::{
    Calldata,
    ContractAddressSalt,
    Fee,
    L1HandlerTransaction,
    Transaction,
    TransactionSignature,
    TransactionVersion,
};
use starknet_api::{calldata, contract_address, patricia_key};
use starknet_client::writer::objects::transaction as client_transaction;
use starknet_types_core::felt::Felt;
use test_utils::{auto_impl_get_test_instance, get_number_of_variants, get_rng, GetTestInstance};

use super::{
    DeployAccountTransaction,
    DeployAccountTransactionV1,
    InvokeTransaction,
    InvokeTransactionV0,
    InvokeTransactionV1,
    TransactionOutput,
    TransactionVersion0,
    TransactionVersion1,
};

lazy_static::lazy_static! {
    // A transaction from GOERLI with tx hash 0x7c9660754689dee9c6de773f1c4c9d94269ed678e7199298a9e1a19cda415ab.
    static ref L1_HANDLER_TX: L1HandlerTransaction = L1HandlerTransaction {
        version: TransactionVersion::ZERO,
        nonce: Nonce(Felt::from_hex_unchecked("0xc01b3")),
        contract_address: contract_address!(Felt::from_hex(
            "0x55350a859da02cb244c8c09f29bc38047cef93d38b72033a0e8be03d24c5756").unwrap()
        ),
        entry_point_selector: EntryPointSelector(Felt::from_hex(
            "0x3fa70707d0e831418fb142ca8fb7483611b84e89c0c42bf1fc2a7a5c40890ad"
        ).unwrap()),
        calldata: calldata![
            Felt::from_hex("0x18e4a8e2badb5f5950758f46f8108e2c5d357b07").unwrap(),
            Felt::from_hex("0x10ae809a95d34dd22538e6c30bec2e11").unwrap(),
            Felt::from_hex("0x8eacfcd7b4046547e3cbe5ff4f08c1f9").unwrap(),
            Felt::from_hex_unchecked("0x99c3dd"),
            Felt::ZERO
        ],
    };
}

// The msg hash of the L1Handler transaction.
const MSG_HASH: &str = "0xd667cda2d870b8146c115cc4e93d701b3e34313686e5925ddc421576a1c8bbd2";

use crate::v0_5::transaction::{L1HandlerMsgHash, L1L2MsgHash};
auto_impl_get_test_instance! {
    pub enum DeployAccountTransaction {
        Version1(DeployAccountTransactionV1) = 0,
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
    pub enum InvokeTransaction {
        Version0(InvokeTransactionV0) = 0,
        Version1(InvokeTransactionV1) = 1,
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
    pub enum TransactionVersion0 {
        Version0 = 0,
    }

    pub enum TransactionVersion1 {
        Version1 = 0,
    }
}

macro_rules! gen_test_from_thin_transaction_output_macro {
    ($variant: ident) => {
        paste! {
            #[tokio::test]
            async fn [<from_thin_transaction_output_ $variant:lower>]() {
                let thin_output = ThinTransactionOutput::$variant([<Thin $variant TransactionOutput>]::default());
                let output = TransactionOutput::from_thin_transaction_output(thin_output, vec![], None);
                assert_matches!(output, TransactionOutput::$variant(_));
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
    let output =
        TransactionOutput::from_thin_transaction_output(thin_output, vec![], Some(msg_hash));
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
}

#[test]
fn test_invoke_transaction_to_client_transaction() {
    let _invoke_transaction: client_transaction::InvokeTransaction =
        InvokeTransactionV1::get_test_instance(&mut get_rng()).into();
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
