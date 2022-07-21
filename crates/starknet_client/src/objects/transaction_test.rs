use assert::assert_ok;

use super::super::test_utils::read_resource::read_resource_file;
use super::transaction::InvokeTransaction;

#[test]
fn load_invoke_transaction_succeeds() {
    assert_ok!(serde_json::from_str::<InvokeTransaction>(&read_resource_file(
        "invoke_transaction.json"
    )));
}
