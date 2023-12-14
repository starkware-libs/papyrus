#[cfg(test)]
mod rpc_metrics_test;

use std::collections::HashSet;
use std::net::SocketAddr;
use std::time::Instant;

use jsonrpsee::server::logger::{HttpRequest, Logger, MethodKind, TransportProtocol};
use jsonrpsee::types::Params;
use jsonrpsee::Methods;
use metrics::{histogram, increment_counter, register_counter, register_histogram};

// Name of the metrics.
const INCOMING_REQUEST: &str = "rpc_incoming_requests";
const FAILED_REQUESTS: &str = "rpc_failed_requests";
const REQUEST_LATENCY: &str = "rpc_request_latency_seconds";

// Labels for the metrics.
const METHOD_LABEL: &str = "method";
const VERSION_LABEL: &str = "version";
const ILLEGAL_METHOD: &str = "illegal_method";

// Register the metrics and returns a set of the method names.
fn init_metrics(methods: &Methods) -> HashSet<String> {
    let mut methods_set: HashSet<String> = HashSet::new();
    register_counter!(INCOMING_REQUEST, METHOD_LABEL => ILLEGAL_METHOD);
    register_counter!(FAILED_REQUESTS, METHOD_LABEL => ILLEGAL_METHOD);
    for method in methods.method_names() {
        methods_set.insert(method.to_string());
        let (method_name, version) = get_method_and_version(method);
        register_counter!(FAILED_REQUESTS, METHOD_LABEL => method_name.clone(), VERSION_LABEL => version.clone());
        register_counter!(INCOMING_REQUEST, METHOD_LABEL => method_name.clone(), VERSION_LABEL => version.clone());
        register_histogram!(REQUEST_LATENCY, METHOD_LABEL => method_name, VERSION_LABEL => version);
    }
    methods_set
}
#[derive(Clone)]
pub(crate) struct MetricLogger {
    // A set of all the method names the node support.
    methods_set: HashSet<String>,
}

impl MetricLogger {
    pub(crate) fn new(methods: &Methods) -> Self {
        let methods_set = init_metrics(methods);
        Self { methods_set }
    }
}

impl Logger for MetricLogger {
    type Instant = Instant;

    fn on_result(
        &self,
        method_name: &str,
        success_or_error: jsonrpsee::helpers::MethodResponseResult,
        started_at: Self::Instant,
        _transport: TransportProtocol,
    ) {
        // To prevent creating metrics for illegal methods.
        if self.methods_set.contains(method_name) {
            let (method, version) = get_method_and_version(method_name);
            if let jsonrpsee::helpers::MethodResponseResult::Failed(_) = success_or_error {
                increment_counter!(FAILED_REQUESTS, METHOD_LABEL=> method.clone(), VERSION_LABEL=> version.clone());
            }
            increment_counter!(INCOMING_REQUEST, METHOD_LABEL=> method.clone(), VERSION_LABEL=> version.clone());
            let latency = started_at.elapsed().as_secs_f64();
            histogram!(REQUEST_LATENCY, latency,METHOD_LABEL=> method, VERSION_LABEL=> version);
        } else {
            increment_counter!(INCOMING_REQUEST, METHOD_LABEL => ILLEGAL_METHOD);
            increment_counter!(FAILED_REQUESTS, METHOD_LABEL => ILLEGAL_METHOD);
        }
    }

    #[cfg_attr(coverage_nightly, coverage_attribute)]
    fn on_request(&self, _transport: TransportProtocol) -> Self::Instant {
        Instant::now()
    }

    // Required methods.
    #[cfg_attr(coverage_nightly, coverage_attribute)]
    fn on_connect(&self, _remote_addr: SocketAddr, _request: &HttpRequest, _t: TransportProtocol) {}

    #[cfg_attr(coverage_nightly, coverage_attribute)]
    fn on_call(
        &self,
        _method_name: &str,
        _params: Params<'_>,
        _kind: MethodKind,
        _transport: TransportProtocol,
    ) {
    }

    #[cfg_attr(coverage_nightly, coverage_attribute)]
    fn on_response(
        &self,
        _result: &str,
        _started_at: Self::Instant,
        _transport: TransportProtocol,
    ) {
    }

    #[cfg_attr(coverage_nightly, coverage_attribute)]
    fn on_disconnect(&self, _remote_addr: SocketAddr, _transport: TransportProtocol) {}
}

// Given method_name returns (method, version).
// Example: method_name: starknet_V0_6_0_blockNumber; output: (blockNumber, V0_6_0).
fn get_method_and_version(method_name: &str) -> (String, String) {
    // The structure of method_name is in the following format: "starknet_V0_6_0_blockNumber".
    // Only method in this format will arrive to this point in the code.
    let last_underscore_index = method_name
        .rfind('_')
        .expect("method_name should be in the following format: starknet_V0_6_0_blockNumber");

    (
        method_name[last_underscore_index + 1..].to_string(),
        method_name[9..last_underscore_index].to_string(),
    )
}
