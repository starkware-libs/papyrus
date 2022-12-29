use assert::assert_ok;

use super::super::test_utils::read_resource::read_resource_file;
use super::transaction::{
    DeclareTransaction, DeployTransaction, InvokeTransaction, L1HandlerTransaction, Transaction,
    TransactionReceipt,
};

#[test]
fn load_deploy_transaction_succeeds() {
    assert_ok!(serde_json::from_str::<DeployTransaction>(&read_resource_file(
        "deploy_transaction.json"
    )));
}

#[test]
fn load_invoke_transaction_succeeds() {
    assert_ok!(serde_json::from_str::<InvokeTransaction>(&read_resource_file(
        "invoke_transaction.json"
    )));
}

#[test]
fn load_l1_handler_transaction_succeeds() {
    assert_ok!(serde_json::from_str::<L1HandlerTransaction>(&read_resource_file(
        "invoke_transaction_l1_handler.json"
    )));
}

#[test]
fn load_declare_transaction_succeeds() {
    assert_ok!(serde_json::from_str::<DeclareTransaction>(&read_resource_file(
        "declare_transaction.json"
    )));
}

#[test]
fn load_transaction_succeeds() {
    for file_name in
        ["deploy_transaction.json", "invoke_transaction.json", "declare_transaction.json"]
    {
        assert_ok!(serde_json::from_str::<Transaction>(&read_resource_file(file_name)));
    }
}

#[test]
fn load_transaction_receipt_succeeds() {
    for file_name in [
        "transaction_receipt.json",
        "transaction_receipt_without_l1_to_l2.json",
        "transaction_receipt_without_l1_to_l2_nonce.json",
    ] {
        assert_ok!(serde_json::from_str::<TransactionReceipt>(&read_resource_file(file_name)));
    }
}
