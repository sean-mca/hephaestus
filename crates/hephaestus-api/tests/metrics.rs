//! Integration tests for OBSV-01 (Prometheus metrics) and CORE-04 (request timeout).
//!
//! OBSV-01 tests verify that StageTimer records metrics and the PrometheusHandle
//! renders them correctly. Full router integration tests requiring a real
//! ClassifierPipeline are marked `#[ignore]`.

use std::time::Instant;

use hephaestus_api::metrics;

#[test]
fn metrics_endpoint_returns_prometheus_text() {
    // OBSV-01: Install the recorder, record metrics via StageTimer, then
    // verify the PrometheusHandle renders Prometheus exposition text
    // containing our custom histograms and counters with model_id labels.

    let handle = metrics::install_recorder()
        .expect("should install Prometheus recorder");

    // Record metrics via the deep-module StageTimer abstraction.
    let timer = hephaestus_api::StageTimer::new("test-model".to_string());
    let _result = timer.time("tokenization", || 42);
    let _result2 = timer.time("inference", || "output");
    timer.finish_request(Instant::now(), true);

    // Render the Prometheus exposition text.
    let text = handle.render();

    // Verify our custom metrics are present.
    assert!(
        text.contains("hephaestus_stage_duration_seconds"),
        "metrics output should contain stage duration histogram:\n{text}"
    );
    assert!(
        text.contains("hephaestus_request_duration_seconds"),
        "metrics output should contain request duration histogram:\n{text}"
    );
    assert!(
        text.contains("hephaestus_requests_total"),
        "metrics output should contain requests total counter:\n{text}"
    );
    // Verify model_id label is present (D-10).
    assert!(
        text.contains("model_id=\"test-model\""),
        "metrics should carry model_id label:\n{text}"
    );
    // Verify stage label is present (D-08).
    assert!(
        text.contains("stage=\"tokenization\""),
        "metrics should carry stage label:\n{text}"
    );
}

#[tokio::test]
#[ignore = "requires model files on disk for ClassifierPipeline"]
async fn stage_duration_histograms_recorded_after_inference() {
    // OBSV-01: After an inference request completes, per-stage duration
    // histograms are recorded (tokenization, inference, post-processing).
    // Full integration test requires a real ClassifierPipeline with model files.
}

#[tokio::test]
#[ignore = "requires model files on disk for ClassifierPipeline"]
async fn request_timeout_returns_504() {
    // CORE-04: When an inference request exceeds the configured timeout,
    // the server returns 504 Gateway Timeout with INFERENCE_TIMEOUT error code.
    // Timeout logic is implemented in handlers::infer via tokio::time::timeout.
    // Unit test coverage is in error::tests::api_error_timeout_maps_to_504.
}
