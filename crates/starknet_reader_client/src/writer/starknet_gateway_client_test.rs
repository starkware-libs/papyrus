use std::fmt::Debug;

use mockito::{mock, Matcher};
use serde::{Deserialize, Serialize};
use test_utils::read_json_file;

use crate::test_utils::retry::get_test_config;
use crate::writer::objects::response::{DeclareResponse, DeployAccountResponse, InvokeResponse};
use crate::writer::objects::transaction::{
    DeclareTransaction, DeployAccountTransaction, InvokeTransaction,
};
use crate::writer::{StarknetGatewayClient, StarknetWriter};

const NODE_VERSION: &str = "NODE VERSION";

async fn test_add_transaction<
    Transaction: Serialize + for<'a> Deserialize<'a>,
    Response: for<'a> Deserialize<'a> + Debug + Eq,
>(
    resource_file_transaction_path: &str,
    resource_file_response_path: &str,
) {
    let client =
        StarknetGatewayClient::new(&mockito::server_url(), None, NODE_VERSION, get_test_config())
            .unwrap();
    let tx_json_value = read_json_file(resource_file_transaction_path);
    let tx = serde_json::from_value::<Transaction>(tx_json_value.clone()).unwrap();
    let response_json_value = read_json_file(resource_file_response_path);
    let mock_add_transaction = mock("POST", "/gateway/add_transaction")
        .match_body(Matcher::Json(tx_json_value))
        .with_status(200)
        .with_body(serde_json::to_string(&response_json_value).unwrap())
        .create();
    let expected_response = serde_json::from_value::<Response>(response_json_value).unwrap();
    assert_eq!(expected_response, client.add_transaction(tx).await.unwrap());
    mock_add_transaction.assert();
}

#[tokio::test]
async fn add_invoke_transaction() {
    test_add_transaction::<InvokeTransaction, InvokeResponse>(
        "writer/invoke.json",
        "writer/invoke_response.json",
    )
    .await;
}

#[tokio::test]
async fn add_declare_v1_transaction() {
    test_add_transaction::<DeclareTransaction, DeclareResponse>(
        "writer/declare_v1.json",
        "writer/declare_response.json",
    )
    .await;
}

#[tokio::test]
async fn add_declare_v2_transaction() {
    test_add_transaction::<DeclareTransaction, DeclareResponse>(
        "writer/declare_v2.json",
        "writer/declare_response.json",
    )
    .await;
}

#[tokio::test]
async fn add_deploy_account_transaction() {
    test_add_transaction::<DeployAccountTransaction, DeployAccountResponse>(
        "writer/deploy_account.json",
        "writer/deploy_account_response.json",
    )
    .await;
}
