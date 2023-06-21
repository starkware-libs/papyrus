use std::net::{SocketAddr, TcpListener};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::response::Response;
use axum::Router;
use http_body::combinators::UnsyncBoxBody;
use metrics::{absolute_counter, describe_counter, register_counter};
use metrics_exporter_prometheus::PrometheusBuilder;
use papyrus_storage::{table_names, test_utils};
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::{app, MONITORING_PREFIX};

const TEST_CONFIG_REPRESENTATION: &str = "general_config_representation";
const TEST_VERSION: &str = "1.2.3-dev";

// TODO(dan): consider using a proper fixture.
fn setup_app() -> Router {
    let ((storage_reader, _), _temp_dir) = test_utils::get_test_storage();
    app(
        storage_reader,
        TEST_VERSION,
        serde_json::to_value(TEST_CONFIG_REPRESENTATION).unwrap(),
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
    let app =
        app(storage_reader, TEST_VERSION, serde_json::Value::default(), Some(prometheus_handle));

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
