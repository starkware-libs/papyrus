use assert::assert_ok;

use super::super::test_utils::read_resource::read_resource_file;
use super::transaction::DeclareTransaction;

#[test]
fn load_declare_transaction_succeeds() {
    assert_ok!(serde_json::from_str::<DeclareTransaction>(&read_resource_file(
        "declare_transaction.json"
    )));
}
