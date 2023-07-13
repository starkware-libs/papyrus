use assert::assert_ok;
use assert_matches::assert_matches;
use papyrus_test_utils::read_json_file;

use crate::objects::transaction::Transaction;

fn validate_load_and_dump<F: Fn(&Transaction)>(
    path_in_resource_dir: &str,
    validate_transaction_matches: F,
) {
    let json_value = read_json_file(path_in_resource_dir);
    let load_result = serde_json::from_value::<Transaction>(json_value.clone());
    assert_ok!(load_result);
    validate_transaction_matches(load_result.as_ref().unwrap());
    let dump_result = serde_json::to_value(&(load_result.unwrap()));
    assert_ok!(dump_result);
    assert_eq!(json_value, dump_result.unwrap());
}

#[test]
fn load_and_dump_deploy_account_same_string() {
    validate_load_and_dump("deploy_account.json", |transaction| {
        assert_matches!(transaction, Transaction::DeployAccount(_));
    });
}

#[test]
fn load_and_dump_invoke_same_string() {
    validate_load_and_dump("invoke.json", |transaction| {
        assert_matches!(transaction, Transaction::Invoke(_));
    });
}

#[test]
fn load_and_dump_declare_v1_same_string() {
    validate_load_and_dump("declare_v1.json", |transaction| {
        assert_matches!(transaction, Transaction::DeclareV1(_));
    });
}

#[test]
fn load_and_dump_declare_v2_same_string() {
    validate_load_and_dump("declare_v2.json", |transaction| {
        assert_matches!(transaction, Transaction::DeclareV2(_));
    });
}
