use test_utils::validate_load_and_dump;

use super::{
    DeclareV1Transaction,
    DeclareV2Transaction,
    DeclareV3Transaction,
    DeployAccountV1Transaction,
    DeployAccountV3Transaction,
    InvokeV1Transaction,
    InvokeV3Transaction,
};

#[test]
fn load_and_dump_deploy_account_v1_same_string() {
    validate_load_and_dump::<DeployAccountV1Transaction>("writer/deploy_account_v1.json");
}

#[test]
fn load_and_dump_deploy_account_v3_same_string() {
    validate_load_and_dump::<DeployAccountV3Transaction>("writer/deploy_account_v3.json");
}

#[test]
fn load_and_dump_invoke_v1_same_string() {
    validate_load_and_dump::<InvokeV1Transaction>("writer/invoke_v1.json");
}

#[test]
fn load_and_dump_invoke_v3_same_string() {
    validate_load_and_dump::<InvokeV3Transaction>("writer/invoke_v3.json");
}

#[test]
fn load_and_dump_declare_v1_same_string() {
    validate_load_and_dump::<DeclareV1Transaction>("writer/declare_v1.json");
}

#[test]
fn load_and_dump_declare_v2_same_string() {
    validate_load_and_dump::<DeclareV2Transaction>("writer/declare_v2.json");
}

#[test]
fn load_and_dump_declare_v3_same_string() {
    validate_load_and_dump::<DeclareV3Transaction>("writer/declare_v3.json");
}
