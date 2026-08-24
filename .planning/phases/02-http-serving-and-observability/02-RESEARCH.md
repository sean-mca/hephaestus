# Phase 2: HTTP Serving and Observability - Research

**Researched:** 2026-08-23
**Domain:** HTTP serving (axum), Prometheus metrics, OpenTelemetry tracing, graceful shutdown
**Confidence:** HIGH

## Summary

Phase 2 transforms the standalone Hephaestus binary into a deployable HTTP service with health probes, Prometheus metrics, structured JSON logging, and OpenTelemetry distributed tracing. The existing `ClassifierPipeline` from Phase 1 becomes the inference backend behind axum HTTP handlers, with a new `hephaestus-api` crate owning the serving layer.

The standard Rust stack for this domain is well-established: axum 0.8 for HTTP routing, tower-http for request timeout middleware, metrics + metrics-exporter-prometheus for Prometheus scrape endpoints, and the tracing + tracing-opentelemetry + opentelemetry-otlp stack for distributed tracing. All libraries are mature, actively maintained, and designed to compose via Tower's layer system. The main complexity lies in wiring them together correctly -- particularly the conditional OTel layer that activates only when `OTEL_EXPORTER_OTLP_ENDPOINT` is set, the readiness probe that gates on warmup completion, and the graceful shutdown sequence that flips readiness to 503 before draining.

**Primary recommendation:** Build a new `hephaestus-api` crate with axum routes, shared state holding the pipeline + readiness flag + metrics handle, tower-http TimeoutLayer for request timeouts, and a layered tracing subscriber with optional OTel export. The main binary becomes an async tokio entry point that constructs the pipeline, runs warmup, flips readiness, and starts the HTTP server with graceful shutdown.

<user_constraints>

## User Constraints (from CONTEXT.md)

### Locked Decisions
- **D-01:** Minimal flat JSON request body: `{"text": "input string"}`. Single field matches the one-model-per-pod design.
- **D-02:** Response includes model metadata: `{"label": "POSITIVE", "score": 0.95, "model_id": "distilbert-...", "latency_ms": 12}`. Aids debugging without requiring external correlation.
- **D-03:** Structured error responses with machine-parseable codes: `{"error": {"code": "TOKENIZATION_FAILED", "message": "..."}}`. HTTP status codes: 400 (bad request), 422 (unprocessable), 500 (internal), 503 (not ready), 504 (timeout).
- **D-04:** New `hephaestus-api` crate owns the HTTP layer (axum routes, handlers, middleware). Separates serving concerns from the binary crate.
- **D-05:** Readiness probe returns 200 only after the warmup inference pass succeeds. Liveness probe returns 200 immediately on startup.
- **D-06:** Health endpoints include diagnostics: `{"status": "ok", "model_id": "...", "uptime_s": 3600}`. K8s ignores the body but it's useful for operators hitting the endpoint directly.
- **D-07:** On SIGTERM, readiness flips to 503 immediately so k8s stops routing new traffic while in-flight requests drain.
- **D-08:** Per-stage timing breakdown: separate histograms for tokenization, inference, and postprocessing latency. Total request latency as a summary metric.
- **D-09:** Deep-module style timer abstraction -- a shared timing utility that each pipeline stage calls through the same interface, hiding the metrics recording plumbing. Callers should never interact with the metrics crate directly.
- **D-10:** All metrics carry a `model_id` label. One model per pod means one label value per pod -- cardinality is controlled.
- **D-11:** Full OTel wiring with conditional activation. The OTLP exporter layer registers only when `OTEL_EXPORTER_OTLP_ENDPOINT` is set. Without it, structured JSON logs still capture span/trace context via tracing-subscriber. No feature flags or if/else in hot paths -- just a layered subscriber with an optional OTel layer.
- **D-12:** Request timeout default: 30 seconds. Configurable via `REQUEST_TIMEOUT_SECS` env var.
- **D-13:** Graceful shutdown drain: 30 seconds. Configurable via `SHUTDOWN_TIMEOUT_SECS` env var. Matches the request timeout so any in-flight request finishes within one timeout window.
- **D-14:** Timeout responses: HTTP 504 Gateway Timeout with `{"error": {"code": "INFERENCE_TIMEOUT", "message": "..."}}`.
- **D-15:** New env vars added to the Config struct via envy, following the same pattern as Phase 1 (D-11, D-12): `PORT`, `REQUEST_TIMEOUT_SECS`, `SHUTDOWN_TIMEOUT_SECS`, `OTEL_EXPORTER_OTLP_ENDPOINT`.

### Claude's Discretion
No areas deferred to Claude's discretion -- all decisions made explicitly.

### Deferred Ideas (OUT OF SCOPE)
None -- discussion stayed within phase scope.

</user_constraints>

<phase_requirements>

## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| API-01 | Runtime serves inference requests over HTTP REST (JSON request/response) | axum 0.8 JSON extractor/response, Router with POST handler, State sharing for pipeline access |
| API-02 | Runtime exposes liveness probe endpoint that responds immediately on startup | axum route at `/healthz/live` returning 200 unconditionally |
| API-03 | Runtime exposes readiness probe endpoint that gates on successful model load | AtomicBool or watch channel readiness flag in shared state, flipped after warmup pass |
| API-04 | Runtime performs graceful shutdown on SIGTERM -- drains in-flight requests before exiting | axum serve().with_graceful_shutdown(), tokio::signal for SIGTERM, readiness flip to 503 |
| CORE-04 | Runtime enforces request timeouts to prevent runaway inference from blocking the server | tower-http TimeoutLayer with configurable duration from `REQUEST_TIMEOUT_SECS` |
| OBSV-01 | Runtime exposes Prometheus metrics endpoint with inference latency histograms, request counts, and error rates | metrics crate + metrics-exporter-prometheus, PrometheusHandle.render() on `/metrics` |
| OBSV-02 | Runtime emits structured JSON logs with request context (model ID, latency, status) | tracing-subscriber fmt layer with JSON format, #[instrument] on handlers |
| OBSV-03 | Runtime integrates OpenTelemetry distributed tracing with span propagation across inference pipeline | tracing-opentelemetry + opentelemetry-otlp conditional layer, trace context propagation |

</phase_requirements>

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| HTTP request routing | API / Backend (hephaestus-api) | -- | axum handles request dispatch, extraction, response serialization |
| Inference execution | API / Backend (hephaestus-core) | -- | Pipeline trait already owns tokenization + ONNX inference; HTTP layer wraps it |
| Health probes | API / Backend (hephaestus-api) | -- | /healthz/live and /healthz/ready are HTTP endpoints checked by k8s kubelet |
| Metrics collection | API / Backend (hephaestus-api) | -- | metrics crate records in-process; Prometheus scrapes /metrics endpoint |
| Structured logging | API / Backend (hephaestus binary) | -- | tracing-subscriber configured at process startup in main.rs |
| Distributed tracing | API / Backend (hephaestus binary) | External (OTel Collector) | tracing spans exported via OTLP gRPC to external collector |
| Graceful shutdown | API / Backend (hephaestus binary) | -- | Signal handling and drain logic in the binary entry point |
| Request timeout | API / Backend (hephaestus-api) | -- | tower-http TimeoutLayer applied as middleware |
| Configuration | API / Backend (hephaestus binary) | -- | envy loads env vars into Config struct at startup |

## Standard Stack

### Core (New for Phase 2)

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| axum | 0.8.9 | HTTP framework | Tokio-team maintained, Tower-native, macro-free. Shares middleware stack with future tonic gRPC (v2). [CITED: docs.rs/axum/0.8.9] |
| tower-http | 0.7.0 | HTTP middleware | TimeoutLayer, TraceLayer, CorsLayer. Official Tower middleware collection. [CITED: crates.io/crates/tower-http] |
| tower | 0.5.3 | Service/Layer trait | Foundation that axum and tonic both build on. Already a transitive dep. [CITED: crates.io/crates/tower] |
| metrics | 0.24.6 | Metrics facade | Lightweight counter!/histogram!/gauge! macros. Simpler than OTel metrics for Prometheus scrape. [CITED: crates.io/crates/metrics] |
| metrics-exporter-prometheus | 0.18.3 | Prometheus export | PrometheusBuilder installs recorder, Handle.render() serves /metrics. [CITED: docs.rs/metrics-exporter-prometheus] |
| tracing-opentelemetry | 0.33.0 | OTel bridge | Bridges tracing spans to OTel. Used with optional layer pattern. [CITED: crates.io/crates/tracing-opentelemetry] |
| opentelemetry | 0.32.0 | OTel API | Core types for trace context, span creation. [CITED: crates.io/crates/opentelemetry] |
| opentelemetry_sdk | 0.32.1 | OTel SDK | SdkTracerProvider, batch span processor. [CITED: crates.io/crates/opentelemetry_sdk] |
| opentelemetry-otlp | 0.32.0 | OTLP exporter | SpanExporter with gRPC (tonic) transport to OTel Collector. [CITED: docs.rs/opentelemetry-otlp] |

### Carried from Phase 1 (already in workspace)

| Library | Version | Purpose |
|---------|---------|---------|
| tokio | 1 | Async runtime -- needs `signal` feature added for SIGTERM handling |
| tracing | 0.1 | Instrumentation facade -- already used |
| tracing-subscriber | 0.3 | Log formatting -- extend with Registry + optional OTel layer |
| serde / serde_json | 1.0 | Request/response serialization -- already used |
| anyhow | 1.0 | Application-level error handling -- already used |
| thiserror | 2.0 | Library error types -- already used in hephaestus-core |
| envy | 0.4 | Config from env vars -- extend Config struct |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| metrics + prometheus exporter | opentelemetry metrics SDK | OTel metrics SDK is more complex, prometheus exporter is deprecated, push-based adds infra. metrics crate is simpler for scrape. [ASSUMED] |
| tower-http TimeoutLayer | Custom timeout middleware | TimeoutLayer handles the 408 response, edge cases with streaming, and integrates with Tower. No reason to hand-roll. [ASSUMED] |
| Conditional OTel layer via Option | Feature flags | Feature flags require recompilation. Runtime env var check is simpler and matches k8s deployment pattern. [ASSUMED] |
| axum-prometheus | Manual metrics middleware | axum-prometheus adds auto HTTP metrics but locks you into its label schema. Manual recording gives per-stage histograms (D-08). [ASSUMED] |

**Installation (workspace Cargo.toml additions):**
```toml
# HTTP serving
axum = "0.8"
tower = { version = "0.5", features = ["timeout"] }
tower-http = { version = "0.7", features = ["timeout", "trace"] }

# Metrics
metrics = "0.24"
metrics-exporter-prometheus = "0.18"

# OpenTelemetry (conditional at runtime)
opentelemetry = "0.32"
opentelemetry_sdk = { version = "0.32", features = ["rt-tokio"] }
opentelemetry-otlp = { version = "0.32", features = ["grpc-tonic"] }
tracing-opentelemetry = "0.33"
```

**Tokio feature update:**
```toml
tokio = { version = "1", features = ["rt-multi-thread", "macros", "signal"] }
```

## Package Legitimacy Audit

| Package | Registry | Age | Downloads | Source Repo | Verdict | Disposition |
|---------|----------|-----|-----------|-------------|---------|-------------|
| axum | crates.io | 5 yrs | 8.5M/wk | github.com/tokio-rs/axum | OK | Approved |
| tower | crates.io | 10 yrs | 12.4M/wk | github.com/tower-rs/tower | OK | Approved |
| tower-http | crates.io | 9 yrs | 10.1M/wk | github.com/tower-rs/tower-http | OK | Approved |
| metrics | crates.io | 11 yrs | 1.4M/wk | github.com/metrics-rs/metrics | OK | Approved |
| metrics-exporter-prometheus | crates.io | 6 yrs | 897K/wk | github.com/metrics-rs/metrics | OK | Approved |
| tracing-opentelemetry | crates.io | 7 yrs | 3.2M/wk | github.com/tokio-rs/tracing-opentelemetry | OK | Approved |
| opentelemetry | crates.io | 7 yrs | 4.1M/wk | github.com/open-telemetry/opentelemetry-rust | OK | Approved |
| opentelemetry_sdk | crates.io | 7 yrs | 3.7M/wk | github.com/open-telemetry/opentelemetry-rust | OK | Approved |
| opentelemetry-otlp | crates.io | 6 yrs | 2.9M/wk | github.com/open-telemetry/opentelemetry-rust | OK | Approved |

**Packages removed due to [SLOP] verdict:** none
**Packages flagged as suspicious [SUS]:** none

## Architecture Patterns

### System Architecture Diagram

```
                     HTTP Request (JSON)
                           |
                           v
                  +------------------+
                  |   axum Router    |
                  |  (hephaestus-api)|
                  +--------+---------+
                           |
            +--------------+--------------+
            |              |              |
            v              v              v
     POST /infer    GET /healthz/*   GET /metrics
            |              |              |
            v              v              v
    +-------+------+  ReadinessFlag  PrometheusHandle
    | TimeoutLayer |  (AtomicBool)    .render()
    +-------+------+
            |
            v
    +-------+------+     metrics crate
    | InferHandler |---> histogram!/counter!
    +-------+------+     (via Timer abstraction)
            |
     +------+------+
     |  Pipeline   |
     |  .prepare() |----> tracing spans
     |  .execute() |      (OTel layer optional)
     +------+------+
            |
            v
      ClassifierOutput
            |
            v
    JSON Response + latency_ms
```

### Recommended Project Structure

```
crates/
  hephaestus/                    # Binary crate (existing)
    src/
      main.rs                    # async tokio main, server startup, shutdown
      config.rs                  # Config struct extended with Phase 2 env vars
  hephaestus-api/                # NEW: HTTP serving layer
    src/
      lib.rs                     # Re-exports
      routes.rs                  # axum Router construction, route mounting
      handlers.rs                # POST /infer, health probe handlers
      error.rs                   # CoreError -> HTTP response mapping
      state.rs                   # AppState: pipeline + readiness + metrics
      metrics.rs                 # Timer abstraction, histogram setup, /metrics handler
      telemetry.rs               # tracing subscriber init, conditional OTel layer
    Cargo.toml
  hephaestus-core/               # Unchanged from Phase 1
  hephaestus-proto/              # Unchanged (stub)
  hephaestus-resolve/            # Unchanged (stub)
```

### Pattern 1: Shared Application State

**What:** All HTTP handlers access the pipeline, readiness flag, model metadata, and metrics handle through axum's State extractor backed by an Arc.

**When to use:** Every handler that needs pipeline access, readiness checks, or metric recording.

**Example:**
```rust
// Source: docs.rs/axum/0.8.9 State extractor pattern
use std::sync::Arc;
use axum::extract::State;

pub struct AppState {
    pub pipeline: tokio::sync::Mutex<ClassifierPipeline>,
    pub ready: std::sync::atomic::AtomicBool,
    pub model_id: String,
    pub start_time: std::time::Instant,
    pub metrics_handle: metrics_exporter_prometheus::PrometheusHandle,
}

async fn infer(
    State(state): State<Arc<AppState>>,
    axum::Json(req): axum::Json<InferRequest>,
) -> Result<axum::Json<InferResponse>, AppError> {
    // ...
}
```

### Pattern 2: Conditional OTel Layer

**What:** The OTel tracing layer is added only when `OTEL_EXPORTER_OTLP_ENDPOINT` is set. Uses `Option<Layer>` which implements `Layer` itself (passing through when None).

**When to use:** Telemetry initialization at startup.

**Example:**
```rust
// Source: tracing-subscriber docs, Registry layering pattern
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, Registry};

let fmt_layer = tracing_subscriber::fmt::layer()
    .json()
    .with_target(true);

let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
    .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&config.log_level));

let otel_layer = config.otel_endpoint.as_ref().map(|endpoint| {
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .build()
        .expect("failed to build OTLP exporter");
    let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .build();
    opentelemetry::global::set_tracer_provider(provider.clone());
    let tracer = provider.tracer("hephaestus");
    tracing_opentelemetry::OpenTelemetryLayer::new(tracer)
});

Registry::default()
    .with(env_filter)
    .with(fmt_layer)
    .with(otel_layer)  // Option<Layer> is itself a Layer
    .init();
```

### Pattern 3: Graceful Shutdown with Readiness Flip

**What:** On SIGTERM, immediately flip readiness to 503 (k8s stops routing), then let axum drain in-flight requests within the configured timeout.

**When to use:** Server startup in main.rs.

**Example:**
```rust
// Source: github.com/tokio-rs/axum/examples/graceful-shutdown
use std::sync::Arc;
use tokio::signal;

async fn shutdown_signal(state: Arc<AppState>) {
    let ctrl_c = async { signal::ctrl_c().await.expect("Ctrl+C handler") };
    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!("shutdown signal received, draining connections");
    state.ready.store(false, std::sync::atomic::Ordering::SeqCst);
}

// In main:
axum::serve(listener, app)
    .with_graceful_shutdown(shutdown_signal(state.clone()))
    .await?;
```

### Pattern 4: Deep-Module Timer Abstraction (D-09)

**What:** A timer struct that hides all metrics crate interaction. Pipeline stages call a single interface; the timer records histograms with model_id labels internally.

**When to use:** Every pipeline stage (tokenization, inference, postprocessing) and the overall request.

**Example:**
```rust
// Deep module: callers never touch metrics crate directly
use std::time::Instant;

pub struct StageTimer {
    model_id: String,
}

impl StageTimer {
    pub fn new(model_id: String) -> Self {
        Self { model_id }
    }

    /// Time a stage and record its duration to the appropriate histogram.
    pub fn time<T>(&self, stage: &'static str, f: impl FnOnce() -> T) -> T {
        let start = Instant::now();
        let result = f();
        let elapsed = start.elapsed().as_secs_f64();
        metrics::histogram!(
            "hephaestus_stage_duration_seconds",
            "stage" => stage,
            "model_id" => self.model_id.clone(),
        )
        .record(elapsed);
        result
    }
}
```

### Pattern 5: CoreError to HTTP Error Response Mapping

**What:** Map `CoreError` variants to structured JSON error responses with appropriate HTTP status codes (D-03).

**When to use:** The error.rs module in hephaestus-api.

**Example:**
```rust
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

struct AppError(anyhow::Error);

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code) = if let Some(core_err) = self.0.downcast_ref::<CoreError>() {
            match core_err {
                CoreError::Tokenization(_) => (StatusCode::UNPROCESSABLE_ENTITY, "TOKENIZATION_FAILED"),
                CoreError::Inference(_) => (StatusCode::INTERNAL_SERVER_ERROR, "INFERENCE_FAILED"),
                CoreError::ModelLoad(_) | CoreError::ModelValidation(_) => {
                    (StatusCode::INTERNAL_SERVER_ERROR, "MODEL_ERROR")
                }
                _ => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR"),
            }
        } else {
            (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR")
        };

        let body = serde_json::json!({
            "error": { "code": code, "message": self.0.to_string() }
        });
        (status, axum::Json(body)).into_response()
    }
}
```

### Anti-Patterns to Avoid
- **Blocking the tokio runtime with synchronous inference:** ort Session::run() is synchronous and CPU-bound. Wrapping it in `tokio::task::spawn_blocking()` prevents it from starving the async executor. However, with one-model-per-pod and low request concurrency, the initial implementation can use a Mutex on the pipeline directly -- spawn_blocking is an optimization for Phase 4 batching. [ASSUMED]
- **Global mutable state for readiness:** Do not use `lazy_static` or `OnceCell<Mutex<bool>>`. Use `AtomicBool` in the shared `AppState` -- lock-free, zero-overhead reads from health probe handlers.
- **Recording metrics in hot-path closures without labels:** Always attach `model_id` via the timer abstraction. Forgetting labels produces unlabeled metrics that cannot be filtered per-deployment in Grafana.
- **Initializing OTel in a blocking context:** `SdkTracerProvider::builder().with_batch_exporter()` spawns a tokio task. The tracing subscriber must be initialized inside the tokio runtime (after `#[tokio::main]`), not before.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Request timeout | Custom future wrapper with tokio::time::timeout | tower-http TimeoutLayer | Handles response body, streaming, status code (408 default). Edge cases with half-sent responses. [CITED: docs.rs/tower-http/0.7.0/tower_http/timeout] |
| Prometheus text format | Manual string formatting of metric values | metrics-exporter-prometheus PrometheusHandle.render() | Correctly encodes histograms with le buckets, TYPE/HELP lines, escaping. [CITED: docs.rs/metrics-exporter-prometheus] |
| OTLP span export | Manual gRPC calls to OTel Collector | opentelemetry-otlp SpanExporter | Handles batching, retry, backoff, protobuf encoding. [CITED: docs.rs/opentelemetry-otlp] |
| Signal handling (SIGTERM) | Raw libc signal handlers | tokio::signal::unix::signal(SignalKind::terminate()) | Async-safe, composable with select!, no unsafe code. [CITED: docs.rs/tokio/latest/tokio/signal] |
| Graceful shutdown drain | Manual connection tracking | axum::serve().with_graceful_shutdown() | Tracks in-flight connections internally, waits for completion. [CITED: github.com/tokio-rs/axum/examples/graceful-shutdown] |

**Key insight:** The axum + tower + tower-http ecosystem provides composable middleware for every infrastructure concern in this phase. Building custom solutions would duplicate tested behavior and miss edge cases (half-closed connections, streaming bodies, concurrent signal delivery).

## Common Pitfalls

### Pitfall 1: OTel Layer Initialized Outside Tokio Runtime
**What goes wrong:** `SdkTracerProvider::builder().with_batch_exporter()` spawns a background tokio task for batch flushing. If called before the tokio runtime starts, it panics.
**Why it happens:** Phase 1's `main()` initializes tracing before any async work. Phase 2 must restructure to init tracing inside `#[tokio::main]`.
**How to avoid:** Move tracing-subscriber initialization into the async `main()` function, after `#[tokio::main]` sets up the runtime.
**Warning signs:** Panic with "must be called from the context of a Tokio runtime" at startup.

### Pitfall 2: Pipeline Mutex Contention Under Load
**What goes wrong:** `ClassifierPipeline::execute()` takes `&mut self` (ort Session::run() needs mutability). Wrapping in `tokio::sync::Mutex` means only one inference runs at a time.
**Why it happens:** ONNX Runtime sessions are not thread-safe for concurrent run() calls without internal synchronization.
**How to avoid:** For Phase 2 (one-model-per-pod, low concurrency), a single Mutex is acceptable. Document that Phase 4 (batching) or high-concurrency scenarios should use `spawn_blocking` + a pool of sessions. [ASSUMED]
**Warning signs:** Increasing p99 latency under concurrent requests, histogram showing bimodal distribution.

### Pitfall 3: Readiness Race Condition
**What goes wrong:** If the readiness probe endpoint is mounted before the warmup pass completes, a brief window exists where the probe returns 503 as expected -- but if readiness defaults to `true`, traffic arrives before the model is loaded.
**Why it happens:** Readiness flag initialized to wrong default.
**How to avoid:** Initialize `AtomicBool` readiness to `false`. Flip to `true` only after warmup inference succeeds. The server starts listening but the readiness probe correctly returns 503 until warmup completes.
**Warning signs:** 500 errors on first few requests in a new pod, ONNX session panic before model load.

### Pitfall 4: TimeoutLayer Returns 408, Not 504
**What goes wrong:** tower-http TimeoutLayer returns HTTP 408 (Request Timeout) by default. D-14 specifies HTTP 504 with a structured JSON error body.
**Why it happens:** 408 is the RFC-correct status for server-detected request timeout. 504 is more appropriate for "upstream inference took too long."
**How to avoid:** Either (a) use TimeoutLayer directly and accept 408, or (b) implement a custom timeout via `tokio::time::timeout` around the inference call in the handler, returning the D-14 structured 504 response. Option (b) is recommended to match the decision exactly.
**Warning signs:** Monitoring dashboards showing 408 instead of expected 504 on timeouts.

### Pitfall 5: OpenTelemetry Version Mismatch
**What goes wrong:** opentelemetry, opentelemetry_sdk, opentelemetry-otlp, and tracing-opentelemetry must be version-compatible. Mismatched versions cause compile errors or subtle runtime bugs.
**Why it happens:** The OTel Rust ecosystem releases in lockstep but crates.io allows mixing versions.
**How to avoid:** Pin all OTel crates in workspace dependencies. The compatible set is: opentelemetry 0.32, opentelemetry_sdk 0.32, opentelemetry-otlp 0.32, tracing-opentelemetry 0.33. [CITED: crates.io version cross-reference]
**Warning signs:** Cryptic trait bound errors mentioning `opentelemetry::trace::Tracer` during compilation.

## Code Examples

### Complete Inference Handler

```rust
// Source: axum docs (State, Json extractors), combined with D-01/D-02/D-03
use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Instant;

#[derive(Deserialize)]
pub struct InferRequest {
    pub text: String,
}

#[derive(Serialize)]
pub struct InferResponse {
    pub label: String,
    pub score: f32,
    pub model_id: String,
    pub latency_ms: u64,
}

#[tracing::instrument(skip(state))]
pub async fn infer(
    State(state): State<Arc<AppState>>,
    Json(req): Json<InferRequest>,
) -> Result<Json<InferResponse>, AppError> {
    let start = Instant::now();
    let timer = StageTimer::new(state.model_id.clone());

    let mut pipeline = state.pipeline.lock().await;

    let prepared = timer.time("tokenization", || pipeline.prepare(req.text))?;
    let output = timer.time("inference", || pipeline.execute(prepared))?;

    let latency = start.elapsed();
    metrics::histogram!(
        "hephaestus_request_duration_seconds",
        "model_id" => state.model_id.clone(),
    )
    .record(latency.as_secs_f64());
    metrics::counter!(
        "hephaestus_requests_total",
        "model_id" => state.model_id.clone(),
        "status" => "ok",
    )
    .increment(1);

    Ok(Json(InferResponse {
        label: output.label,
        score: output.score,
        model_id: state.model_id.clone(),
        latency_ms: latency.as_millis() as u64,
    }))
}
```

### Router Construction

```rust
// Source: axum docs, D-04 (hephaestus-api crate)
use axum::{Router, routing::{get, post}};
use std::sync::Arc;

pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/infer", post(infer))
        .route("/healthz/live", get(liveness))
        .route("/healthz/ready", get(readiness))
        .route("/metrics", get(metrics_handler))
        .with_state(state)
}

async fn liveness(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "model_id": state.model_id,
        "uptime_s": state.start_time.elapsed().as_secs(),
    }))
}

async fn readiness(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    if state.ready.load(std::sync::atomic::Ordering::SeqCst) {
        (StatusCode::OK, Json(serde_json::json!({
            "status": "ok",
            "model_id": state.model_id,
            "uptime_s": state.start_time.elapsed().as_secs(),
        })))
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({
            "status": "not_ready",
            "model_id": state.model_id,
        })))
    }
}

async fn metrics_handler(State(state): State<Arc<AppState>>) -> String {
    state.metrics_handle.render()
}
```

### Async Main Restructure

```rust
// Source: axum graceful-shutdown example + Phase 1 main.rs integration
use anyhow::Context;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let config = config::Config::from_env()?;

    // Initialize telemetry (must be inside tokio runtime for OTel batch exporter)
    hephaestus_api::telemetry::init(&config)?;

    // Set up metrics recorder
    let metrics_handle = hephaestus_api::metrics::install_recorder()?;

    // Construct pipeline
    let model_dir = config.model_dir()?;
    let pipeline = ClassifierPipeline::new(&model_dir)
        .context("failed to construct classifier pipeline")?;

    // Build shared state (readiness starts false)
    let state = Arc::new(AppState {
        pipeline: tokio::sync::Mutex::new(pipeline),
        ready: std::sync::atomic::AtomicBool::new(false),
        model_id: config.model_id.clone(),
        start_time: std::time::Instant::now(),
        metrics_handle,
    });

    // Run warmup, then flip readiness
    {
        let mut pipeline = state.pipeline.lock().await;
        let warmup_text = config.warmup_input.as_deref().unwrap_or("warmup");
        let prepared = pipeline.prepare(warmup_text.to_string())?;
        let _output = pipeline.execute(prepared)?;
    }
    state.ready.store(true, std::sync::atomic::Ordering::SeqCst);
    tracing::info!("warmup complete, readiness enabled");

    // Bind and serve
    let addr = format!("0.0.0.0:{}", config.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!(%addr, "listening");

    let app = hephaestus_api::routes::build_router(state.clone());
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal(state.clone()))
        .await?;

    // Flush OTel spans before exit
    opentelemetry::global::shutdown_tracer_provider();
    Ok(())
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| opentelemetry 0.20-0.27 API | opentelemetry 0.28+ unified API | Late 2024 | ExportError trait moved, Views opt-in, Temporality enum relocated |
| opentelemetry-prometheus (deprecated) | metrics + metrics-exporter-prometheus | 2024 | Simpler setup, no OTel metrics SDK dependency |
| axum 0.6/0.7 | axum 0.8 | March 2025 | serve() replaces Server::bind(), State replaces Extension |
| hyper::Server for axum | axum::serve() | axum 0.7+ | Direct TcpListener binding, simpler API |

**Deprecated/outdated:**
- `opentelemetry-prometheus` crate: Deprecated in favor of metrics crate + Prometheus exporter for scrape-based setups
- `axum::Server::bind()`: Replaced by `axum::serve(listener, app)` in axum 0.7+
- `opentelemetry::sdk::trace::TracerProvider` path: Now `opentelemetry_sdk::trace::SdkTracerProvider`

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | Mutex on ClassifierPipeline is acceptable for Phase 2 low-concurrency scenario | Anti-Patterns, Pitfalls | If concurrent requests are expected, p99 latency degrades; fix by using spawn_blocking or session pool |
| A2 | metrics crate is simpler than OTel metrics SDK for Prometheus scrape | Alternatives Considered | If OTel metrics are needed later for push-based export, would need migration |
| A3 | OTel batch exporter requires tokio runtime context at init time | Pitfalls | If this is wrong, tracing init could stay before tokio::main; low risk since docs confirm it |
| A4 | axum-prometheus is not used because it locks label schema | Alternatives Considered | If the built-in labels suffice, it would save implementation time; but D-08 requires custom per-stage histograms |

## Open Questions (RESOLVED)

1. **Timeout implementation: TimeoutLayer (408) vs custom handler-level timeout (504)**
   - What we know: tower-http TimeoutLayer returns 408 by default. D-14 specifies 504 with structured JSON body.
   - What's unclear: Whether TimeoutLayer can be configured with a custom response body and status code (docs mention `with_status_code` method).
   - Recommendation: Use `tokio::time::timeout` around the inference call in the handler body. This gives full control over the 504 response structure matching D-14. Apply TimeoutLayer as a safety net at a higher timeout (e.g., 60s) to catch edge cases.
   - RESOLVED: Use `tokio::time::timeout` at the handler level for D-14 structured 504 responses. Implemented in 02-01-PLAN.md Task 2.

2. **Pipeline mutability and Send bounds**
   - What we know: `ClassifierPipeline::execute()` takes `&mut self`. ort Session is not Sync.
   - What's unclear: Whether tokio::sync::Mutex is sufficient or if spawn_blocking is needed for the synchronous ort::Session::run() call.
   - Recommendation: Start with `tokio::sync::Mutex<ClassifierPipeline>` in shared state. Document spawn_blocking as a Phase 4 optimization. The one-model-per-pod design means low request concurrency per pod.
   - RESOLVED: Use `tokio::sync::Mutex<ClassifierPipeline>` in AppState. spawn_blocking deferred to Phase 4 per one-model-per-pod low-concurrency design. Implemented in 02-01-PLAN.md Task 1.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| Rust toolchain | All code | Yes | 1.97.1 (stable) | -- |
| Cargo | Build | Yes | 1.97.1 | -- |
| tokio runtime | Async HTTP | Yes (dep) | 1.x | -- |

**Missing dependencies with no fallback:** None -- all dependencies are Rust crates resolved via Cargo.

**Missing dependencies with fallback:** None.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | cargo test (built-in) + tokio::test for async |
| Config file | none -- uses Cargo.toml test settings |
| Quick run command | `cargo test -p hephaestus-api` |
| Full suite command | `cargo test --workspace` |

### Phase Requirements to Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| API-01 | POST /infer returns JSON classification | integration | `cargo test -p hephaestus-api --test http_integration -- --ignored` | No -- Wave 0 |
| API-02 | GET /healthz/live returns 200 immediately | unit | `cargo test -p hephaestus-api health_live` | No -- Wave 0 |
| API-03 | GET /healthz/ready gates on warmup | unit | `cargo test -p hephaestus-api health_ready` | No -- Wave 0 |
| API-04 | SIGTERM triggers graceful drain | integration | `cargo test -p hephaestus-api --test shutdown_integration -- --ignored` | No -- Wave 0 |
| CORE-04 | Request timeout returns error | unit | `cargo test -p hephaestus-api timeout` | No -- Wave 0 |
| OBSV-01 | /metrics endpoint returns Prometheus text | unit | `cargo test -p hephaestus-api metrics_endpoint` | No -- Wave 0 |
| OBSV-02 | Structured JSON log contains model_id + latency | unit | `cargo test -p hephaestus-api structured_log` | No -- Wave 0 |
| OBSV-03 | OTel spans propagate through pipeline | integration | `cargo test -p hephaestus-api --test otel_integration -- --ignored` | No -- Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test -p hephaestus-api`
- **Per wave merge:** `cargo test --workspace`
- **Phase gate:** Full suite green before `/gsd-verify-work`

### Wave 0 Gaps
- [ ] `crates/hephaestus-api/Cargo.toml` -- new crate, needs creation
- [ ] `crates/hephaestus-api/tests/` -- integration test directory
- [ ] Unit tests in `crates/hephaestus-api/src/*.rs` via `#[cfg(test)]` modules
- [ ] Test utilities: mock pipeline (using mockall on Pipeline trait, already in Phase 1)

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | No | Internal service, no auth (out of scope per REQUIREMENTS.md) |
| V3 Session Management | No | Stateless HTTP service |
| V4 Access Control | No | Internal service only |
| V5 Input Validation | Yes | Validate request body (text field present, non-empty); tokenizer truncation at 512 tokens (T-01-02 from Phase 1) |
| V6 Cryptography | No | No secrets handling in this phase |

### Known Threat Patterns for HTTP + ONNX Inference

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Oversized request body | Denial of Service | axum has default body size limit (2MB). Add explicit limit via `DefaultBodyLimit::max()` if needed. |
| Runaway inference (adversarial input) | Denial of Service | Request timeout (CORE-04) via D-12 (30s default). Tokenizer truncation at 512 tokens (existing T-01-02). |
| Resource exhaustion via concurrent requests | Denial of Service | One-model-per-pod design limits blast radius. k8s resource limits (CPU/memory) provide hard ceiling. Pipeline Mutex serializes inference. |
| Information disclosure via error messages | Information Disclosure | Structured error responses (D-03) expose error codes, not stack traces. Debug info only in logs, not responses. |
| Health probe abuse | Information Disclosure | Health endpoints return minimal info. model_id is not sensitive (it is the deployment identity). |

## Project Constraints (from CLAUDE.md)

- **Language:** Rust only, 2024 edition, workspace resolver 3
- **Rules compliance:** Every file must adhere to all rules in `rules/`
- **Deep module pattern:** Traits expose 1-3 methods hiding significant complexity (Ousterhout principle)
- **No Clap:** Do not use Clap for k8s-only services; use envy for env var config (from user memory)
- **GSD Workflow:** All file changes through GSD commands
- **Error handling:** thiserror for library errors (hephaestus-core, hephaestus-api), anyhow for application errors (hephaestus binary)

## Sources

### Primary (HIGH confidence)
- [crates.io/axum](https://crates.io/crates/axum) -- version 0.8.9 verified via cargo search
- [crates.io/tower-http](https://crates.io/crates/tower-http) -- version 0.7.0 verified
- [crates.io/metrics](https://crates.io/crates/metrics) -- version 0.24.6 verified
- [crates.io/metrics-exporter-prometheus](https://crates.io/crates/metrics-exporter-prometheus) -- version 0.18.3 verified
- [crates.io/tracing-opentelemetry](https://crates.io/crates/tracing-opentelemetry) -- version 0.33.0 verified
- [crates.io/opentelemetry](https://crates.io/crates/opentelemetry) -- version 0.32.0 verified
- [crates.io/opentelemetry_sdk](https://crates.io/crates/opentelemetry_sdk) -- version 0.32.1 verified
- [crates.io/opentelemetry-otlp](https://crates.io/crates/opentelemetry-otlp) -- version 0.32.0 verified

### Secondary (MEDIUM confidence)
- [docs.rs/axum/0.8.9](https://docs.rs/axum/latest/axum/) -- Router, State, Json patterns
- [docs.rs/metrics-exporter-prometheus](https://docs.rs/metrics-exporter-prometheus/latest/metrics_exporter_prometheus/) -- PrometheusBuilder, render()
- [docs.rs/opentelemetry-otlp](https://docs.rs/opentelemetry-otlp/latest/opentelemetry_otlp/) -- SpanExporter builder
- [docs.rs/tower-http/timeout](https://docs.rs/tower-http/latest/tower_http/timeout/) -- TimeoutLayer
- [github.com/tokio-rs/axum/examples/graceful-shutdown](https://github.com/tokio-rs/axum/blob/main/examples/graceful-shutdown/src/main.rs) -- Shutdown signal pattern
- [github.com/tokio-rs/axum/examples/prometheus-metrics](https://github.com/tokio-rs/axum/blob/main/examples/prometheus-metrics/src/main.rs) -- Metrics endpoint pattern

### Tertiary (LOW confidence)
- WebSearch results for OTel conditional layer pattern -- verified against docs.rs documentation

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- all crates verified on crates.io, versions confirmed, official examples reviewed
- Architecture: HIGH -- patterns derived from official axum examples and docs.rs documentation, cross-referenced with Phase 1 codebase
- Pitfalls: MEDIUM -- identified from docs and common patterns, but OTel init timing and Mutex contention claims based on training knowledge

**Research date:** 2026-08-23
**Valid until:** 2026-09-22 (30 days -- mature stable stack)
