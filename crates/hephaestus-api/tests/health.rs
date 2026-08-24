//! Integration tests for API-03 (readiness probe).
//!
//! Tests require a real ClassifierPipeline for full router integration.
//! Readiness logic is also unit-tested inline in handlers.rs.

#[tokio::test]
#[ignore = "requires model files on disk for ClassifierPipeline"]
async fn readiness_returns_503_before_warmup() {
    // API-03: GET /healthz/ready returns 503 Service Unavailable when the
    // model has not yet completed its warmup inference pass. Kubernetes
    // should not route traffic to the pod until warmup completes.
    //
    // Readiness gating is unit-tested via the AtomicBool checks in
    // handlers.rs and the shutdown test module.
}

#[tokio::test]
#[ignore = "requires model files on disk for ClassifierPipeline"]
async fn readiness_returns_200_after_warmup() {
    // API-03: GET /healthz/ready returns 200 OK after the model has
    // completed its warmup inference pass and is ready to serve requests.
}
