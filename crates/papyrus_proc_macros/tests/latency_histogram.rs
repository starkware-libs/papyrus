use metrics_exporter_prometheus::PrometheusBuilder;
use papyrus_proc_macros::latency_histogram;
use prometheus_parse::Value::Untyped;
use test_utils::prometheus_is_contained;

#[test]
fn latency_histogram_test() {
    #[latency_histogram("foo_histogram")]
    fn foo() -> usize {
        #[allow(clippy::let_and_return)]
        let start_function_time = 1000;
        start_function_time
    }

    let handle = PrometheusBuilder::new().install_recorder().unwrap();

    assert!(handle.render().is_empty());
    assert_eq!(foo(), 1000);
    assert_eq!(
        prometheus_is_contained(handle.render(), "foo_histogram_count", &[]),
        Some(Untyped(1f64))
    );
    // Test that the "start_function_time" variable from the macro is not shadowed.
    assert_ne!(
        prometheus_is_contained(handle.render(), "foo_histogram_sum", &[]),
        Some(Untyped(1000f64))
    );
}
