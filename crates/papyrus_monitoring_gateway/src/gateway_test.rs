use std::net::{SocketAddr, TcpListener};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::response::Response;
use axum::Router;
use http_body::combinators::UnsyncBoxBody;
use metrics::{absolute_counter, describe_counter, register_counter};
use metrics_exporter_prometheus::PrometheusBuilder;
use papyrus_storage::{table_names, test_utils};
use pretty_assertions::assert_eq;
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::{app, MONITORING_PREFIX};

const TEST_CONFIG_REPRESENTATION: &str = "full_general_config_representation";
const PUBLIC_TEST_CONFIG_REPRESENTATION: &str = "public_general_config_representation";
const SECRET: &str = "abcd";
const TEST_VERSION: &str = "1.2.3-dev";

// TODO(dan): consider using a proper fixture.
fn setup_app() -> Router {
    let ((storage_reader, _), _temp_dir) = test_utils::get_test_storage();
    app(
        storage_reader,
        TEST_VERSION,
        serde_json::to_value(TEST_CONFIG_REPRESENTATION).unwrap(),
        serde_json::to_value(PUBLIC_TEST_CONFIG_REPRESENTATION).unwrap(),
        SECRET.to_string(),
        None,
    )
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

async fn validate_response(request: &str, expected_response: &str) {
    let app = setup_app();
    let response = request_app(app, request).await;

    assert_eq!(response.status(), StatusCode::OK);

    let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body, json!(expected_response));
}

#[tokio::test]
async fn public_node_config() {
    validate_response("nodeConfig", PUBLIC_TEST_CONFIG_REPRESENTATION).await;
}

#[tokio::test]
async fn node_config_valid_secret() {
    validate_response(format!("nodeConfigFull/{SECRET}").as_str(), TEST_CONFIG_REPRESENTATION)
        .await;
}

#[tokio::test]
async fn node_config_invalid_secret() {
    let app = setup_app();
    let response = request_app(app, "nodeConfigFull/zzz".to_string().as_str()).await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn alive() {
    let app = setup_app();
    let response = request_app(app, "alive").await;

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn ready() {
    let app = setup_app();
    let response = request_app(app, "ready").await;

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn without_metrics() {
    let app = setup_app();
    let response = request_app(app, "metrics").await;

    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    let body_bytes = hyper::body::to_bytes(response.into_body()).await.unwrap();
    assert!(body_bytes.is_empty());
}

#[tokio::test]
async fn with_metrics() {
    // Creates an app with prometheus handle.
    let ((storage_reader, _), _temp_dir) = test_utils::get_test_storage();
    let prometheus_handle = PrometheusBuilder::new().install_recorder().unwrap();
    let app = app(
        storage_reader,
        TEST_VERSION,
        serde_json::Value::default(),
        serde_json::Value::default(),
        String::new(),
        Some(prometheus_handle),
    );

    // Register a metric.
    let metric_name = "metric_name";
    let metric_help = "metric_help";
    let metric_value = 8224;
    register_counter!(metric_name);
    describe_counter!(metric_name, metric_help);
    absolute_counter!(metric_name, metric_value);

    let response = request_app(app, "metrics").await;

    assert_eq!(response.status(), StatusCode::OK);

    let body_bytes = hyper::body::to_bytes(response.into_body()).await.unwrap();
    let body_string = String::from_utf8(body_bytes.to_vec()).unwrap();
    let expected_prefix = format!(
        "# HELP {metric_name} {metric_help}\n# TYPE {metric_name} counter\n{metric_name} \
         {metric_value}\n\n"
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

#[test]
fn serialization_precision() {
    let input =
        "{\"value\":244116128358498188146337218061232635775543270890529169229936851982759783745}";
    let serialized = serde_json::from_str::<serde_json::Value>(input).unwrap();
    let deserialized = serde_json::to_string(&serialized).unwrap();
    assert_eq!(input, deserialized);
}
