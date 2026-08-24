---
phase: 02-http-serving-and-observability
reviewed: 2026-08-24T20:15:00Z
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
  warning: 3
  info: 3
  total: 7
status: issues_found
---

# Phase 02: Code Review Report

**Reviewed:** 2026-08-24T20:15:00Z
**Depth:** standard
**Files Reviewed:** 16
**Status:** issues_found

## Summary

The HTTP serving layer (hephaestus-api) and binary entry point (hephaestus) are well-structured with clean separation between error handling, routing, state, metrics, and telemetry. The Ousterhout deep-module principle is applied consistently (StageTimer, AppState accessors). Error responses redact internal details for 5xx errors. Path traversal mitigation is solid. However, a critical architectural flaw exists: CPU-bound ONNX inference runs synchronously on tokio worker threads, which makes the request timeout mechanism (D-14) completely ineffective and can starve the async runtime. Three additional warnings address error propagation and configuration validation gaps.

## Critical Issues

### CR-01: Synchronous CPU-bound inference blocks tokio worker thread, rendering request timeout ineffective

**File:** `crates/hephaestus-api/src/handlers.rs:74-79`
**Issue:** `pipeline.prepare()` (tokenization) and `pipeline.execute()` (ONNX inference via ort `Session::run()`) are synchronous, CPU-bound calls executed directly on a tokio worker thread inside `tokio::time::timeout`. The timeout mechanism only triggers at `.await` points. After the single await point (`state.lock_pipeline().await`) on line 75, the remaining computation runs synchronously with no yield points.

This produces two concrete failures:

1. **Timeout never fires for slow inference.** `tokio::time::timeout` internally polls the inner future first; if the inner future returns `Ready` (which it always will, since the sync code eventually completes), the timeout check never triggers `Err(Elapsed)`. A 45-second inference with a 30-second timeout returns `Ok(Ok(output))` -- the timeout is silently bypassed.

2. **Worker thread starvation.** While inference runs, the tokio worker thread is blocked. Health probes (`/healthz/live`, `/healthz/ready`), metrics scraping (`/metrics`), and the graceful shutdown signal handler all compete for the remaining worker threads. Under concurrent inference load, liveness probes can fail, causing Kubernetes to kill a healthy pod.

**Fix:** Move CPU-bound work to tokio's blocking thread pool using `spawn_blocking`. This creates a `.await` point (the JoinHandle) that `tokio::time::timeout` can interrupt. Because `tokio::sync::MutexGuard` borrows the Mutex and cannot satisfy `'static`, use `lock_owned()` to obtain an `OwnedMutexGuard` that can cross the spawn boundary.

Add to `AppState`:
```rust
pub async fn lock_pipeline_owned(self: &Arc<Self>) -> tokio::sync::OwnedMutexGuard<ClassifierPipeline> {
    self.pipeline.clone().lock_owned().await
}
```

Then in the handler:
```rust
let result = tokio::time::timeout(state.request_timeout(), async {
    let pipeline = state.lock_pipeline_owned().await;
    tokio::task::spawn_blocking(move || {
        let timer = StageTimer::new(model_id);
        let prepared = timer.time("tokenization", || pipeline.prepare(text))?;
        let output = timer.time("inference", || pipeline.execute(prepared))?;
        Ok::<_, ApiError>(output)
    })
    .await
    .map_err(|e| ApiError::Internal(format!("inference task panicked: {e}")))?
})
.await;
```

This requires exposing the inner `Arc<Mutex<ClassifierPipeline>>` or adding the `lock_pipeline_owned` method. The `pipeline` field in `AppState` must change from `Mutex<ClassifierPipeline>` to `Arc<Mutex<ClassifierPipeline>>` to support `lock_owned()`.

## Warnings

### WR-01: Tokenization error details returned to clients risk information disclosure

**File:** `crates/hephaestus-api/src/error.rs:79-84`
**Issue:** The `IntoResponse` implementation redacts messages for `Internal`, `Inference`, and `Model` errors (returning a generic "internal server error"), but the `Tokenization` variant passes the library error message through to the client via `other.to_string()`. While the HuggingFace tokenizers crate is unlikely to include file paths in `encode()` errors, the defense-in-depth principle requires that no internal error detail reaches clients without explicit sanitization. If the tokenizer error format changes in a future version, this becomes an information disclosure vector.

**Fix:** Either redact tokenization errors the same way as server errors, or explicitly construct a sanitized client message:
```rust
let client_message = match &self {
    Self::Internal(_) | Self::Inference(_) | Self::Model(_) => {
        "internal server error".to_string()
    }
    Self::Tokenization(_) => "tokenization failed".to_string(),
    other => other.to_string(),
};
```

### WR-02: `telemetry::init` panics on double-init instead of returning Err

**File:** `crates/hephaestus-api/src/telemetry.rs:72-76`
**Issue:** The function signature returns `Result<(), anyhow::Error>`, but line 76 calls `.init()` on the subscriber, which panics if a global subscriber was already installed. This contradicts the function's error contract. In tests or any scenario where `init` is called twice (e.g., test setup that creates multiple integration test harnesses), the process aborts with a panic instead of a recoverable error.

**Fix:** Replace `.init()` with `.try_init()` and propagate the error:
```rust
tracing_subscriber::Registry::default()
    .with(env_filter)
    .with(fmt_layer)
    .with(otel_layer)
    .try_init()
    .map_err(|e| anyhow::anyhow!("failed to initialize tracing subscriber: {e}"))?;
```

### WR-03: No validation that timeout configuration values are positive

**File:** `crates/hephaestus/src/config.rs:46-55`
**Issue:** Both `request_timeout_secs` and `shutdown_timeout_secs` accept any `u64` value including 0. A `request_timeout_secs` of 0 creates a `Duration::from_secs(0)` timeout that immediately expires at the first await point, causing every inference request to return 504. A `shutdown_timeout_secs` of 0 causes the watchdog in `main.rs:101` to call `process::exit(1)` immediately after any shutdown signal, defeating graceful drain entirely.

**Fix:** Add validation in `Config::from_env()` or a separate `validate()` method:
```rust
pub fn from_env() -> Result<Self, anyhow::Error> {
    let config = envy::from_env::<Self>()
        .context("failed to load config from environment (MODEL_ID is required)")?;
    if config.request_timeout_secs == 0 {
        bail!("REQUEST_TIMEOUT_SECS must be greater than 0");
    }
    if config.shutdown_timeout_secs == 0 {
        bail!("SHUTDOWN_TIMEOUT_SECS must be greater than 0");
    }
    Ok(config)
}
```

## Info

### IN-01: Integration test stubs have empty bodies with no assertions

**File:** `crates/hephaestus-api/tests/api.rs:9-29`, `crates/hephaestus-api/tests/health.rs:6-23`, `crates/hephaestus-api/tests/shutdown.rs:7-22`
**Issue:** Seven `#[ignore]` integration tests have completely empty bodies. While each test has a comment explaining its intent, the empty bodies could mislead contributors into thinking these are real tests that pass when `--ignored` is not used. Adding `todo!("requires model files")` in each body would make it explicit that these are unimplemented.

### IN-02: `from_env_with_defaults_has_correct_defaults` test is inherently racy

**File:** `crates/hephaestus/src/config.rs:156-174`
**Issue:** The test uses `unsafe { std::env::set_var("MODEL_ID", "test-model") }` to set a process-global environment variable. While this is the only `from_env` test in the crate (so no parallel conflict currently exists), any future test that reads environment variables in the same crate binary could race. Additionally, the test asserts `config.model_path.is_none()` which would fail if `MODEL_PATH` happened to be set in the ambient environment.

### IN-03: Watchdog `process::exit(1)` bypasses OTel span flush

**File:** `crates/hephaestus/src/main.rs:102-106`
**Issue:** When the drain timeout watchdog fires, `std::process::exit(1)` terminates the process immediately, skipping `hephaestus_api::telemetry::shutdown()` on line 117. Any buffered OTel spans are lost. This is by design (the watchdog is a last-resort forced exit), but a best-effort `telemetry::shutdown()` call before `process::exit(1)` would recover traces from the final moments of the process's life, which are often the most diagnostically valuable.

---

_Reviewed: 2026-08-24T20:15:00Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
