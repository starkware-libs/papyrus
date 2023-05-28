use std::net::{SocketAddr, TcpListener};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use papyrus_storage::{table_names, test_utils};
use prometheus::core::AtomicU64;
use prometheus::Registry;
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::{app, MONITORING_PREFIX};

const TEST_CONFIG_REPRESENTATION: &str = "general_config_representation";
const TEST_VERSION: &str = "1.2.3-dev";
// For creating prometheus collector.
const COUNTER_NAME: &str = "name";
const COUNTER_HELP: &str = "help";

// TODO(dan): consider using a proper fixture.
fn setup_app() -> Router {
    let (storage_reader, _) = test_utils::get_test_storage();
    let counter =
        prometheus::core::GenericCounter::<AtomicU64>::new(COUNTER_NAME, COUNTER_HELP).unwrap();
    let registry = Registry::new();
    registry.register(Box::new(counter)).unwrap();
    app(
        storage_reader,
        TEST_VERSION,
        serde_json::to_value(TEST_CONFIG_REPRESENTATION).unwrap(),
        registry,
    )
}

#[tokio::test]
async fn db_stats() {
    let app = setup_app();
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/{MONITORING_PREFIX}/dbTablesStats").as_str())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

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
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/{MONITORING_PREFIX}/nodeVersion").as_str())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
    assert_eq!(&body[..], TEST_VERSION.as_bytes());
}

#[tokio::test]
async fn node_config() {
    let app = setup_app();
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/{MONITORING_PREFIX}/nodeConfig").as_str())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body, json!(TEST_CONFIG_REPRESENTATION));
}

#[tokio::test]
async fn alive() {
    let app = setup_app();
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/{MONITORING_PREFIX}/alive").as_str())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}
#[tokio::test]
async fn metrics() {
    let app = setup_app();
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/{MONITORING_PREFIX}/metrics").as_str())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body_bytes = hyper::body::to_bytes(response.into_body()).await.unwrap();
    let body_string = String::from_utf8(body_bytes.to_vec()).unwrap();
    // TODO(dvir): Find a better way to compare the response.
    let expected_prefix = format!(
        "# HELP {COUNTER_NAME} {COUNTER_HELP}\n# TYPE {COUNTER_NAME} counter\n{COUNTER_NAME} 0\n# \
         HELP process_cpu_seconds_total Total user and system CPU time spent in seconds.\n# TYPE \
         process_cpu_seconds_total counter\nprocess_cpu_seconds_total"
    );
    assert!(body_string.starts_with(&expected_prefix));
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
