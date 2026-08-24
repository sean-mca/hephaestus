//! Shared application state for the HTTP serving layer.
//!
//! [`AppState`] holds the inference pipeline, readiness flag, and
//! runtime metadata. It is wrapped in [`Arc`] and passed to every
//! axum handler via the [`State`](axum::extract::State) extractor.

use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use hephaestus_core::ClassifierPipeline;
use metrics_exporter_prometheus::PrometheusHandle;
use tokio::sync::Mutex;

/// Shared application state for all HTTP handlers.
///
/// Constructed once at startup in the binary crate and shared via
/// `Arc<AppState>` through axum's state extractor.
pub struct AppState {
    /// The classifier inference pipeline, guarded by a tokio Mutex
    /// because [`ClassifierPipeline::execute`] requires `&mut self`.
    pub pipeline: Mutex<ClassifierPipeline>,

    /// Readiness flag. Starts `false`; flipped to `true` after the
    /// warmup inference pass succeeds (D-05). On SIGTERM, flipped
    /// back to `false` so the readiness probe returns 503 (D-07).
    pub ready: AtomicBool,

    /// Model identifier from configuration (e.g., `distilbert-base-uncased-finetuned-sst-2-english`).
    pub model_id: String,

    /// Process start time, used to compute `uptime_s` in health responses (D-06).
    pub start_time: Instant,

    /// Per-request inference timeout duration (D-12, CORE-04).
    pub request_timeout: Duration,

    /// Prometheus metrics handle for rendering `/metrics` endpoint (OBSV-01).
    pub metrics_handle: PrometheusHandle,
}
