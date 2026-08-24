---
phase: 02-http-serving-and-observability
reviewed: 2026-08-24T18:42:00Z
depth: standard
files_reviewed: 16
files_reviewed_list:
  - crates/hephaestus-api/Cargo.toml
  - crates/hephaestus-api/src/error.rs
  - crates/hephaestus-api/src/handlers.rs
  - crates/hephaestus-api/src/lib.rs
  - crates/hephaestus-api/src/metrics.rs
  - crates/hephaestus-api/src/routes.rs
  - crates/hephaestus-api/src/state.rs
  - crates/hephaestus-api/src/telemetry.rs
  - crates/hephaestus-api/tests/api.rs
  - crates/hephaestus-api/tests/health.rs
  - crates/hephaestus-api/tests/metrics.rs
  - crates/hephaestus-api/tests/shutdown.rs
  - crates/hephaestus-api/tests/tracing.rs
  - crates/hephaestus/Cargo.toml
  - crates/hephaestus/src/config.rs
  - crates/hephaestus/src/main.rs
findings:
  critical: 1
  warning: 5
  info: 2
  total: 8
status: issues_found
---

# Phase 02: Code Review Report

**Reviewed:** 2026-08-24T18:42:00Z
**Depth:** standard
**Files Reviewed:** 16
**Status:** issues_found

## Summary

Phase 02 adds HTTP serving (axum), health probes, graceful shutdown, Prometheus metrics, OpenTelemetry tracing, and structured JSON logging to the Hephaestus inference runtime. The implementation is generally well-structured with clear module separation, correct error mapping, proper path traversal mitigation, and a clean deep-module abstraction for metrics.

One critical bug was found: the OTLP endpoint parameter is silently discarded, causing the exporter to rely on env-var fallback behavior rather than the passed value. Five warnings cover a dropped log message, blocking CPU work on the async runtime, information disclosure in error responses, potential PII logging of request text, and excessively public struct fields. The integration test suite is mostly stub bodies with `#[ignore]`, providing minimal real coverage.

## Critical Issues

### CR-01: OTLP endpoint parameter silently discarded -- exporter ignores configured endpoint

**File:** `crates/hephaestus-api/src/telemetry.rs:48-52`
**Issue:** The `otel_endpoint` parameter is bound as `_endpoint` (underscore prefix = explicitly unused). The `SpanExporter::builder().with_tonic().build()` call never receives the endpoint value. The function's contract says it accepts an endpoint (`otel_endpoint: Option<&str>`) but silently ignores it. This works in production by accident because the opentelemetry-otlp SDK reads `OTEL_EXPORTER_OTLP_ENDPOINT` directly from the process environment, and `envy` reads the same env var into the config struct. However, any caller passing an endpoint programmatically (tests, alternate config sources) will have their endpoint silently ignored, sending spans to the SDK's default (`http://localhost:4317`) instead.
**Fix:**
```rust
let otel_layer = if let Some(endpoint) = otel_endpoint {
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint)
        .build()
        .map_err(|e| anyhow::anyhow!("failed to build OTLP span exporter: {e}"))?;
```

## Warnings

### WR-01: "OTel export enabled" log emitted before subscriber is installed

**File:** `crates/hephaestus-api/src/telemetry.rs:65`
**Issue:** `tracing::info!("OpenTelemetry OTLP export enabled")` is called at line 65, before the tracing subscriber is installed at lines 71-75. Since no subscriber is registered at that point, the log message is silently dropped. The code already handles this correctly for the "disabled" case (line 79 logs after subscriber init), but the "enabled" case logs too early.
**Fix:** Move the log statement after the subscriber is installed, mirroring the pattern already used for the disabled path:
```rust
    tracing_subscriber::Registry::default()
        .with(env_filter)
        .with(fmt_layer)
        .with(otel_layer)
        .init();

    if otel_endpoint.is_some() {
        tracing::info!("OpenTelemetry OTLP export enabled");
    } else {
        tracing::info!("OpenTelemetry export disabled (OTEL_EXPORTER_OTLP_ENDPOINT not set)");
    }
```

### WR-02: Synchronous CPU-bound inference blocks the async runtime

**File:** `crates/hephaestus-api/src/handlers.rs:75-81`
**Issue:** `pipeline.prepare()` (tokenization) and `pipeline.execute()` (ONNX inference) are synchronous CPU-bound operations that run directly on the tokio runtime thread. During inference, the runtime thread is blocked, preventing it from servicing health probes (`/healthz/live`, `/healthz/ready`), processing shutdown signals, or handling the metrics endpoint. If inference is slow (common with larger models), Kubernetes liveness probes may time out and restart the pod. The `Pipeline::execute` trait method takes `&mut self` synchronously, so this is a design-level issue that may require `spawn_blocking` or restructuring.
**Fix:** Wrap the CPU-bound work in `tokio::task::spawn_blocking` to offload it from the async runtime:
```rust
let result = tokio::time::timeout(state.request_timeout, async {
    let mut pipeline = state.pipeline.lock().await;
    let text = req.text;
    let timer_clone = timer.model_id.clone();
    // Offload CPU-bound tokenization + inference to blocking thread pool
    tokio::task::spawn_blocking(move || {
        let stage_timer = StageTimer::new(timer_clone);
        let prepared = stage_timer.time("tokenization", || pipeline.prepare(text))?;
        let output = stage_timer.time("inference", || pipeline.execute(prepared))?;
        Ok::<_, ApiError>(output)
    }).await.map_err(|e| ApiError::Internal(e.to_string()))?
}).await;
```
Note: This requires `ClassifierPipeline` to be `Send`, and the `Mutex` guard interaction needs careful restructuring. A simpler interim fix is to ensure the tokio runtime has enough threads (via `worker_threads` config) that at least one thread remains available for health probes.

### WR-03: Internal error messages leak system details to HTTP clients

**File:** `crates/hephaestus-api/src/error.rs:54,69-76`
**Issue:** The `From<CoreError>` impl passes raw error messages through to the API response body. For example, `CoreError::Io(e) => Self::Internal(e.to_string())` includes the full IO error which may contain file paths like `/opt/models/tokenizer.json: No such file or directory`. The `IntoResponse` impl at line 72-76 then includes `self.to_string()` in the JSON response, exposing internal system paths and error details to HTTP clients. This is an information disclosure issue.
**Fix:** Log the detailed error server-side and return a generic message to clients:
```rust
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            // ... existing match arms ...
        };

        // Log the detailed error for debugging; return generic message to client.
        if status.is_server_error() {
            tracing::error!(error = %self, "request failed");
        }

        let client_message = match &self {
            Self::Internal(_) => "internal server error".to_string(),
            other => other.to_string(),
        };

        let body = serde_json::json!({
            "error": {
                "code": code,
                "message": client_message,
            }
        });

        (status, axum::Json(body)).into_response()
    }
}
```

### WR-04: Tracing instrument on infer handler records user input text in spans

**File:** `crates/hephaestus-api/src/handlers.rs:54-57`
**Issue:** The `#[tracing::instrument(skip(state))]` annotation skips `state` but not `req: Json<InferRequest>`. Since `InferRequest` derives `Debug`, the user's input text is recorded in the tracing span and will appear in structured JSON log output and OTel traces. For a classification service processing potentially sensitive text (customer messages, support tickets, medical notes), this creates a PII/data leak through the logging pipeline.
**Fix:** Add `req` to the skip list, or use `fields` to log only non-sensitive metadata:
```rust
#[tracing::instrument(skip(state, req), fields(text_len = req.text.len()))]
pub async fn infer(
    State(state): State<Arc<AppState>>,
    Json(req): Json<InferRequest>,
) -> Result<Json<InferResponse>, ApiError> {
```

### WR-05: All AppState fields are pub -- no encapsulation of safety-critical state

**File:** `crates/hephaestus-api/src/state.rs:18-38`
**Issue:** Every field of `AppState` is `pub`, including the `ready` AtomicBool (safety-critical for shutdown correctness) and `request_timeout` Duration. Any downstream code can mutate `ready` directly, bypassing the intended shutdown protocol. This violates the Ousterhout deep-module principle specified in the project's CLAUDE.md constraints. The `pipeline` mutex, `metrics_handle`, and `start_time` are also publicly exposed.
**Fix:** Make fields private and expose controlled accessors:
```rust
pub struct AppState {
    pipeline: Mutex<ClassifierPipeline>,
    ready: AtomicBool,
    model_id: String,
    start_time: Instant,
    request_timeout: Duration,
    metrics_handle: PrometheusHandle,
}

impl AppState {
    pub fn new(/* constructor params */) -> Self { /* ... */ }
    pub fn is_ready(&self) -> bool { self.ready.load(Ordering::SeqCst) }
    pub fn set_ready(&self, val: bool) { self.ready.store(val, Ordering::SeqCst) }
    pub fn model_id(&self) -> &str { &self.model_id }
    pub fn uptime_secs(&self) -> u64 { self.start_time.elapsed().as_secs() }
    pub fn request_timeout(&self) -> Duration { self.request_timeout }
    pub fn render_metrics(&self) -> String { self.metrics_handle.render() }
    pub async fn lock_pipeline(&self) -> tokio::sync::MutexGuard<'_, ClassifierPipeline> {
        self.pipeline.lock().await
    }
}
```

## Info

### IN-01: Integration test stubs with empty bodies provide no coverage

**File:** `crates/hephaestus-api/tests/api.rs:9-29`, `tests/health.rs:6-24`, `tests/shutdown.rs:7-21`
**Issue:** 8 of 11 integration tests have completely empty bodies -- no setup, no assertions, no function calls. They are all marked `#[ignore]` so they pass silently. While the comments describe what they would test, they provide zero test coverage and create a false impression of test breadth when listing test files. The test suite has only 3 real tests (1 in metrics.rs, 1 in tracing.rs, plus the unit tests inline in source files).
**Fix:** Either implement the tests with mock pipelines (since the handlers can be tested via `tower::ServiceExt::oneshot` without real model files), or remove the stubs and track the missing coverage as tech debt items. Mock-based tests are feasible for readiness gating, empty-text validation, and timeout behavior without requiring ONNX model files on disk.

### IN-02: `anyhow` used in library crate violates project error handling convention

**File:** `crates/hephaestus-api/Cargo.toml:14`, `crates/hephaestus-api/src/metrics.rs:28`, `crates/hephaestus-api/src/telemetry.rs:36`
**Issue:** The project's CLAUDE.md tech stack guidance specifies "`anyhow` -- Context-rich error propagation at the binary level. Use in `main()` and CLI, not in library traits." The `hephaestus-api` crate is a library crate, but `install_recorder()` and `telemetry::init()` return `Result<_, anyhow::Error>`. Per project convention, these should define typed errors via `thiserror`.
**Fix:** Define a `TelemetryError` enum in the api crate using `thiserror`, and return that instead of `anyhow::Error` from the library functions. The binary crate can still convert to `anyhow::Error` at the call site with `?` and `.context()`.

---

_Reviewed: 2026-08-24T18:42:00Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
