use test_utils::validate_load_and_dump;

use super::{
    DeclareV1Transaction,
    DeclareV2Transaction,
    DeployAccountTransaction,
    InvokeTransaction,
};

#[test]
fn load_and_dump_deploy_account_same_string() {
    validate_load_and_dump::<DeployAccountTransaction>("writer/deploy_account.json");
}

#[test]
fn load_and_dump_invoke_same_string() {
    validate_load_and_dump::<InvokeTransaction>("writer/invoke.json");
}

#[test]
fn load_and_dump_declare_v1_same_string() {
    validate_load_and_dump::<DeclareV1Transaction>("writer/declare_v1.json");
}

#[test]
fn load_and_dump_declare_v2_same_string() {
    validate_load_and_dump::<DeclareV2Transaction>("writer/declare_v2.json");
}
