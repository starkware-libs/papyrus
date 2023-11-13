use assert_matches::assert_matches;

use super::{Transaction, TransactionReceipt};
use crate::test_utils::read_resource::read_resource_file;

#[test]
fn load_deploy_transaction_succeeds() {
    assert_matches!(
        serde_json::from_str::<Transaction>(&read_resource_file("reader/deploy_v0.json")),
        Ok(Transaction::Deploy(_))
    );
}

#[test]
fn load_invoke_transaction_succeeds() {
    assert_matches!(
        serde_json::from_str::<Transaction>(&read_resource_file("reader/invoke_v0.json")),
        Ok(Transaction::Invoke(_))
    );
}

#[test]
fn load_invoke_with_contract_address_transaction_succeeds() {
    let mut json_val: serde_json::Value =
        serde_json::from_str(&read_resource_file("reader/invoke_v0.json")).unwrap();
    let object = json_val.as_object_mut().unwrap();
    let sender_address_value = object.remove("sender_address").unwrap();
    object.insert("contract_address".to_string(), sender_address_value);
    assert_matches!(serde_json::from_value::<Transaction>(json_val), Ok(Transaction::Invoke(_)));
}

#[test]
fn load_l1_handler_transaction_succeeds() {
    assert_matches!(
        serde_json::from_str::<Transaction>(&read_resource_file("reader/l1_handler_v0.json")),
        Ok(Transaction::L1Handler(_))
    );
}

#[test]
fn load_declare_transaction_succeeds() {
    assert_matches!(
        serde_json::from_str::<Transaction>(&read_resource_file("reader/declare_v0.json")),
        Ok(Transaction::Declare(_))
    );
}

#[test]
fn load_transaction_succeeds() {
    for file_name in [
        "reader/deploy_v0.json",
        "reader/invoke_v0.json",
        "reader/invoke_v3.json",
        "reader/declare_v0.json",
        "reader/declare_v3.json",
        "reader/deploy_account_v3.json",
    ] {
        let res = serde_json::from_str::<Transaction>(&read_resource_file(file_name));
        assert!(res.is_ok(), "filename: {}, error: {:?}", file_name, res);
    }
}

#[test]
fn load_transaction_unknown_field_fails() {
    for file_name in [
        "reader/deploy_v0.json",
        "reader/invoke_v0.json",
        "reader/declare_v0.json",
        "reader/deploy_account_v3.json",
    ] {
        let mut json_value: serde_json::Value =
            serde_json::from_str(&read_resource_file(file_name)).unwrap();
        json_value
            .as_object_mut()
            .unwrap()
            .insert("unknown_field".to_string(), serde_json::Value::Null);
        let json_str = serde_json::to_string(&json_value).unwrap();
        assert!(serde_json::from_str::<Transaction>(&json_str).is_err(), "filename: {}", file_name);
    }
}

#[test]
fn load_transaction_wrong_type_fails() {
    for (file_name, new_wrong_type) in [
        // The transaction has a type that doesn't match the type it is paired with.
        ("reader/deploy_v0.json", "INVOKE_FUNCTION"),
        ("reader/invoke_v0.json", "DECLARE"),
        ("reader/declare_v0.json", "DEPLOY"),
        ("reader/deploy_account_v3.json", "INVOKE_FUNCTION"),
    ] {
        let mut json_value: serde_json::Value =
            serde_json::from_str(&read_resource_file(file_name)).unwrap();
        json_value
            .as_object_mut()
            .unwrap()
            .insert("type".to_string(), serde_json::Value::String(new_wrong_type.to_string()));
        let json_str = serde_json::to_string(&json_value).unwrap();
        assert!(serde_json::from_str::<Transaction>(&json_str).is_err(), "filename: {}", file_name);
    }
}

#[test]
fn load_transaction_receipt_succeeds() {
    for file_name in [
        "reader/transaction_receipt.json",
        "reader/transaction_receipt_without_l1_to_l2.json",
        "reader/transaction_receipt_without_l1_to_l2_nonce.json",
    ] {
        serde_json::from_str::<TransactionReceipt>(&read_resource_file(file_name)).unwrap_or_else(
            |err| {
                panic!(
                    "Failed to deserialize transaction receipt. Filename: {}. Error: {}",
                    file_name, err
                )
            },
        );
    }
}
