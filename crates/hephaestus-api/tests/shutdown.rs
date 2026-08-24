//! Integration test stubs for API-04 (graceful shutdown).
//!
//! These stubs are populated by plan 02-01 as the shutdown lifecycle is built.

#[tokio::test]
#[ignore = "pending 02-01 implementation"]
async fn sigterm_flips_readiness_to_503() {
    // API-04: When SIGTERM is received, the readiness endpoint immediately
    // starts returning 503 so that Kubernetes stops routing new traffic to
    // this pod, while the pod continues serving in-flight requests.
}

#[tokio::test]
#[ignore = "pending 02-01 implementation"]
async fn inflight_requests_drain_before_exit() {
    // API-04: After SIGTERM, the server waits for all in-flight requests to
    // complete (up to the configured drain timeout) before shutting down.
    // Requests that arrive after SIGTERM are rejected with 503.
}
