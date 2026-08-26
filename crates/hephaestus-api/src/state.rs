//! Shared application state for the HTTP serving layer.
//!
//! [`AppState`] holds the inference pipeline, readiness flag, and
//! runtime metadata. It is wrapped in [`Arc`] and passed to every
//! axum handler via the [`State`](axum::extract::State) extractor.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use hephaestus_core::PipelineKind;
use metrics_exporter_prometheus::PrometheusHandle;
use tokio::sync::Mutex;

/// Shared application state for all HTTP handlers.
///
/// Constructed once at startup in the binary crate and shared via
/// `Arc<AppState>` through axum's state extractor. Fields are private
/// to enforce the Ousterhout deep-module principle -- callers interact
/// through controlled accessors rather than reaching into internals.
pub struct AppState {
    /// The inference pipeline, guarded by a tokio Mutex
    /// because [`PipelineKind::execute`] requires `&mut self`.
    pipeline: Mutex<PipelineKind>,

    /// Readiness flag. Starts `false`; flipped to `true` after the
    /// warmup inference pass succeeds (D-05). On SIGTERM, flipped
    /// back to `false` so the readiness probe returns 503 (D-07).
    ready: AtomicBool,

    /// Model identifier from configuration (e.g., `distilbert-base-uncased-finetuned-sst-2-english`).
    model_id: String,

    /// Process start time, used to compute `uptime_s` in health responses (D-06).
    start_time: Instant,

    /// Per-request inference timeout duration (D-12, CORE-04).
    request_timeout: Duration,

    /// Prometheus metrics handle for rendering `/metrics` endpoint (OBSV-01).
    metrics_handle: PrometheusHandle,
}

impl AppState {
    /// Construct new application state.
    pub fn new(
        pipeline: PipelineKind,
        model_id: String,
        request_timeout: Duration,
        metrics_handle: PrometheusHandle,
    ) -> Self {
        Self {
            pipeline: Mutex::new(pipeline),
            ready: AtomicBool::new(false),
            model_id,
            start_time: Instant::now(),
            request_timeout,
            metrics_handle,
        }
    }

    /// Check whether the service is ready to accept inference requests.
    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::SeqCst)
    }

    /// Set the readiness flag (e.g., after warmup or on shutdown).
    pub fn set_ready(&self, val: bool) {
        self.ready.store(val, Ordering::SeqCst);
    }

    /// The model identifier from configuration.
    pub fn model_id(&self) -> &str {
        &self.model_id
    }

    /// Seconds since process start (for health probe responses).
    pub fn uptime_secs(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }

    /// Per-request inference timeout duration.
    pub fn request_timeout(&self) -> Duration {
        self.request_timeout
    }

    /// Render Prometheus exposition text for the `/metrics` endpoint.
    pub fn render_metrics(&self) -> String {
        self.metrics_handle.render()
    }

    /// Acquire an exclusive lock on the inference pipeline.
    pub async fn lock_pipeline(&self) -> tokio::sync::MutexGuard<'_, PipelineKind> {
        self.pipeline.lock().await
    }

    /// Check whether dynamic batching is enabled (stub for Plan 03).
    pub fn is_batching_enabled(&self) -> bool {
        false
    }
}
