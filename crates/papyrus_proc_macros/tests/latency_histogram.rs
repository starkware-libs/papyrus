use metrics_exporter_prometheus::PrometheusBuilder;
use papyrus_proc_macros::latency_histogram;
use prometheus_parse::Value::Untyped;
use test_utils::prometheus_is_contained;

#[test]
fn latency_histogram_test() {
    #[latency_histogram("foo_histogram")]
    fn foo() {
        println!("foo");
    }

    let handle = PrometheusBuilder::new().install_recorder().unwrap();
    assert!(handle.render().is_empty());

    foo();

    assert_eq!(
        prometheus_is_contained(handle.render(), "foo_histogram_count", &[]),
        Some(Untyped(1.0))
    );
}
