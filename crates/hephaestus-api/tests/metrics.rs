//! Integration test stubs for OBSV-01 (Prometheus metrics) and CORE-04 (request timeout).
//!
//! OBSV-01 stubs are populated by plan 02-02 (observability stack).
//! CORE-04 stub is populated by plan 02-01 (HTTP serving layer).

#[tokio::test]
#[ignore = "pending 02-02 implementation"]
async fn metrics_endpoint_returns_prometheus_text() {
    // OBSV-01: GET /metrics returns Prometheus exposition format text
    // with Content-Type: text/plain. The response includes default
    // process metrics and any custom application metrics.
}

#[tokio::test]
#[ignore = "pending 02-02 implementation"]
async fn stage_duration_histograms_recorded_after_inference() {
    // OBSV-01: After an inference request completes, per-stage duration
    // histograms are recorded (tokenization, inference, post-processing).
    // These appear in the /metrics output with appropriate labels.
}

#[tokio::test]
#[ignore = "pending 02-01 implementation"]
async fn request_timeout_returns_504() {
    // CORE-04: When an inference request exceeds the configured timeout,
    // the server returns 504 Gateway Timeout. The timeout applies to the
    // entire request lifecycle (tokenization + inference + post-processing).
}
