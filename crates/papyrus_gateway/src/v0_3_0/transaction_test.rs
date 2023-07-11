use assert_matches::assert_matches;
use camelpaste::paste;
use papyrus_storage::body::events::{
    ThinDeclareTransactionOutput, ThinDeployAccountTransactionOutput, ThinDeployTransactionOutput,
    ThinInvokeTransactionOutput, ThinL1HandlerTransactionOutput, ThinTransactionOutput,
};
use starknet_api::block::BlockHeader;
use starknet_api::transaction::{
    DeclareTransactionOutput, DeployAccountTransactionOutput, DeployTransactionOutput,
    InvokeTransactionOutput, L1HandlerTransactionOutput, Transaction,
};
use test_utils::{get_rng, GetTestInstance};

use crate::v0_3_0::transaction::{TransactionOutput, TransactionReceipt};

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

macro_rules! test_recipe_from_transtaction_output {
    ($variant:ident, $recipe_type:ident) => {
        paste! {
            #[tokio::test]
            async fn [<test_recipe_from_transtaction_output_ $variant:lower>]() {
                let mut rng = get_rng();
                let block_header = BlockHeader::default();
                let transaction = Transaction::$variant(
                    starknet_api::transaction::[<$variant Transaction>]::get_test_instance(&mut rng),
                );
                let output = TransactionOutput::$variant([<$variant TransactionOutput>]::default());
                let receipt = TransactionReceipt::from_transaction_output(
                    output,
                    &transaction,
                    block_header.block_hash,
                    block_header.block_number,
                );
                assert_matches!(receipt, TransactionReceipt::$recipe_type(_));
            }
        }
    }
}

test_recipe_from_transtaction_output!(Declare, Common);
test_recipe_from_transtaction_output!(Invoke, Common);
test_recipe_from_transtaction_output!(L1Handler, Common);
test_recipe_from_transtaction_output!(Deploy, Deploy);
test_recipe_from_transtaction_output!(DeployAccount, Deploy);
