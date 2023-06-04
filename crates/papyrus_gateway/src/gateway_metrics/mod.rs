#[cfg(test)]
mod gateway_metrics_test;

use std::net::SocketAddr;
use std::time::Instant;

use jsonrpsee::server::logger::{HttpRequest, Logger, MethodKind, TransportProtocol};
use jsonrpsee::types::Params;
use metrics::{histogram, increment_counter};

const METHOD_LABEL: &str = "method";
const VERSION_LABEL: &str = "version";

#[derive(Clone)]
pub(crate) struct MetricLogger;

impl Logger for MetricLogger {
    type Instant = Instant;

    // Required methods
    fn on_connect(&self, _remote_addr: SocketAddr, _request: &HttpRequest, _t: TransportProtocol) {}
    fn on_request(&self, _transport: TransportProtocol) -> Self::Instant {
        Instant::now()
    }
    fn on_call(
        &self,
        _method_name: &str,
        _params: Params<'_>,
        _kind: MethodKind,
        _transport: TransportProtocol,
    ) {
    }
    fn on_result(
        &self,
        method_name: &str,
        success: bool,
        started_at: Self::Instant,
        _transport: TransportProtocol,
    ) {
        if success {
            let (method, version) = get_method_and_version(method_name);
            increment_counter!("gateway_incoming_requests", METHOD_LABEL=> method.clone(), VERSION_LABEL=> version.clone());
            let latency = started_at.elapsed().as_secs_f64();
            histogram!("gateway_request_latency_seconds", latency,METHOD_LABEL=> method, VERSION_LABEL=> version);
        } else {
            increment_counter!("gateway_failed_requests");
        }
    }
    fn on_response(
        &self,
        _result: &str,
        _started_at: Self::Instant,
        _transport: TransportProtocol,
    ) {
    }
    fn on_disconnect(&self, _remote_addr: SocketAddr, _transport: TransportProtocol) {}
}

// Given method_name returns (method, version).
// Example: method_name: starknet_V0_3_0_blockNumber; output: (blockNumber, V0_3_0).
fn get_method_and_version(method_name: &str) -> (String, String) {
    let last_underscore_index = method_name
        .rfind('_')
        .expect("method_name should be in the following format: starknet_V0_3_0_blockNumber");
    // The structure of method_name is in the following format: "starknet_V0_3_0_blockNumber".
    (
        method_name[last_underscore_index + 1..].to_string(),
        method_name[9..last_underscore_index].to_string(),
    )
}
