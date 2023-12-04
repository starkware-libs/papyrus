use std::sync::Arc;
use std::time::Instant;

use jsonrpsee::server::logger::{Logger, TransportProtocol};
use jsonrpsee::Methods;
use metrics_exporter_prometheus::PrometheusBuilder;
use prometheus_parse::Value::Counter;
use test_utils::prometheus_is_contained;

use crate::rpc_metrics::{
    get_method_and_version,
    MetricLogger,
    FAILED_REQUESTS,
    ILLEGAL_METHOD,
    INCOMING_REQUEST,
    METHOD_LABEL,
    VERSION_LABEL,
};

#[test]
fn get_method_and_version_test() {
    let method_name = "starknet_V0_6_0_blockNumber";
    let (method, version) = get_method_and_version(method_name);
    assert_eq!(method, "blockNumber");
    assert_eq!(version, "V0_6_0");
}

#[test]
fn logger_test() {
    let full_method_name = "starknet_V0_6_0_blockNumber";
    let (method, version) = get_method_and_version(full_method_name);
    let labels = vec![(METHOD_LABEL, method.as_str()), (VERSION_LABEL, version.as_str())];
    let illegal_method_label = vec![(METHOD_LABEL, ILLEGAL_METHOD)];
    let handle = PrometheusBuilder::new().install_recorder().unwrap();
    let callback = jsonrpsee::MethodCallback::Unsubscription(Arc::new(|_, _, _, _| {
        jsonrpsee::MethodResponse { result: String::new(), success: true }
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
    logger.on_result(full_method_name, true, Instant::now(), TransportProtocol::Http);
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
    logger.on_result(full_method_name, false, Instant::now(), TransportProtocol::Http);
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
    logger.on_result(bad_method_name, false, Instant::now(), TransportProtocol::Http);
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
