use std::fmt::Debug;
use std::future::Future;

use mockito::{mock, Matcher};
use serde::{Deserialize, Serialize};
use test_utils::read_json_file;

use crate::test_utils::retry::get_test_config;
use crate::writer::{StarknetGatewayClient, StarknetWriter, WriterClientError, WriterClientResult};

const NODE_VERSION: &str = "NODE VERSION";
const GATEWAY_ALIVE_RESPONSE: &str = "Gateway is alive!";

async fn run_add_transaction<
    Transaction: Serialize + for<'a> Deserialize<'a>,
    Response: for<'a> Deserialize<'a> + Debug + Eq,
    F: FnOnce(StarknetGatewayClient, Transaction) -> Fut,
    Fut: Future<Output = Result<Response, WriterClientError>>,
>(
    resource_file_transaction_path: &str,
    resource_file_response_path: &str,
    add_transaction_function: F,
) -> WriterClientResult<Response> {
    let client =
        StarknetGatewayClient::new(&mockito::server_url(), NODE_VERSION, get_test_config())
            .unwrap();
    let tx_json_value = read_json_file(resource_file_transaction_path);
    let tx = serde_json::from_value::<Transaction>(tx_json_value.clone()).unwrap();
    let response_json_value = read_json_file(resource_file_response_path);
    let mock_add_transaction = mock("POST", "/gateway/add_transaction")
        .match_body(Matcher::Json(tx_json_value))
        .with_status(200)
        .with_body(serde_json::to_string(&response_json_value).unwrap())
        .create();
    let result = add_transaction_function(client, tx).await;
    mock_add_transaction.assert();
    result
}

async fn test_add_transaction_succeeds<
    Transaction: Serialize + for<'a> Deserialize<'a>,
    Response: for<'a> Deserialize<'a> + Debug + Eq,
    F: FnOnce(StarknetGatewayClient, Transaction) -> Fut,
    Fut: Future<Output = Result<Response, WriterClientError>>,
>(
    resource_file_transaction_path: &str,
    resource_file_response_path: &str,
    add_transaction_function: F,
) {
    let response_json_value = read_json_file(resource_file_response_path);
    let expected_response = serde_json::from_value::<Response>(response_json_value).unwrap();
    assert_eq!(
        expected_response,
        run_add_transaction(
            resource_file_transaction_path,
            resource_file_response_path,
            add_transaction_function
        )
        .await
        .unwrap()
    );
}

async fn test_add_transaction_fails_serde<
    Transaction: Serialize + for<'a> Deserialize<'a>,
    Response: for<'a> Deserialize<'a> + Debug + Eq,
    F: FnOnce(StarknetGatewayClient, Transaction) -> Fut,
    Fut: Future<Output = Result<Response, WriterClientError>>,
>(
    resource_file_transaction_path: &str,
    resource_file_response_path: &str,
    add_transaction_function: F,
) {
    let Err(WriterClientError::SerdeError(_)) = run_add_transaction(
        resource_file_transaction_path,
        resource_file_response_path,
        add_transaction_function,
    )
    .await
    else {
        panic!("Adding a transaction with bad response did not cause a SerdeError");
    };
}

#[tokio::test]
async fn is_alive() {
    let starknet_client =
        StarknetGatewayClient::new(&mockito::server_url(), NODE_VERSION, get_test_config())
            .unwrap();
    let mock_is_alive = mock("GET", "/gateway/is_alive")
        .with_status(200)
        .with_body(GATEWAY_ALIVE_RESPONSE)
        .create();
    let response = starknet_client.is_alive().await;
    mock_is_alive.assert();
    assert!(response);
}

#[tokio::test]
async fn add_invoke_v1_transaction() {
    test_add_transaction_succeeds(
        "writer/invoke_v1.json",
        "writer/invoke_response.json",
        |client, tx| async move { client.add_invoke_transaction(&tx).await },
    )
    .await;
}

#[tokio::test]
async fn add_invoke_v3_transaction() {
    test_add_transaction_succeeds(
        "writer/invoke_v3.json",
        "writer/invoke_response.json",
        |client, tx| async move { client.add_invoke_transaction(&tx).await },
    )
    .await;
}

#[tokio::test]
async fn add_declare_v1_transaction() {
    test_add_transaction_succeeds(
        "writer/declare_v1.json",
        "writer/declare_response.json",
        |client, tx| async move { client.add_declare_transaction(&tx).await },
    )
    .await;
}

#[tokio::test]
async fn add_declare_v2_transaction() {
    test_add_transaction_succeeds(
        "writer/declare_v2.json",
        "writer/declare_response.json",
        |client, tx| async move { client.add_declare_transaction(&tx).await },
    )
    .await;
}

#[tokio::test]
async fn add_declare_v3_transaction() {
    test_add_transaction_succeeds(
        "writer/declare_v3.json",
        "writer/declare_response.json",
        |client, tx| async move { client.add_declare_transaction(&tx).await },
    )
    .await;
}

#[tokio::test]
async fn add_deploy_account_v1_transaction() {
    test_add_transaction_succeeds(
        "writer/deploy_account_v1.json",
        "writer/deploy_account_response.json",
        |client, tx| async move { client.add_deploy_account_transaction(&tx).await },
    )
    .await;
}

#[tokio::test]
async fn add_deploy_account_v3_transaction() {
    test_add_transaction_succeeds(
        "writer/deploy_account_v3.json",
        "writer/deploy_account_response.json",
        |client, tx| async move { client.add_deploy_account_transaction(&tx).await },
    )
    .await;
}

#[tokio::test]
async fn add_invoke_v1_transaction_wrong_type_response() {
    for bad_response_path in ["writer/declare_response.json", "writer/deploy_account_response.json"]
    {
        test_add_transaction_fails_serde(
            "writer/invoke_v1.json",
            bad_response_path,
            |client, tx| async move { client.add_invoke_transaction(&tx).await },
        )
        .await;
    }
}

#[tokio::test]
async fn add_declare_v1_transaction_wrong_type_response() {
    for bad_response_path in ["writer/invoke_response.json", "writer/deploy_account_response.json"]
    {
        test_add_transaction_fails_serde(
            "writer/declare_v1.json",
            bad_response_path,
            |client, tx| async move { client.add_declare_transaction(&tx).await },
        )
        .await;
    }
}

#[tokio::test]
async fn add_declare_v2_transaction_wrong_type_response() {
    for bad_response_path in ["writer/invoke_response.json", "writer/deploy_account_response.json"]
    {
        test_add_transaction_fails_serde(
            "writer/declare_v2.json",
            bad_response_path,
            |client, tx| async move { client.add_declare_transaction(&tx).await },
        )
        .await;
    }
}

#[tokio::test]
async fn add_deploy_account_transaction_wrong_type_response() {
    for bad_response_path in ["writer/invoke_response.json", "writer/declare_response.json"] {
        test_add_transaction_fails_serde(
            "writer/deploy_account_v1.json",
            bad_response_path,
            |client, tx| async move { client.add_deploy_account_transaction(&tx).await },
        )
        .await;
    }
}
