use assert_matches::assert_matches;
use test_utils::validate_load_and_dump;

use crate::writer::objects::transaction::Transaction;

#[test]
fn load_and_dump_deploy_account_same_string() {
    validate_load_and_dump("writer/deploy_account.json", |transaction| {
        assert_matches!(transaction, Transaction::DeployAccount(_));
    });
}

#[test]
fn load_and_dump_invoke_same_string() {
    validate_load_and_dump("writer/invoke.json", |transaction| {
        assert_matches!(transaction, Transaction::Invoke(_));
    });
}

#[test]
fn load_and_dump_declare_v1_same_string() {
    validate_load_and_dump("writer/declare_v1.json", |transaction| {
        assert_matches!(transaction, Transaction::DeclareV1(_));
    });
}

#[test]
fn load_and_dump_declare_v2_same_string() {
    validate_load_and_dump("writer/declare_v2.json", |transaction| {
        assert_matches!(transaction, Transaction::DeclareV2(_));
    });
}
