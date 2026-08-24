---
phase: 02-http-serving-and-observability
fixed_at: 2026-08-24T19:15:00Z
review_path: .planning/phases/02-http-serving-and-observability/02-REVIEW.md
iteration: 1
findings_in_scope: 6
fixed: 5
skipped: 1
status: partial
---

# Phase 02: Code Review Fix Report

**Fixed at:** 2026-08-24T19:15:00Z
**Source review:** .planning/phases/02-http-serving-and-observability/02-REVIEW.md
**Iteration:** 1

**Summary:**
- Findings in scope: 6 (1 critical, 5 warning)
- Fixed: 5
- Skipped: 1

## Fixed Issues

### CR-01: OTLP endpoint parameter silently discarded -- exporter ignores configured endpoint

**Files modified:** `crates/hephaestus-api/src/telemetry.rs`
**Commit:** ebb7640
**Applied fix:** Changed `_endpoint` to `endpoint` and added `.with_endpoint(endpoint)` to the OTLP SpanExporter builder. Added `use opentelemetry_otlp::WithExportConfig` import required by the method.

### WR-01: "OTel export enabled" log emitted before subscriber is installed

**Files modified:** `crates/hephaestus-api/src/telemetry.rs`
**Commit:** db7ce20
**Applied fix:** Removed the `tracing::info!` call from inside the `if let Some(endpoint)` block (before subscriber installation) and added a unified if/else log block after `Registry::default().init()`, mirroring the pattern already used for the disabled case.

### WR-04: Tracing instrument on infer handler records user input text in spans

**Files modified:** `crates/hephaestus-api/src/handlers.rs`
**Commit:** f1ab029
**Applied fix:** Changed `#[tracing::instrument(skip(state))]` to `#[tracing::instrument(skip(state, req), fields(text_len = req.text.len()))]` on the `infer` handler. This prevents the full request text from appearing in structured logs and OTel spans while still recording the text length for diagnostics.

### WR-03: Internal error messages leak system details to HTTP clients

**Files modified:** `crates/hephaestus-api/src/error.rs`
**Commit:** e41c610
**Applied fix:** Added server-side `tracing::error!` logging for 5xx errors before response construction, then replaced the client-facing message for `Internal`, `Inference`, and `Model` error variants with a generic "internal server error" string. Non-server errors (400, 422, 503, 504) retain their descriptive messages since they contain no system internals.

### WR-05: All AppState fields are pub -- no encapsulation of safety-critical state

**Files modified:** `crates/hephaestus-api/src/state.rs`, `crates/hephaestus-api/src/handlers.rs`, `crates/hephaestus-api/src/metrics.rs`, `crates/hephaestus/src/main.rs`
**Commit:** 0d78347
**Applied fix:** Made all `AppState` fields private and added a `new()` constructor plus controlled accessors: `is_ready()`, `set_ready()`, `model_id()`, `uptime_secs()`, `request_timeout()`, `render_metrics()`, and `lock_pipeline()`. Updated all call sites in handlers, metrics, and the binary crate. Removed now-unused `AtomicBool`, `Ordering`, `Instant`, and `Mutex` imports from consuming modules. All 10 unit tests pass unchanged.

## Skipped Issues

### WR-02: Synchronous CPU-bound inference blocks the async runtime

**File:** `crates/hephaestus-api/src/handlers.rs:75-81`
**Reason:** Design-level restructuring required. The suggested `spawn_blocking` fix cannot be applied without changing the Mutex strategy: `tokio::sync::MutexGuard` borrows from the Mutex and does not satisfy the `'static` bound required by `spawn_blocking`. Viable approaches (switching to `Arc<Mutex<ClassifierPipeline>>` for `OwnedMutexGuard`, or switching to `std::sync::Mutex` for synchronous locking inside the blocking closure) both alter the concurrency model across multiple files and risk introducing subtle deadlock or contention issues. The review itself acknowledges this: "This requires `ClassifierPipeline` to be `Send`, and the `Mutex` guard interaction needs careful restructuring." Recommended for a dedicated follow-up with design discussion rather than an automated fix pass.
**Original issue:** `pipeline.prepare()` (tokenization) and `pipeline.execute()` (ONNX inference) are synchronous CPU-bound operations that run directly on the tokio runtime thread, potentially blocking health probes and shutdown signal handling during slow inference.

---

_Fixed: 2026-08-24T19:15:00Z_
_Fixer: Claude (gsd-code-fixer)_
_Iteration: 1_
