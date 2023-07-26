use assert_matches::assert_matches;
use test_utils::validate_load_and_dump;

use crate::writer::objects::response::AddTransactionResponse;

#[test]
fn load_and_dump_deploy_account_same_string() {
    validate_load_and_dump("writer/deploy_account_response.json", |response| {
        assert_matches!(response, AddTransactionResponse::DeployAccountResponse(_));
    });
}

#[test]
fn load_and_dump_invoke_same_string() {
    validate_load_and_dump("writer/invoke_response.json", |response| {
        assert_matches!(response, AddTransactionResponse::InvokeResponse(_));
    });
}

#[test]
fn load_and_dump_declare_v1_same_string() {
    validate_load_and_dump("writer/declare_response.json", |response| {
        assert_matches!(response, AddTransactionResponse::DeclareResponse(_));
    });
}
