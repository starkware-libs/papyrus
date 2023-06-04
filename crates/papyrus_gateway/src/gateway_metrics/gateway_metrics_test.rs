use std::time::Instant;

use jsonrpsee::server::logger::{Logger, TransportProtocol};
use lazy_static::lazy_static;
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};

use super::MetricLogger;
use crate::gateway_metrics::get_method_and_version;

// It is possible to install recorder only once in a process.
lazy_static! {
    static ref PROMETHEUS_HANDLE: PrometheusHandle =
        PrometheusBuilder::new().install_recorder().unwrap();
}

#[test]
fn get_method_and_version_test() {
    let method_name = "starknet_V0_3_0_blockNumber";
    let (method, version) = get_method_and_version(method_name);
    assert_eq!(method, "blockNumber");
    assert_eq!(version, "V0_3_0");
}

#[test]
fn logger_on_result_failed() {
    // Before the first failed request, the metric doesn’t exist.
    let metric_name = "gateway_failed_requests";
    assert!(!(*PROMETHEUS_HANDLE).render().contains(metric_name));

    let logger = MetricLogger {};
    let method_name = "starknet_V0_3_0_nonExistingMethod";
    logger.on_result(method_name, false, Instant::now(), TransportProtocol::Http);
    let (method, version) = get_method_and_version(method_name);

    // TODO(dvir): Find a better way to get the content of a metric.
    let expected_metric = format!("# TYPE {metric_name} counter\n{metric_name} 1\n");
    assert!((*PROMETHEUS_HANDLE).render().contains(&expected_metric));

    // We don’t want to add an incoming_request metric when the request fails.
    let metric_name = "gateway_incoming_requests";
    let not_expected = format!(
        "# TYPE {metric_name} counter\n
{metric_name}{{method={method},version={version}}}\n"
    );
    assert!(!(*PROMETHEUS_HANDLE).render().contains(&not_expected));
}

#[test]
fn logger_on_result_success() {
    // Before the first successful requests the metric don't exist.
    let metric_name = "gateway_incoming_requests";
    assert!(!(*PROMETHEUS_HANDLE).render().contains(metric_name));

    let logger = MetricLogger {};
    let method_name = "starknet_V0_3_0_blockNumber";
    logger.on_result(method_name, true, Instant::now(), TransportProtocol::Http);
    let (method, version) = get_method_and_version(method_name);

    let expected_metric = format!(
        "# TYPE {metric_name} counter\n{metric_name}{{method=\"{method}\",version=\"{version}\"}} \
         1\n"
    );
    assert!((*PROMETHEUS_HANDLE).render().contains(&expected_metric));
}
