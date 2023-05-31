use std::net::{SocketAddr, TcpListener};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::response::Response;
use axum::Router;
use http_body::combinators::UnsyncBoxBody;
use papyrus_storage::{table_names, test_utils};
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::{app, MONITORING_PREFIX};

const TEST_CONFIG_REPRESENTATION: &str = "general_config_representation";
const TEST_VERSION: &str = "1.2.3-dev";

// TODO(dan): consider using a proper fixture.
fn setup_app() -> Router {
    let (storage_reader, _) = test_utils::get_test_storage();
    app(storage_reader, TEST_VERSION, serde_json::to_value(TEST_CONFIG_REPRESENTATION).unwrap())
}

async fn request_app(
    app: Router,
    method: &str,
) -> Response<UnsyncBoxBody<axum::body::Bytes, axum::Error>> {
    app.oneshot(
        Request::builder()
            .uri(format!("/{MONITORING_PREFIX}/{method}").as_str())
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn db_stats() {
    let app = setup_app();
    let response = request_app(app, "dbTablesStats").await;

    assert_eq!(response.status(), StatusCode::OK);

    let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
    let body: Value = serde_json::from_slice(&body).unwrap();
    for &name in table_names() {
        assert!(body["stats"].get(name).is_some(), "{name} is not found in returned DB statistics.")
    }
}

#[tokio::test]
async fn version() {
    let app = setup_app();
    let response = request_app(app, "nodeVersion").await;

    assert_eq!(response.status(), StatusCode::OK);

    let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
    assert_eq!(&body[..], TEST_VERSION.as_bytes());
}

#[tokio::test]
async fn node_config() {
    let app = setup_app();
    let response = request_app(app, "nodeConfig").await;

    assert_eq!(response.status(), StatusCode::OK);

    let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body, json!(TEST_CONFIG_REPRESENTATION));
}

#[tokio::test]
async fn alive() {
    let app = setup_app();
    let response = request_app(app, "alive").await;

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn run_server() {
    let listener = TcpListener::bind("0.0.0.0:0".parse::<SocketAddr>().unwrap()).unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::Server::from_tcp(listener)
            .unwrap()
            .serve(setup_app().into_make_service())
            .await
            .unwrap();
    });

    let client = hyper::Client::new();

    let response = client
        .request(
            Request::builder()
                .uri(format!("http://{addr}/{MONITORING_PREFIX}/nodeVersion"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}
