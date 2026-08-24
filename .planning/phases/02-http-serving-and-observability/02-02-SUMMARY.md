---
phase: 02-http-serving-and-observability
plan: 02
subsystem: observability
tags: [prometheus, opentelemetry, metrics, tracing, structured-logging, axum]

requires:
  - phase: 02-http-serving-and-observability (plan 01)
    provides: axum HTTP router, handlers, AppState, graceful shutdown

provides:
  - Prometheus metrics with per-stage histograms via deep-module StageTimer abstraction
  - GET /metrics scrape endpoint for Prometheus
  - Conditional OpenTelemetry distributed tracing via OTLP gRPC export
  - Structured JSON logging with span context (OBSV-02)
  - Telemetry initialization module with layered tracing subscriber

affects: [phase-03-model-resolution, phase-04-batching]

tech-stack:
  added: [metrics 0.24, metrics-exporter-prometheus 0.18, opentelemetry 0.32, opentelemetry_sdk 0.32, opentelemetry-otlp 0.32, tracing-opentelemetry 0.33]
  patterns: [deep-module timer abstraction, conditional OTel layer via Option, OnceLock for provider shutdown]

key-files:
  created:
    - crates/hephaestus-api/src/metrics.rs
    - crates/hephaestus-api/src/telemetry.rs
  modified:
    - crates/hephaestus-api/src/handlers.rs
    - crates/hephaestus-api/src/state.rs
    - crates/hephaestus-api/src/routes.rs
    - crates/hephaestus-api/src/lib.rs
    - crates/hephaestus/src/main.rs
    - crates/hephaestus/src/config.rs
    - Cargo.toml
    - crates/hephaestus-api/Cargo.toml
    - crates/hephaestus-api/tests/metrics.rs
    - crates/hephaestus-api/tests/tracing.rs

key-decisions:
  - "OTel v0.32 removed global shutdown_tracer_provider; store SdkTracerProvider in OnceLock for clean shutdown"
  - "Metrics integration test uses install_recorder + StageTimer directly, avoiding unsafe zeroed pipeline"
  - "Wired install_recorder in main.rs during Task 1 (Rule 3) to unblock build before Task 2 telemetry refactor"

patterns-established:
  - "Deep-module StageTimer: callers never touch metrics crate directly; all recording goes through time() and finish_request()"
  - "Conditional OTel layer: Option<Layer> in subscriber registry; None passes through with zero overhead"
  - "OnceLock<SdkTracerProvider> for global provider storage and clean shutdown"

requirements-completed: [OBSV-01, OBSV-02, OBSV-03]

coverage:
  - id: D1
    description: "GET /metrics returns Prometheus exposition text with hephaestus_stage_duration_seconds and hephaestus_request_duration_seconds histograms with model_id labels"
    requirement: "OBSV-01"
    verification:
      - kind: integration
        ref: "crates/hephaestus-api/tests/metrics.rs#metrics_endpoint_returns_prometheus_text"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-api/src/metrics.rs#stage_timer_time_returns_closure_result"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-api/src/metrics.rs#stage_timer_new_accepts_model_id"
        status: pass
    human_judgment: false
  - id: D2
    description: "Structured JSON logs include model_id, latency, and status fields on every request via tracing span context"
    requirement: "OBSV-02"
    verification:
      - kind: integration
        ref: "crates/hephaestus-api/tests/tracing.rs#telemetry_init_without_otel_does_not_panic"
        status: pass
    human_judgment: true
    rationale: "Structured log field presence requires inspecting JSON log output at runtime; integration test verifies init does not panic but does not assert on log field content"
  - id: D3
    description: "When OTEL_EXPORTER_OTLP_ENDPOINT is set, tracing spans export via OTLP gRPC; when unset, only JSON logs are active"
    requirement: "OBSV-03"
    verification:
      - kind: integration
        ref: "crates/hephaestus-api/tests/tracing.rs#telemetry_init_without_otel_does_not_panic"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-api/src/telemetry.rs#shutdown_without_init_does_not_panic"
        status: pass
    human_judgment: true
    rationale: "Full OTel export requires a running OTel Collector; unit tests verify the None-endpoint path only"

duration: 8min
completed: 2026-08-24
status: complete
---

# Phase 02 Plan 02: Observability Stack Summary

**Prometheus metrics with deep-module StageTimer, conditional OTel distributed tracing via OTLP, and structured JSON logging with span context**

## Performance

- **Duration:** 8 min
- **Started:** 2026-08-24T22:28:58Z
- **Completed:** 2026-08-24T22:37:00Z
- **Tasks:** 2
- **Files modified:** 12

## Accomplishments

- Deep-module StageTimer abstraction hides all metrics crate interaction; handlers record per-stage (tokenization, inference) and per-request histograms without touching metrics macros directly (D-09)
- GET /metrics endpoint returns Prometheus exposition text with hephaestus_stage_duration_seconds, hephaestus_request_duration_seconds, and hephaestus_requests_total metrics carrying model_id labels (D-08, D-10, OBSV-01)
- Conditional OpenTelemetry layer activates OTLP span export when OTEL_EXPORTER_OTLP_ENDPOINT is set; falls back to JSON-only logging with zero overhead when unset (D-11, OBSV-03)
- Structured JSON logging via tracing-subscriber with target and span context fields (OBSV-02)
- Telemetry init moved from inline in main.rs to reusable telemetry::init() module

## Task Commits

Each task was committed atomically:

1. **Task 1: Prometheus metrics with deep-module StageTimer and /metrics endpoint** - `6db66a3` (feat)
2. **Task 2: Conditional OTel tracing and structured logging upgrade** - `29e0ff0` (feat)

## Files Created/Modified

- `crates/hephaestus-api/src/metrics.rs` - StageTimer abstraction, install_recorder, metrics_handler
- `crates/hephaestus-api/src/telemetry.rs` - Layered tracing subscriber with conditional OTel export
- `crates/hephaestus-api/src/handlers.rs` - Infer handler uses StageTimer for per-stage timing
- `crates/hephaestus-api/src/state.rs` - AppState gains metrics_handle field
- `crates/hephaestus-api/src/routes.rs` - GET /metrics route mounted
- `crates/hephaestus-api/src/lib.rs` - pub mod metrics and telemetry with re-exports
- `crates/hephaestus/src/main.rs` - telemetry::init() replaces inline setup; install_recorder wired; shutdown() on exit
- `crates/hephaestus/src/config.rs` - Removed dead_code expect on otel_exporter_otlp_endpoint
- `Cargo.toml` - metrics, metrics-exporter-prometheus, opentelemetry, opentelemetry_sdk, opentelemetry-otlp, tracing-opentelemetry workspace deps
- `crates/hephaestus-api/Cargo.toml` - All new deps added
- `crates/hephaestus-api/tests/metrics.rs` - Integration test verifying Prometheus text output with all custom metrics and labels
- `crates/hephaestus-api/tests/tracing.rs` - Integration test verifying telemetry init with None endpoint

## Decisions Made

- **OTel v0.32 API change:** `opentelemetry::global::shutdown_tracer_provider()` was removed in v0.32. Stored `SdkTracerProvider` in a `OnceLock<SdkTracerProvider>` static for clean shutdown via `provider.shutdown()`.
- **TracerProvider trait import:** `SdkTracerProvider::tracer()` requires `opentelemetry::trace::TracerProvider` trait in scope (not automatically available in v0.32).
- **Metrics integration test approach:** Used `install_recorder()` + `StageTimer` directly to verify Prometheus text output, avoiding the need to construct a full AppState with a real ClassifierPipeline.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Wired install_recorder in main.rs during Task 1**
- **Found during:** Task 1 (metrics setup)
- **Issue:** Adding metrics_handle field to AppState caused main.rs to fail compilation because it didn't construct the field
- **Fix:** Added install_recorder() call and metrics_handle field to AppState construction in main.rs
- **Files modified:** crates/hephaestus/src/main.rs
- **Verification:** cargo build --workspace exits 0
- **Committed in:** 6db66a3 (Task 1 commit)

**2. [Rule 1 - Bug] Fixed OTel v0.32 API incompatibility**
- **Found during:** Task 2 (telemetry init)
- **Issue:** Research docs referenced `opentelemetry::global::shutdown_tracer_provider()` which was removed in v0.32. Also `SdkTracerProvider::tracer()` requires explicit trait import.
- **Fix:** Stored provider in OnceLock, call `provider.shutdown()` directly. Added `use opentelemetry::trace::TracerProvider as _` import.
- **Files modified:** crates/hephaestus-api/src/telemetry.rs
- **Verification:** cargo build + cargo test + cargo clippy all pass
- **Committed in:** 29e0ff0 (Task 2 commit)

---

**Total deviations:** 2 auto-fixed (1 blocking, 1 bug)
**Impact on plan:** Both fixes were necessary for compilation and correctness. No scope creep.

## Issues Encountered

- Clippy flagged nested `if let` in shutdown() as collapsible; fixed by using chained `if let && let` syntax (Rust 2024 edition).

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Phase 02 observability stack is complete: metrics, tracing, structured logging all wired
- Phase 03 (model resolution) can add model-acquisition timing via StageTimer
- OTel export requires an OTel Collector in the k8s cluster (deployment is infra, not Hephaestus concern)

## Self-Check: PASSED

All 10 key files verified present. Both task commits (6db66a3, 29e0ff0) verified in git log.

---
*Phase: 02-http-serving-and-observability*
*Completed: 2026-08-24*
