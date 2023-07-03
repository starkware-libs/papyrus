use assert::{assert_err, assert_ok};

use super::super::test_utils::read_resource::read_resource_file;
use super::transaction::{
    DeployTransaction, IntermediateDeclareTransaction, IntermediateInvokeTransaction,
    L1HandlerTransaction, Transaction, TransactionReceipt,
};

#[test]
fn load_deploy_transaction_succeeds() {
    assert_ok!(serde_json::from_str::<DeployTransaction>(&read_resource_file(
        "deploy_transaction.json"
    )));
}

#[test]
fn load_invoke_transaction_succeeds() {
    assert_ok!(serde_json::from_str::<IntermediateInvokeTransaction>(&read_resource_file(
        "invoke_transaction.json"
    )));
}

#[test]
fn load_invoke_with_contract_address_transaction_succeeds() {
    let json_str: String = read_resource_file("invoke_transaction.json");
    let json_str = json_str.replace("sender_address", "contract_address");
    assert_ok!(serde_json::from_str::<IntermediateInvokeTransaction>(&json_str));
}

#[test]
fn load_l1_handler_transaction_succeeds() {
    assert_ok!(serde_json::from_str::<L1HandlerTransaction>(&read_resource_file(
        "invoke_transaction_l1_handler.json"
    )));
}

#[test]
fn load_declare_transaction_succeeds() {
    assert_ok!(serde_json::from_str::<IntermediateDeclareTransaction>(&read_resource_file(
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
fn load_transaction_unknown_field_fails() {
    for file_name in
        ["deploy_transaction.json", "invoke_transaction.json", "declare_transaction.json"]
    {
        let mut json_value: serde_json::Value =
            serde_json::from_str(&read_resource_file(file_name)).unwrap();
        json_value
            .as_object_mut()
            .unwrap()
            .insert("unknown_field".to_string(), serde_json::Value::Null);
        let json_str = serde_json::to_string(&json_value).unwrap();
        assert_err!(serde_json::from_str::<Transaction>(&json_str));
    }
}

#[test]
fn load_transaction_wrong_type_fails() {
    for (file_name, new_type) in [
        ("deploy_transaction.json", "INVOKE_FUNCTION"),
        ("invoke_transaction.json", "DECLARE"),
        ("declare_transaction.json", "DEPLOY"),
    ] {
        let mut json_value: serde_json::Value =
            serde_json::from_str(&read_resource_file(file_name)).unwrap();
        json_value
            .as_object_mut()
            .unwrap()
            .insert("type".to_string(), serde_json::Value::String(new_type.to_string()));
        let json_str = serde_json::to_string(&json_value).unwrap();
        assert_err!(serde_json::from_str::<Transaction>(&json_str));
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
