use mockito::{mock, Matcher};
use test_utils::read_json_file;

use super::objects::response::AddTransactionResponse;
use super::objects::transaction::Transaction;
use super::{StarknetGatewayClient, StarknetWriter};
use crate::test_utils::retry::get_test_config;

const NODE_VERSION: &str = "NODE VERSION";

#[tokio::test]
async fn add_transaction() {
    let client =
        StarknetGatewayClient::new(&mockito::server_url(), None, NODE_VERSION, get_test_config())
            .unwrap();
    let tx_json_value = read_json_file("writer/invoke.json");
    let tx = serde_json::from_value::<Transaction>(tx_json_value.clone()).unwrap();
    let response_json_value = read_json_file("writer/invoke_response.json");
    let mock_add_transaction = mock("POST", "/gateway/add_transaction")
        .match_body(Matcher::Json(tx_json_value))
        .with_status(200)
        .with_body(serde_json::to_string(&response_json_value).unwrap())
        .create();
    let expected_response =
        serde_json::from_value::<AddTransactionResponse>(response_json_value).unwrap();
    assert_eq!(expected_response, client.add_transaction(tx).await.unwrap());
    mock_add_transaction.assert();
}
