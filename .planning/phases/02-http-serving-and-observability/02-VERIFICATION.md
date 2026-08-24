---
phase: 02-http-serving-and-observability
verified: 2026-08-24T23:40:00Z
status: human_needed
score: 3/8 must-haves verified
behavior_unverified: 5
overrides_applied: 0
re_verification:
  previous_status: gaps_found
  previous_score: 2/8
  gaps_closed:
    - "Structured JSON logs include model_id, latency, and status fields on every request (OBSV-02)"
  gaps_remaining: []
  regressions: []
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
human_verification:
  - test: "Construct an AppState with ready=false, call the readiness handler directly (or via router), assert 503; flip ready to true, call again, assert 200"
    expected: "503 Service Unavailable before the flag flips, 200 OK after"
    why_human: "tests/health.rs is entirely #[ignore]d pending model fixtures; the readiness state transition is unexercised by any automated test."
  - test: "Start the binary with a real model, send SIGTERM while a request is in flight, verify readiness flips to 503 immediately and the in-flight request completes before the process exits (or force-exits after SHUTDOWN_TIMEOUT_SECS if it doesn't)"
    expected: "Readiness flips to false on signal; process waits for drain up to the timeout, then force-exits if exceeded"
    why_human: "tests/shutdown.rs is entirely #[ignore]d; this is a runtime, signal-based, multi-task cancellation/ordering invariant that presence-of-code cannot prove."
  - test: "POST /infer against a pipeline whose prepare/execute takes longer than request_timeout; verify tokio::time::timeout fires and the response is 504 with error.code == INFERENCE_TIMEOUT"
    expected: "504 Gateway Timeout with structured INFERENCE_TIMEOUT body"
    why_human: "tests/metrics.rs::request_timeout_returns_504 is #[ignore]d; the actual timeout race is untested."
  - test: "Start the binary with MODEL_PATH pointing at real model files, POST {\"text\": \"...\"} to /infer, verify 200 with label/score/model_id/latency_ms populated from a real inference pass"
    expected: "JSON classification result reflecting real model output"
    why_human: "No model fixture (.onnx/tokenizer.json) exists in this repository/environment; tests/api.rs is entirely #[ignore]d."
  - test: "Set OTEL_EXPORTER_OTLP_ENDPOINT to a running OTel Collector, start the binary, make a request, verify spans arrive at the collector"
    expected: "Spans appear at the OTel Collector when the endpoint is configured"
    why_human: "The Some-endpoint path (actual OTLP exporter construction and span export) requires a live collector and is untested."
---

# Phase 2: HTTP Serving and Observability — Verification Report

**Phase Goal:** As a platform operator, I want to deploy Hephaestus as a Kubernetes pod and send HTTP inference requests, so that I can serve model inference with health probes, metrics, and tracing in production.
**Verified:** 2026-08-24T23:40:00Z
**Status:** human_needed
**Re-verification:** Yes — after gap closure (Plan 02-03, OBSV-02 structured logging)

## Re-Verification Summary

The prior verification (2026-08-24T22:50:18Z) found status `gaps_found` with one BLOCKER: no per-request structured log event was ever emitted, and `tower_http::trace::TraceLayer` was declared as a workspace dependency but never wired. Gap closure plan 02-03 addressed this directly. This re-verification confirms:

1. **`handlers.rs::infer()` now emits tracing events on all three exit paths** — `tracing::info!` on success (line 115-120) with `model_id`, `latency_ms`, `status = "success"`; `tracing::warn!` on the pipeline-error path (line 91-96) with `status = "error"`; `tracing::warn!` on the timeout path (line 103-108) with `status = "timeout"`. All three carry `model_id` (via `%state.model_id()`) and `latency_ms` computed from `request_start.elapsed()`.
2. **`tower_http::trace::TraceLayer` is wired into the router** — `routes.rs` imports `tower_http::trace::TraceLayer` and applies `.layer(TraceLayer::new_for_http())` in `build_router()` before `.with_state(state)`.
3. **`tower-http` is now a real dependency of `hephaestus-api`** — added to `crates/hephaestus-api/Cargo.toml` as `tower-http.workspace = true` (previously declared only at the workspace root and never consumed — flagged as an anti-pattern in the prior verification).
4. **`tests/tracing.rs::structured_logs_contain_model_id` is implemented and no longer `#[ignore]`d** — it scopes a test-local subscriber via `tracing::subscriber::with_default` (avoiding conflict with the other test's global `telemetry::init` subscriber), captures JSON output through a custom `TestWriter`/`MakeWriter`, and asserts `parsed["fields"]["model_id"]`, `parsed["fields"]["latency_ms"]`, and `parsed["fields"]["status"]` all have the expected values.

Live verification: `cargo test -p hephaestus-api --test tracing -- --nocapture` runs 2 tests, both pass, 0 ignored. Full workspace test run (`cargo test --workspace`) confirms no regressions: `hephaestus` bin 6/6, `hephaestus_api` lib 10/10, `tests/tracing.rs` 2/2 (was 1/1 + 1 ignored), all other suites unchanged. `cargo build --workspace` and `cargo clippy --workspace -- -D warnings` both exit 0.

**Gap closed:** OBSV-02 (Structured JSON logs include model_id, latency, and status fields on every request) — now ✓ VERIFIED with a passing automated test asserting on JSON field content, not just code presence.

**No regressions found.** No new `TBD`/`FIXME`/`XXX`/`TODO`/`HACK`/`PLACEHOLDER` markers introduced in the modified files. The `#[tracing::instrument(skip(state, req), ...)]` attribute on `infer()` (WR-04 fix from an earlier code review) still skips `req` from the span — the new `info!`/`warn!` events only log `model_id`, `latency_ms`, and `status`, no raw request text, so no PII-logging regression was introduced.

**Gaps remaining:** None new. The 5 items that were `PRESENT_BEHAVIOR_UNVERIFIED` in the prior verification (API-01, API-03, API-04, CORE-04, OBSV-03 behavioral proof) are untouched by plan 02-03 — they were out of scope for this gap closure and remain unverified by automated tests (all gated on `#[ignore]`d tests requiring a model fixture, a live signal-handling harness, or a live OTel collector, none of which exist in this repo/environment). These carry forward unchanged and route this phase to `human_needed` rather than `passed`.

## Goal Achievement

### Observable Truths

| # | Truth | Requirement | Status | Evidence |
|---|-------|-------------|--------|----------|
| 1 | POST /infer returns JSON classification (label, score, model_id, latency_ms) | API-01 | ⚠️ PRESENT_BEHAVIOR_UNVERIFIED | `handlers.rs::infer` implemented and wired; only request/response struct (de)serialization is unit-tested. Full pipeline round trip requires model files not present in this repo. |
| 2 | GET /healthz/live returns 200 immediately with model_id, uptime_s | API-02 | ✓ VERIFIED | `handlers.rs::liveness` is a pure, branchless read of `state.model_id`/`state.start_time`; wired at `routes.rs:26`. No state-dependent logic to leave unverified. |
| 3 | GET /healthz/ready returns 503 before warmup, 200 after (state transition) | API-03 | ⚠️ PRESENT_BEHAVIOR_UNVERIFIED | `handlers.rs::readiness` reads `state.ready` AtomicBool and branches correctly in code, but no test constructs an `AppState` and calls it in both states — `tests/health.rs` is entirely `#[ignore]`d. |
| 4 | SIGTERM flips readiness to 503 and drains in-flight requests within SHUTDOWN_TIMEOUT_SECS | API-04 | ⚠️ PRESENT_BEHAVIOR_UNVERIFIED | `main.rs::shutdown_signal` + drain watchdog task implemented; `tests/shutdown.rs` entirely `#[ignore]`d; no automated evidence the signal → flag flip → drain → force-exit chain actually works. |
| 5 | Requests exceeding REQUEST_TIMEOUT_SECS return HTTP 504 INFERENCE_TIMEOUT | CORE-04 | ⚠️ PRESENT_BEHAVIOR_UNVERIFIED | `handlers.rs::infer` wraps the pipeline call in `tokio::time::timeout(state.request_timeout(), ...)` and now emits a `tracing::warn!` with `status="timeout"` on that path; only the `ApiError::Timeout` → 504 response *mapping* is unit-tested, not the actual timeout race. `tests/metrics.rs::request_timeout_returns_504` is `#[ignore]`d. |
| 6 | GET /metrics returns Prometheus text with hephaestus_stage_duration_seconds / hephaestus_request_duration_seconds histograms carrying model_id labels | OBSV-01 | ✓ VERIFIED | `tests/metrics.rs::metrics_endpoint_returns_prometheus_text` (not ignored, passes) installs the recorder, records via `StageTimer`, renders, and asserts the exposition text contains both histograms, `hephaestus_requests_total`, `model_id="test-model"`, and `stage="tokenization"`. |
| 7 | Structured JSON logs include model_id, latency, and status fields on every request | OBSV-02 | ✓ **VERIFIED (gap closed)** | `handlers.rs::infer` emits `tracing::info!`/`tracing::warn!` on success, error, and timeout paths, each carrying `model_id`, `latency_ms`, `status`. `tests/tracing.rs::structured_logs_contain_model_id` captures real JSON output via a scoped subscriber and asserts on `parsed["fields"]["model_id"|"latency_ms"|"status"]`; test passes, not ignored. `tower_http::trace::TraceLayer` additionally wired for HTTP-level access logs on every route. |
| 8 | When OTEL_EXPORTER_OTLP_ENDPOINT is set, spans export via OTLP; unset → JSON logs only | OBSV-03 | ⚠️ PRESENT_BEHAVIOR_UNVERIFIED | `telemetry.rs::init` correctly builds `Option<OpenTelemetryLayer>` gated on `otel_endpoint`; the `None` path is tested (`telemetry_init_without_otel_does_not_panic`), the `Some` path (real OTLP export) requires a collector and is untested. |

**Score:** 3/8 truths verified (5 present, behavior-unverified; 0 failed)

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/hephaestus-api/src/handlers.rs` | `infer`, `liveness`, `readiness` handlers; per-request structured log events | ✓ VERIFIED | All three `pub async fn` present, exported, wired into router. `tracing::info!`/`warn!` calls added on all 3 exit paths of `infer()` (3 total call sites, confirmed via grep). |
| `crates/hephaestus-api/src/routes.rs` | `build_router` mounting all endpoints + TraceLayer | ✓ VERIFIED | Mounts `POST /infer`, `GET /healthz/live`, `GET /healthz/ready`, `GET /metrics`; `.layer(TraceLayer::new_for_http())` applied before `.with_state(state)`. |
| `crates/hephaestus-api/Cargo.toml` | `tower-http` as a real dependency | ✓ VERIFIED | `tower-http.workspace = true` added under `[dependencies]`. |
| `crates/hephaestus-api/tests/tracing.rs` | Passing test asserting JSON log field presence | ✓ VERIFIED | `structured_logs_contain_model_id` implemented with `TestWriter`/`MakeWriter` + scoped subscriber; asserts on `model_id`, `latency_ms`, `status`; not `#[ignore]`d; passes. |
| `crates/hephaestus-api/src/state.rs` | `AppState` with pipeline Mutex, readiness flag, model metadata, metrics handle | ✓ VERIFIED | Unchanged from prior verification — confirmed present. |
| `crates/hephaestus-api/src/error.rs` | `ApiError` + `CoreError` → HTTP mapping per D-03 | ✓ VERIFIED | Unchanged from prior verification — confirmed present. |
| `crates/hephaestus-api/src/metrics.rs` | `StageTimer` deep-module abstraction, `install_recorder`, `metrics_handler` | ✓ VERIFIED | Unchanged from prior verification — confirmed present. |
| `crates/hephaestus-api/src/telemetry.rs` | Layered subscriber with conditional OTel export | ✓ VERIFIED | Unchanged from prior verification — confirmed present. |

### Key Link Verification

| From | To | Via | Status | Details |
|------|-----|-----|--------|---------|
| `handlers.rs` | `hephaestus-core/pipeline.rs` | `Pipeline::prepare()` + `execute()` behind `tokio::sync::Mutex` | ✓ WIRED | Unchanged — `state.lock_pipeline().await` then `timer.time(...)`. |
| `main.rs` | `routes.rs` | `build_router(state)` before `axum::serve()` | ✓ WIRED | Unchanged. |
| `handlers.rs` | `state.rs` | `State<Arc<AppState>>` extractor | ✓ WIRED | Unchanged. |
| `handlers.rs` | `metrics.rs` | `StageTimer::time()` wraps prepare/execute | ✓ WIRED | Unchanged. |
| `main.rs` | `telemetry.rs` | `telemetry::init()` replaces inline tracing setup | ✓ WIRED | Unchanged. |
| `metrics.rs` | `routes.rs` | `metrics_handler` reads `PrometheusHandle` from `AppState`, rendered at `/metrics` | ✓ WIRED | Unchanged. |
| `handlers.rs` | `telemetry`/tracing output | Per-request structured log with model_id/latency/status | ✓ **WIRED (was NOT WIRED)** | `tracing::info!`/`warn!` events in `infer()` flow through the `telemetry.rs` JSON fmt layer; confirmed by a passing test that captures and parses the actual JSON output. |
| `routes.rs` | `tower_http::trace` | `TraceLayer::new_for_http()` applied in `build_router` | ✓ **WIRED (new)** | HTTP-level access logging (method, URI, status, latency) now active for all routes. |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Workspace builds | `cargo build --workspace` | Exit 0, no errors | ✓ PASS |
| Lint | `cargo clippy --workspace -- -D warnings` | Exit 0, no warnings | ✓ PASS |
| Workspace tests (single full run) | `cargo test --workspace` | All suites green: `hephaestus` bin 6/6, `hephaestus_api` lib 10/10, `tests/api.rs` 0/2 (ignored), `tests/health.rs` 0/2 (ignored), `tests/metrics.rs` 1/1 + 2 ignored, `tests/shutdown.rs` 0/2 (ignored), `tests/tracing.rs` **2/2, 0 ignored** (was 1/1 + 1 ignored), `hephaestus_core` 10/10 | ✓ PASS — no regressions, OBSV-02 test now runs |
| Structured per-request log content | `cargo test -p hephaestus-api --test tracing -- --nocapture` | Both tests pass; captured JSON line shows `{"level":"INFO",...,"model_id":"test-model"}` style output and the field-presence assertions succeed | ✓ PASS — confirms Gap #1 (OBSV-02) is closed |
| `tracing::info!`/`warn!` call sites in handlers.rs | `grep -c 'tracing::info!\|tracing::warn!' crates/hephaestus-api/src/handlers.rs` | 3 | ✓ PASS (1 info for success, 2 warn for error+timeout, matches plan acceptance criteria) |
| `TraceLayer` referenced in routes.rs | `grep -c 'TraceLayer' crates/hephaestus-api/src/routes.rs` | 2 (import + usage) | ✓ PASS |
| Live end-to-end HTTP server against a real model | Would require `MODEL_PATH` pointing at real `.onnx` + `tokenizer.json` + `config.json` | No fixture found anywhere in repo | ? SKIP — no runnable model fixture in this environment (unchanged from prior verification) |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|-------------|--------------|--------|----------|
| API-01 | 02-01 | HTTP REST inference (POST /infer, JSON) | ✓ SATISFIED (code) / needs human E2E | `handlers.rs::infer`, `routes.rs` |
| API-02 | 02-01 | Liveness probe responds immediately | ✓ SATISFIED | `handlers.rs::liveness` |
| API-03 | 02-01 | Readiness probe gates on model load | ✓ SATISFIED (code) / needs human E2E for the state transition | `handlers.rs::readiness`, `state.rs::ready` |
| API-04 | 02-01 | Graceful shutdown drains in-flight requests | ✓ SATISFIED (code) / needs human E2E | `main.rs::shutdown_signal`, drain watchdog |
| CORE-04 | 02-01 | Request timeout enforcement | ✓ SATISFIED (code) / needs human E2E | `handlers.rs::infer` `tokio::time::timeout` |
| OBSV-01 | 02-02 | Prometheus metrics endpoint | ✓ SATISFIED — behaviorally tested | `metrics.rs`, `tests/metrics.rs` |
| OBSV-02 | 02-02, 02-03 (gap closure) | Structured JSON logs with request context (model ID, latency, status) | ✓ **SATISFIED — behaviorally tested** | `handlers.rs::infer` tracing events, `tests/tracing.rs::structured_logs_contain_model_id` (passes, asserts on JSON field content) |
| OBSV-03 | 02-02 | OpenTelemetry distributed tracing with span propagation | ✓ SATISFIED (code, None-path tested) / needs human E2E for Some-path | `telemetry.rs::init` |

REQUIREMENTS.md marks OBSV-02 as `[x]` Complete — this re-verification now confirms that marking is accurate. No orphaned requirements: the 8 IDs in the phase's plans (`API-01..04, CORE-04, OBSV-01..03`) exactly match the IDs listed for Phase 2 in `ROADMAP.md` and `REQUIREMENTS.md`.

### Anti-Patterns Found

None. The previously flagged info-level anti-pattern (`tower-http` declared at workspace root but never consumed by `hephaestus-api`) is resolved — `tower-http.workspace = true` is now a real dependency and `tower_http::trace::TraceLayer` is imported and used in `routes.rs`.

No `TBD`/`FIXME`/`XXX`/`TODO`/`HACK`/`PLACEHOLDER` markers found in any file modified by plan 02-03.

### Gaps Summary

**No gaps remain that block Phase 2 completion.** The OBSV-02 BLOCKER identified in the prior verification is closed: `infer()` now emits `tracing::info!`/`tracing::warn!` events with `model_id`, `latency_ms`, and `status` on every exit path (success, pipeline error, timeout), `tower_http::trace::TraceLayer` provides HTTP-level access logging for all routes, and an automated test (`structured_logs_contain_model_id`) proves the JSON output actually contains these fields — not just that the code compiles.

Five items remain `PRESENT_BEHAVIOR_UNVERIFIED` (API-01, API-03, API-04, CORE-04, OBSV-03's OTLP-export path). These were out of scope for the OBSV-02 gap closure and are unchanged from the prior verification: the corresponding integration tests (`tests/api.rs`, `tests/health.rs`, `tests/shutdown.rs`, `tests/metrics.rs::request_timeout_returns_504`) are all `#[ignore]`d pending a real model fixture (no `.onnx`/`tokenizer.json` in this repo/environment), a live signal-handling test harness, or a live OTel Collector. The code paths are implemented and match the plan's design, but have zero automated proof of correct runtime behavior. These route the phase to `human_needed` rather than `passed` — a human (or a future phase that adds model fixtures / a test harness) should exercise them before treating Phase 2 as fully production-verified.

---

*Verified: 2026-08-24T23:40:00Z*
*Verifier: Claude (gsd-verifier)*
