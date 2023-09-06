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
use starknet_api::transaction::Transaction;
use test_utils::{get_rng, GetTestInstance};

use super::TransactionOutput;

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
#[tokio::test]
async fn test_gateway_trascation_from_starknet_api_transaction() {
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
