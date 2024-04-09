use std::sync::Arc;
use std::time::Instant;

use jsonrpsee::server::logger::{Logger, TransportProtocol};
use jsonrpsee::Methods;
use metrics_exporter_prometheus::PrometheusBuilder;
use papyrus_storage::body::BodyStorageWriter;
use papyrus_storage::class::ClassStorageWriter;
use papyrus_storage::header::HeaderStorageWriter;
use papyrus_storage::state::StateStorageWriter;
use papyrus_storage::test_utils::get_test_storage;
use pretty_assertions::assert_eq;
use prometheus_parse::Value::Counter;
use starknet_api::block::{BlockBody, BlockHeader, BlockNumber};
use starknet_api::state::ThinStateDiff;
use test_utils::{prometheus_is_contained, send_request};

use crate::rpc_metrics::{
    get_method_and_version,
    MetricLogger,
    FAILED_REQUESTS,
    ILLEGAL_METHOD,
    INCOMING_REQUEST,
    METHOD_LABEL,
    VERSION_LABEL,
};
use crate::run_server;
use crate::test_utils::{
    get_test_highest_block,
    get_test_pending_classes,
    get_test_pending_data,
    get_test_rpc_config,
};

#[test]
fn get_method_and_version_test() {
    let method_name = "starknet_V0_6_0_blockNumber";
    let (method, version) = get_method_and_version(method_name);
    assert_eq!(method, "blockNumber");
    assert_eq!(version, "V0_6_0");
}

// Ignored because server_metrics test is running in parallel and we are unable to install multiple
// recorders.
#[ignore]
#[test]
fn logger_test() {
    let full_method_name = "starknet_V0_6_0_blockNumber";
    let (method, version) = get_method_and_version(full_method_name);
    let labels = vec![(METHOD_LABEL, method.as_str()), (VERSION_LABEL, version.as_str())];
    let illegal_method_label = vec![(METHOD_LABEL, ILLEGAL_METHOD)];
    let handle = PrometheusBuilder::new().install_recorder().unwrap();
    let callback = jsonrpsee::MethodCallback::Unsubscription(Arc::new(|_, _, _, _| {
        jsonrpsee::MethodResponse {
            result: String::new(),
            success_or_error: jsonrpsee::helpers::MethodResponseResult::Success,
        }
    }));
    let mut methods = Methods::new();
    methods.verify_and_insert(full_method_name, callback).unwrap();
    let logger = MetricLogger::new(&methods);

    // The counters are initialized with zero.
    assert_eq!(
        prometheus_is_contained(handle.render(), INCOMING_REQUEST, &labels),
        Some(Counter(0f64))
    );
    assert_eq!(
        prometheus_is_contained(handle.render(), INCOMING_REQUEST, &illegal_method_label),
        Some(Counter(0f64))
    );
    assert_eq!(
        prometheus_is_contained(handle.render(), FAILED_REQUESTS, &labels),
        Some(Counter(0f64))
    );
    assert_eq!(
        prometheus_is_contained(handle.render(), FAILED_REQUESTS, &illegal_method_label),
        Some(Counter(0f64))
    );

    // Successful call.
    logger.on_result(
        full_method_name,
        jsonrpsee::helpers::MethodResponseResult::Success,
        Instant::now(),
        TransportProtocol::Http,
    );
    assert_eq!(
        prometheus_is_contained(handle.render(), INCOMING_REQUEST, &labels),
        Some(Counter(1f64))
    );
    assert_eq!(
        prometheus_is_contained(handle.render(), FAILED_REQUESTS, &labels),
        Some(Counter(0f64))
    );
    assert_eq!(
        prometheus_is_contained(handle.render(), FAILED_REQUESTS, &illegal_method_label),
        Some(Counter(0f64))
    );

    // Failed call.
    logger.on_result(
        full_method_name,
        jsonrpsee::helpers::MethodResponseResult::Failed(0),
        Instant::now(),
        TransportProtocol::Http,
    );
    assert_eq!(
        prometheus_is_contained(handle.render(), INCOMING_REQUEST, &labels),
        Some(Counter(2f64))
    );
    assert_eq!(
        prometheus_is_contained(handle.render(), FAILED_REQUESTS, &labels),
        Some(Counter(1f64))
    );
    assert_eq!(
        prometheus_is_contained(handle.render(), FAILED_REQUESTS, &illegal_method_label),
        Some(Counter(0f64))
    );

    // Illegal method.
    let bad_method_name = "starknet_V0_6_0_illegal_method";
    let (method, version) = get_method_and_version(bad_method_name);
    let bad_labels = vec![(METHOD_LABEL, method.as_str()), (VERSION_LABEL, version.as_str())];
    logger.on_result(
        bad_method_name,
        jsonrpsee::helpers::MethodResponseResult::Failed(0),
        Instant::now(),
        TransportProtocol::Http,
    );
    assert_eq!(prometheus_is_contained(handle.render(), INCOMING_REQUEST, &bad_labels), None);
    assert_eq!(
        prometheus_is_contained(handle.render(), INCOMING_REQUEST, &illegal_method_label),
        Some(Counter(1f64))
    );
    assert_eq!(prometheus_is_contained(handle.render(), FAILED_REQUESTS, &bad_labels), None);
    assert_eq!(
        prometheus_is_contained(handle.render(), FAILED_REQUESTS, &illegal_method_label),
        Some(Counter(1f64))
    );
}

#[tokio::test]
async fn server_metrics() {
    let prometheus_handle = PrometheusBuilder::new().install_recorder().unwrap();

    // Run the server.
    let ((storage_reader, mut storage_writer), _temp_dir) = get_test_storage();
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(BlockNumber(0), &BlockHeader::default())
        .unwrap()
        .append_body(BlockNumber(0), BlockBody::default())
        .unwrap()
        .append_thin_state_diff(BlockNumber(0), ThinStateDiff::default())
        .unwrap()
        .append_classes(BlockNumber(0), &vec![], &vec![])
        .unwrap()
        .commit()
        .unwrap();
    let mut gateway_config = get_test_rpc_config();
    gateway_config.collect_metrics = true;
    let (server_address, _handle) = run_server(
        &gateway_config,
        get_test_highest_block(),
        get_test_pending_data(),
        get_test_pending_classes(),
        storage_reader,
        "NODE VERSION",
    )
    .await
    .unwrap();

    let get_counters = || {
        let mut incoming_block_number = String::new();
        let mut failing_block_number = String::new();
        let mut incoming_get_state_update = String::new();
        let mut failing_get_state_update = String::new();
        let metrics = prometheus_handle.render();
        for line in metrics.split('\n').filter(|line| line.contains("V0_6")) {
            if line.contains("rpc_incoming_requests{method=\"blockNumber\"") {
                println!("{}", line);
                incoming_block_number = line.split(' ').last().unwrap().to_owned();
            }
            if line.contains("rpc_failed_requests{method=\"blockNumber\"") {
                println!("{}", line);
                failing_block_number = line.split(' ').last().unwrap().to_owned();
            }
            if line.contains("rpc_incoming_requests{method=\"getStateUpdate\"") {
                println!("{}", line);
                incoming_get_state_update = line.split(' ').last().unwrap().to_owned();
            }
            if line.contains("rpc_failed_requests{method=\"getStateUpdate\"") {
                println!("{}", line);
                failing_get_state_update = line.split(' ').last().unwrap().to_owned();
            }
        }
        (
            incoming_block_number,
            failing_block_number,
            incoming_get_state_update,
            failing_get_state_update,
        )
    };

    let (
        incoming_block_number,
        failing_block_number,
        incoming_get_state_update,
        failing_get_state_update,
    ) = get_counters();

    assert_eq!(incoming_block_number, "0");
    assert_eq!(failing_block_number, "0");
    assert_eq!(incoming_get_state_update, "0");
    assert_eq!(failing_get_state_update, "0");

    send_request(server_address, "starknet_blockNumber", "", "V0_6").await;
    send_request(server_address, "starknet_getStateUpdate", r#"{"block_number": 7}"#, "V0_6").await;

    let (
        incoming_block_number,
        failing_block_number,
        incoming_get_state_update,
        failing_get_state_update,
    ) = get_counters();

    assert_eq!(incoming_block_number, "1");
    assert_eq!(failing_block_number, "0");
    assert_eq!(incoming_get_state_update, "1");
    assert_eq!(failing_get_state_update, "1");
}
