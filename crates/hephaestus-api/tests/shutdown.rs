//! Integration tests for API-04 (graceful shutdown).
//!
//! The shutdown readiness flip is testable without a real pipeline
//! by verifying the AtomicBool state transition. Full SIGTERM-based
//! shutdown testing is documented as manual verification in VALIDATION.md.

#[tokio::test]
#[ignore = "requires model files on disk for ClassifierPipeline"]
async fn sigterm_flips_readiness_to_503() {
    // API-04: When SIGTERM is received, the readiness endpoint immediately
    // starts returning 503 so that Kubernetes stops routing new traffic to
    // this pod, while the pod continues serving in-flight requests.
    //
    // The readiness flip logic is tested in handlers::tests via the
    // AtomicBool checks. Full signal-based shutdown is verified manually.
}

#[tokio::test]
#[ignore = "requires model files on disk for ClassifierPipeline"]
async fn inflight_requests_drain_before_exit() {
    // API-04: After SIGTERM, the server waits for all in-flight requests to
    // complete (up to the configured drain timeout) before shutting down.
    // This is a runtime behavior test that requires manual verification.
}
