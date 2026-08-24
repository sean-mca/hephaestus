---
phase: 02-http-serving-and-observability
verified: 2026-08-24T22:50:18Z
status: gaps_found
score: 2/8 must-haves verified
behavior_unverified: 5
overrides_applied: 0
gaps:
  - truth: "Structured JSON logs include model_id, latency, and status fields on every request (OBSV-02)"
    status: failed
    reason: >
      No per-request log event is ever emitted. crates/hephaestus-api/src/handlers.rs infer()
      carries #[tracing::instrument(skip(state))] but that attribute only creates a tracing
      span -- it does not by itself produce any log line. crates/hephaestus-api/src/telemetry.rs
      builds the JSON fmt layer as tracing_subscriber::fmt::layer().json().with_target(true) with
      no .with_span_events(...) configured, so span enter/exit is never logged either. There is
      no tracing::info!/warn!/event! call anywhere inside infer() on the success path or any
      error path. tower-http is declared in the root Cargo.toml workspace deps with
      features = ["timeout", "trace"] (added in 02-01 specifically to support HTTP tracing), but
      hephaestus-api/Cargo.toml never depends on tower-http and tower_http::trace::TraceLayer is
      never applied to the router in routes.rs. The only structured log lines that exist are
      startup-time events in crates/hephaestus/src/main.rs (config loaded, pipeline constructed,
      warmup complete, listening, shutdown) -- none of which fire per request or carry latency/status.
    artifacts:
      - path: "crates/hephaestus-api/src/handlers.rs"
        issue: "infer() never emits a tracing event with model_id/latency/status; #[instrument] alone produces no log output"
      - path: "crates/hephaestus-api/src/telemetry.rs"
        issue: "fmt layer has no with_span_events(...), so span enter/exit for the instrumented infer() span is never printed"
      - path: "crates/hephaestus-api/src/routes.rs"
        issue: "tower_http::trace::TraceLayer (workspace dep declared with the trace feature) is never applied to the router"
    missing:
      - "Emit a tracing event at the end of infer() (success and error paths) carrying model_id, latency_ms, and status, e.g. tracing::info!(model_id = %state.model_id, latency_ms, status = \"ok\", \"request completed\")"
      - "Or wire tower_http::trace::TraceLayer (already a workspace dependency) into build_router() so every request produces a structured log line"
      - "Add an automated test that asserts the emitted JSON log line contains model_id, latency, and status -- the existing tests/tracing.rs::structured_logs_contain_model_id test is #[ignore]d and does not assert on log content even when manually run"
behavior_unverified_items:
  - truth: "GET /healthz/ready returns 503 before warmup and 200 after (API-03)"
    test: "Construct an AppState with ready=false, call the readiness handler directly (or via router), assert 503; flip ready to true, call again, assert 200"
    expected: "503 Service Unavailable before the flag flips, 200 OK after"
    why_human: "No test in the repo constructs an AppState and calls readiness()/liveness() directly; tests/health.rs is entirely #[ignore]d pending model fixtures. The state transition itself is unexercised by any automated test."
  - truth: "SIGTERM flips readiness to 503 and drains in-flight requests within SHUTDOWN_TIMEOUT_SECS (API-04)"
    test: "Start the binary with a real model, send SIGTERM while a request is in flight, verify readiness flips to 503 immediately and the in-flight request completes before the process exits (or force-exits after SHUTDOWN_TIMEOUT_SECS if it doesn't)"
    expected: "Readiness flips to false on signal; process waits for drain up to the timeout, then force-exits if exceeded"
    why_human: "tests/shutdown.rs is entirely #[ignore]d; no test exercises shutdown_signal() or the drain watchdog task in main.rs. This is a runtime, signal-based, multi-task cancellation/ordering invariant that presence-of-code cannot prove."
  - truth: "Inference requests exceeding REQUEST_TIMEOUT_SECS return HTTP 504 with INFERENCE_TIMEOUT (CORE-04)"
    test: "POST /infer against a pipeline whose prepare/execute takes longer than request_timeout; verify tokio::time::timeout fires and the response is 504 with error.code == INFERENCE_TIMEOUT"
    expected: "504 Gateway Timeout with structured INFERENCE_TIMEOUT body"
    why_human: "Only the ApiError::Timeout -> IntoResponse mapping is unit-tested (error.rs). The actual tokio::time::timeout race around pipeline.prepare()/execute() in handlers::infer is untested -- tests/metrics.rs::request_timeout_returns_504 is #[ignore]d."
  - truth: "User can POST JSON text to /infer and receive a classification result end to end (API-01)"
    test: "Start the binary with MODEL_PATH pointing at real model files, POST {\"text\": \"...\"} to /infer, verify 200 with label/score/model_id/latency_ms populated from a real inference pass"
    expected: "JSON classification result reflecting real model output"
    why_human: "No model fixture exists in this repository/environment (no .onnx, tokenizer.json under crates/*). tests/api.rs is entirely #[ignore]d. Only serialization/deserialization of the request/response structs is unit-tested; the HTTP -> pipeline -> HTTP round trip is never exercised."
  - truth: "When OTEL_EXPORTER_OTLP_ENDPOINT is set, tracing spans export via OTLP (OBSV-03)"
    test: "Set OTEL_EXPORTER_OTLP_ENDPOINT to a running OTel Collector, start the binary, make a request, verify spans arrive at the collector"
    expected: "Spans appear at the OTel Collector when the endpoint is configured"
    why_human: "Only the None-endpoint path is unit/integration tested (telemetry_init_without_otel_does_not_panic). The Some-endpoint path (actual OTLP exporter construction and span export) requires a live collector and is untested."
---

# Phase 2: HTTP Serving and Observability — Verification Report

**Phase Goal:** As a platform operator, I want to deploy Hephaestus as a Kubernetes pod and send HTTP inference requests, so that I can serve model inference with health probes, metrics, and tracing in production.
**Verified:** 2026-08-24T22:50:18Z
**Status:** gaps_found
**Re-verification:** No — initial verification
**Mode:** mvp (user-story goal validated via `user-story.validate`)

## User Flow Coverage

User story: «As a platform operator, I want to deploy Hephaestus as a Kubernetes pod and send HTTP inference requests, so that I can serve model inference with health probes, metrics, and tracing in production.»

| Step | Expected | Evidence | Status |
|------|----------|----------|--------|
| Deploy as a pod | Binary starts via `#[tokio::main]`, loads env config, binds `0.0.0.0:{PORT}` | `crates/hephaestus/src/main.rs:18-89` | ✓ (code present; not run live — no model fixture in repo) |
| Send HTTP inference request | `POST /infer` accepts `{"text": ...}`, returns `{label, score, model_id, latency_ms}` | `crates/hephaestus-api/src/handlers.rs:55-108`, `routes.rs:24` | ⚠️ present + wired, full round trip unverified (see behavior_unverified_items) |
| Health probes | `GET /healthz/live` always 200; `GET /healthz/ready` gates on warmup | `handlers.rs:115-146`, `routes.rs:25-26` | ⚠️ liveness verified by inspection; readiness state transition unverified |
| Metrics | `GET /metrics` returns Prometheus text with per-stage and per-request histograms, `model_id` labels | `metrics.rs`, `routes.rs:27` | ✓ VERIFIED — `tests/metrics.rs::metrics_endpoint_returns_prometheus_text` passes and asserts on content |
| Tracing | OTel export activates when `OTEL_EXPORTER_OTLP_ENDPOINT` set; JSON logs otherwise | `telemetry.rs` | ⚠️ None-path verified; Some-path (actual export) unverified |
| Outcome: "serve model inference with health probes, metrics, and tracing in production" | All of the above hold together, including structured per-request logs | See Observable Truths below | ✗ **FAILED** — structured JSON logs with model_id/latency/status per request are not emitted anywhere in the request path |

## Goal Achievement

### Observable Truths

| # | Truth | Requirement | Status | Evidence |
|---|-------|-------------|--------|----------|
| 1 | POST /infer returns JSON classification (label, score, model_id, latency_ms) | API-01 | ⚠️ PRESENT_BEHAVIOR_UNVERIFIED | `handlers.rs::infer` implemented and wired; only request/response struct (de)serialization is unit-tested (`handlers.rs` tests). Full pipeline round trip requires model files not present in this repo. |
| 2 | GET /healthz/live returns 200 immediately with model_id, uptime_s | API-02 | ✓ VERIFIED | `handlers.rs::liveness` is a pure, branchless read of `state.model_id`/`state.start_time`; wired at `routes.rs:25`. No state-dependent logic to leave unverified. |
| 3 | GET /healthz/ready returns 503 before warmup, 200 after (state transition) | API-03 | ⚠️ PRESENT_BEHAVIOR_UNVERIFIED | `handlers.rs::readiness` reads `state.ready` AtomicBool and branches correctly in code, but no test constructs an `AppState` and calls it in both states — `tests/health.rs` is entirely `#[ignore]`d. |
| 4 | SIGTERM flips readiness to 503 and drains in-flight requests within SHUTDOWN_TIMEOUT_SECS | API-04 | ⚠️ PRESENT_BEHAVIOR_UNVERIFIED | `main.rs::shutdown_signal` + drain watchdog task implemented; `tests/shutdown.rs` entirely `#[ignore]`d; no automated evidence the signal → flag flip → drain → force-exit chain actually works. |
| 5 | Requests exceeding REQUEST_TIMEOUT_SECS return HTTP 504 INFERENCE_TIMEOUT | CORE-04 | ⚠️ PRESENT_BEHAVIOR_UNVERIFIED | `handlers.rs::infer` wraps the pipeline call in `tokio::time::timeout(state.request_timeout, ...)`; only the `ApiError::Timeout` → 504 response *mapping* is unit-tested, not the actual timeout race. `tests/metrics.rs::request_timeout_returns_504` is `#[ignore]`d. |
| 6 | GET /metrics returns Prometheus text with hephaestus_stage_duration_seconds / hephaestus_request_duration_seconds histograms carrying model_id labels | OBSV-01 | ✓ VERIFIED | `tests/metrics.rs::metrics_endpoint_returns_prometheus_text` (not ignored, passes) installs the recorder, records via `StageTimer`, renders, and asserts the exposition text contains both histograms, `hephaestus_requests_total`, `model_id="test-model"`, and `stage="tokenization"`. |
| 7 | Structured JSON logs include model_id, latency, and status fields on every request | OBSV-02 | ✗ **FAILED** | No tracing event is emitted anywhere in `infer()`. `#[tracing::instrument(skip(state))]` alone creates a span but produces no log line (fmt layer has no `with_span_events`). `tower-http`'s `trace` feature is declared as a workspace dependency but never wired into the router. See Gaps below. |
| 8 | When OTEL_EXPORTER_OTLP_ENDPOINT is set, spans export via OTLP; unset → JSON logs only | OBSV-03 | ⚠️ PRESENT_BEHAVIOR_UNVERIFIED | `telemetry.rs::init` correctly builds `Option<OpenTelemetryLayer>` gated on `otel_endpoint`; the `None` path is tested (`telemetry_init_without_otel_does_not_panic`), the `Some` path (real OTLP export) requires a collector and is untested. |

**Score:** 2/8 truths verified (5 present, behavior-unverified; 1 failed)

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/hephaestus-api/src/lib.rs` | Module declarations + re-exports (error, handlers, metrics, routes, state, telemetry) | ✓ VERIFIED | All 6 modules declared; `StageTimer`, `install_recorder`, `build_router`, `AppState` re-exported |
| `crates/hephaestus-api/src/state.rs` | `AppState` with pipeline Mutex, readiness flag, model metadata, metrics handle | ✓ VERIFIED | Contains `pub struct AppState` with `pipeline: Mutex<ClassifierPipeline>`, `ready: AtomicBool`, `model_id`, `start_time`, `request_timeout`, `metrics_handle` |
| `crates/hephaestus-api/src/error.rs` | `ApiError` + `CoreError` → HTTP mapping per D-03 | ✓ VERIFIED | `impl IntoResponse for ApiError` maps all 7 variants to correct status codes and error codes; unit-tested |
| `crates/hephaestus-api/src/handlers.rs` | `infer`, `liveness`, `readiness` handlers | ✓ VERIFIED | All three `pub async fn` present, exported, wired into router |
| `crates/hephaestus-api/src/routes.rs` | `build_router` mounting all endpoints | ✓ VERIFIED | Mounts `POST /infer`, `GET /healthz/live`, `GET /healthz/ready`, `GET /metrics` |
| `crates/hephaestus-api/src/metrics.rs` | `StageTimer` deep-module abstraction, `install_recorder`, `metrics_handler` | ✓ VERIFIED | `StageTimer::time`/`finish_request` hide all `metrics` crate calls; behaviorally tested |
| `crates/hephaestus-api/src/telemetry.rs` | Layered subscriber with conditional OTel export | ✓ VERIFIED | `pub fn init(log_level, otel_endpoint) -> Result<()>` builds Registry + fmt + EnvFilter + optional OTel layer; `pub fn shutdown()` present |

### Key Link Verification

| From | To | Via | Status | Details |
|------|-----|-----|--------|---------|
| `handlers.rs` | `hephaestus-core/pipeline.rs` | `Pipeline::prepare()` + `execute()` behind `tokio::sync::Mutex` | ✓ WIRED | `state.pipeline.lock().await` then `timer.time("tokenization", \|\| pipeline.prepare(...))` / `timer.time("inference", \|\| pipeline.execute(...))` |
| `main.rs` | `routes.rs` | `build_router(state)` before `axum::serve()` | ✓ WIRED | `main.rs:91` calls `build_router(state.clone())`, passed into `axum::serve(listener, app)` |
| `handlers.rs` | `state.rs` | `State<Arc<AppState>>` extractor | ✓ WIRED | All three handlers take `State(state): State<Arc<AppState>>` |
| `handlers.rs` | `metrics.rs` | `StageTimer::time()` wraps prepare/execute | ✓ WIRED | Confirmed above |
| `main.rs` | `telemetry.rs` | `telemetry::init()` replaces inline tracing setup | ✓ WIRED | `main.rs:27-30` calls `hephaestus_api::telemetry::init(...)`; no inline `tracing_subscriber::fmt().json().init()` remains |
| `metrics.rs` | `routes.rs` | `metrics_handler` reads `PrometheusHandle` from `AppState`, rendered at `/metrics` | ✓ WIRED | `routes.rs:27` mounts `get(metrics::metrics_handler)`; handler calls `state.metrics_handle.render()` |
| `handlers.rs` | `telemetry`/tracing output | Per-request structured log with model_id/latency/status | ✗ **NOT WIRED** | No event emitted in `infer()`; no `TraceLayer` applied to router; see Gaps |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Workspace builds | `cargo build --workspace` | Exit 0, no errors | ✓ PASS |
| Workspace tests | `cargo test --workspace` | All suites green: `hephaestus` bin 6/6, `hephaestus_api` lib 10/10, `tests/api.rs` 0/2 (ignored), `tests/health.rs` 0/2 (ignored), `tests/metrics.rs` 1/1 + 2 ignored, `tests/shutdown.rs` 0/2 (ignored), `tests/tracing.rs` 1/1 + 1 ignored, `hephaestus_core` 10/10 | ✓ PASS (no failures; several requirement-critical tests are `#[ignore]`d rather than run) |
| Lint | `cargo clippy --workspace -- -D warnings` | Exit 0, no warnings | ✓ PASS |
| `/metrics` renders real Prometheus text with required series | `cargo test -p hephaestus-api --test metrics metrics_endpoint_returns_prometheus_text` | Passes; asserts `hephaestus_stage_duration_seconds`, `hephaestus_request_duration_seconds`, `hephaestus_requests_total`, `model_id="test-model"`, `stage="tokenization"` all present in rendered text | ✓ PASS |
| Live end-to-end HTTP server against a real model | Would require `MODEL_PATH` pointing at real `.onnx` + `tokenizer.json` + `config.json` | No fixture found anywhere in repo (`find ... -iname "*.onnx"` empty) | ? SKIP — no runnable model fixture in this environment |
| Structured per-request log content | `grep -rn "tracing::\(info\|warn\|error\)!" crates/hephaestus-api/src/handlers.rs` | No matches | ✗ FAIL — confirms Gap #1 (OBSV-02) |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|-------------|--------------|--------|----------|
| API-01 | 02-01 | HTTP REST inference (POST /infer, JSON) | ✓ SATISFIED (code) / needs human E2E | `handlers.rs::infer`, `routes.rs` |
| API-02 | 02-01 | Liveness probe responds immediately | ✓ SATISFIED | `handlers.rs::liveness` |
| API-03 | 02-01 | Readiness probe gates on model load | ✓ SATISFIED (code) / needs human E2E for the state transition | `handlers.rs::readiness`, `state.rs::ready` |
| API-04 | 02-01 | Graceful shutdown drains in-flight requests | ✓ SATISFIED (code) / needs human E2E | `main.rs::shutdown_signal`, drain watchdog |
| CORE-04 | 02-01 | Request timeout enforcement | ✓ SATISFIED (code) / needs human E2E | `handlers.rs::infer` `tokio::time::timeout` |
| OBSV-01 | 02-02 | Prometheus metrics endpoint | ✓ SATISFIED — behaviorally tested | `metrics.rs`, `tests/metrics.rs` |
| OBSV-02 | 02-02 | Structured JSON logs with request context (model ID, latency, status) | ✗ **BLOCKED** | No per-request log event exists anywhere in the request path |
| OBSV-03 | 02-02 | OpenTelemetry distributed tracing with span propagation | ✓ SATISFIED (code, None-path tested) / needs human E2E for Some-path | `telemetry.rs::init` |

No orphaned requirements: the 8 IDs in the phase's plans (`API-01..04, CORE-04, OBSV-01..03`) exactly match the IDs listed for Phase 2 in `ROADMAP.md` and `REQUIREMENTS.md`. Note that `REQUIREMENTS.md` currently marks OBSV-02 as `[x]` Complete — this verification contradicts that marking; OBSV-02 should be reopened until the logging gap is closed.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| `Cargo.toml` (root) | 41 | `tower-http = { version = "0.7", features = ["timeout", "trace"] }` declared but never consumed by any crate (`hephaestus-api/Cargo.toml` does not depend on it; no `tower_http::` import anywhere in `src/`) | ℹ️ Info | Corroborates Gap #1 — the dependency added specifically to support HTTP-level tracing (`trace` feature) was never wired in |

No `TBD`/`FIXME`/`XXX`/`TODO`/`HACK`/`PLACEHOLDER` markers found in any Phase 2 source file.

### Gaps Summary

One BLOCKER: **OBSV-02 (structured JSON logs with request context) is not implemented.** The infrastructure for structured logging exists (JSON fmt layer, conditional OTel layer, `#[tracing::instrument]` on the handler), but nothing in the request path ever emits a log event carrying `model_id`, `latency`, or `status`. `#[instrument]` alone only creates a span; without `with_span_events(...)` on the fmt layer or an explicit `tracing::info!`/`event!` call inside `infer()`, no JSON log line is produced per request. The `tower-http` `trace` feature was added to the workspace Cargo.toml in Plan 02-01 specifically to cover this, but was never applied to the router. This must be fixed before Phase 2's roadmap success criterion 4 ("logs are structured JSON with request context — model ID, latency, status") can be considered true.

Five items are present-and-wired but behaviorally unverified because their correctness depends on runtime state transitions (readiness flip, SIGTERM drain, request timeout race) or require a real model fixture / live OTel collector that does not exist in this repository/environment. These are not blockers in themselves — the code paths are implemented and match the plan's design — but they have zero automated proof of correct runtime behavior (all corresponding integration tests are `#[ignore]`d) and should be exercised manually (or with a model fixture added) before treating Phase 2 as production-ready.

---

*Verified: 2026-08-24T22:50:18Z*
*Verifier: Claude (gsd-verifier)*
