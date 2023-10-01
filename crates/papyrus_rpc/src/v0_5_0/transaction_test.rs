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
use starknet_api::core::{ClassHash, ContractAddress, EntryPointSelector, Nonce};
use starknet_api::transaction::{
    Calldata,
    ContractAddressSalt,
    Fee,
    Transaction,
    TransactionSignature,
    TransactionVersion,
};
use starknet_client::writer::objects::transaction as client_transaction;
use test_utils::{auto_impl_get_test_instance, get_number_of_variants, get_rng, GetTestInstance};

use super::{
    DeployAccountTransaction,
    DeployAccountTransactionV1,
    InvokeTransaction,
    InvokeTransactionV0,
    InvokeTransactionV1,
    TransactionOutput,
};
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
        pub version: TransactionVersion,
    }
    pub enum InvokeTransaction {
        Version0(InvokeTransactionV0) = 0,
        Version1(InvokeTransactionV1) = 1,
    }
    pub struct InvokeTransactionV0 {
        pub max_fee: Fee,
        pub version: TransactionVersion,
        pub signature: TransactionSignature,
        pub contract_address: ContractAddress,
        pub entry_point_selector: EntryPointSelector,
        pub calldata: Calldata,
    }
    pub struct InvokeTransactionV1 {
        pub max_fee: Fee,
        pub version: TransactionVersion,
        pub signature: TransactionSignature,
        pub nonce: Nonce,
        pub sender_address: ContractAddress,
        pub calldata: Calldata,
    }
}

macro_rules! gen_test_from_thin_transaction_output_macro {
    ($variant: ident) => {
        paste! {
            #[tokio::test]
            async fn [<from_thin_transaction_output_ $variant:lower>]() {
                let thin_output = ThinTransactionOutput::$variant([<Thin $variant TransactionOutput>]::default());
                let output = TransactionOutput::from_thin_transaction_output(thin_output, vec![]);
                assert_matches!(output, TransactionOutput::$variant(_));
            }
        }
    };
}

gen_test_from_thin_transaction_output_macro!(Declare);
gen_test_from_thin_transaction_output_macro!(Deploy);
gen_test_from_thin_transaction_output_macro!(DeployAccount);
gen_test_from_thin_transaction_output_macro!(Invoke);
gen_test_from_thin_transaction_output_macro!(L1Handler);

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
        InvokeTransactionV1::get_test_instance(&mut get_rng()).try_into().unwrap();
}
