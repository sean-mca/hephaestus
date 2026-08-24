# Phase 2: HTTP Serving and Observability - Pattern Map

**Mapped:** 2026-08-23
**Files analyzed:** 10 new/modified files
**Analogs found:** 4 / 10

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|-------------------|------|-----------|----------------|---------------|
| `crates/hephaestus-api/src/lib.rs` | module-root | re-export | `crates/hephaestus-core/src/lib.rs` | exact |
| `crates/hephaestus-api/src/routes.rs` | controller | request-response | none | -- |
| `crates/hephaestus-api/src/handlers.rs` | controller | request-response | none | -- |
| `crates/hephaestus-api/src/error.rs` | utility | transform | `crates/hephaestus-core/src/error.rs` | role-match |
| `crates/hephaestus-api/src/state.rs` | provider | shared-state | none | -- |
| `crates/hephaestus-api/src/metrics.rs` | utility | event-driven | none | -- |
| `crates/hephaestus-api/src/telemetry.rs` | config | event-driven | none | -- |
| `crates/hephaestus-api/Cargo.toml` | config | -- | `crates/hephaestus-core/Cargo.toml` | role-match |
| `crates/hephaestus/src/main.rs` (modify) | binary | request-response | `crates/hephaestus/src/main.rs` | exact (self) |
| `crates/hephaestus/src/config.rs` (modify) | config | -- | `crates/hephaestus/src/config.rs` | exact (self) |

## Pattern Assignments

### `crates/hephaestus-api/src/lib.rs` (module-root, re-export)

**Analog:** `crates/hephaestus-core/src/lib.rs`

**Pattern** (lines 1-14):
```rust
//! Core inference pipeline for Hephaestus ONNX runtime.
//!
//! This crate defines the [`Pipeline`] trait contract and profile
//! implementations (starting with [`ClassifierPipeline`]). Callers
//! interact only through `prepare()` and `execute()` -- all internal
//! tokenization, tensor construction, and ONNX inference details are
//! hidden behind the trait boundary.

pub mod error;
pub mod pipeline;
pub(crate) mod postprocess;

pub use error::CoreError;
pub use pipeline::{ClassifierOutput, ClassifierPipeline, Pipeline, PreparedInput};
```

**Apply:** Same structure -- doc comment describing crate purpose, public module declarations, selective re-exports. New crate re-exports `routes::build_router`, `state::AppState`, `telemetry`, `metrics`.

---

### `crates/hephaestus-api/src/error.rs` (utility, transform)

**Analog:** `crates/hephaestus-core/src/error.rs`

**Error enum pattern** (lines 1-35):
```rust
//! Error types for the core inference pipeline.

use thiserror::Error;

/// Errors produced by the core inference pipeline.
#[derive(Error, Debug)]
pub enum CoreError {
    /// Failed to tokenize input text.
    #[error("tokenization failed: {0}")]
    Tokenization(String),

    /// Failed to run ONNX inference.
    #[error("inference failed: {0}")]
    Inference(String),

    // ... more variants ...
}
```

**Apply:** The API error type wraps `CoreError` and maps to HTTP responses. Use `thiserror` for the API error enum. Implement `axum::response::IntoResponse` to produce structured JSON error bodies per D-03. Map `CoreError::Tokenization` to 422, `CoreError::Inference` to 500, etc.

---

### `crates/hephaestus/src/config.rs` (modify -- extend Config struct)

**Analog:** self (lines 24-44)

**Config struct with envy pattern:**
```rust
#[derive(Deserialize, Debug)]
pub struct Config {
    /// Model identifier (required).
    pub model_id: String,

    /// Local directory containing model files (optional).
    #[serde(default)]
    pub model_path: Option<String>,

    /// ONNX execution provider (default: `"cpu"`).
    #[serde(default = "default_ep")]
    pub execution_provider: String,

    /// Log level (default: `"info"`).
    #[serde(default = "default_log_level")]
    pub log_level: String,

    /// Custom warmup inference text (optional).
    #[serde(default)]
    pub warmup_input: Option<String>,
}
```

**Apply:** Add new fields following the same pattern -- `#[serde(default = "default_X")]` for fields with defaults, doc comments on each field:
- `port: u16` with default `8080`
- `request_timeout_secs: u64` with default `30`
- `shutdown_timeout_secs: u64` with default `30`
- `otel_endpoint: Option<String>` with `#[serde(default)]`

**Test helper pattern** (lines 110-118):
```rust
fn config_with_model_path(model_path: Option<&str>) -> Config {
    Config {
        model_id: "test-model".to_string(),
        model_path: model_path.map(String::from),
        execution_provider: "cpu".to_string(),
        log_level: "info".to_string(),
        warmup_input: None,
    }
}
```

**Apply:** Update test helper to include new fields with sensible defaults.

---

### `crates/hephaestus/src/main.rs` (modify -- async main)

**Analog:** self (lines 1-60)

**Current structure:**
```rust
mod config;

use anyhow::Context;
use hephaestus_core::{ClassifierPipeline, Pipeline};

fn main() -> Result<(), anyhow::Error> {
    // 1. Load typed configuration from environment variables.
    let config = config::Config::from_env()?;

    // 2. Initialize structured JSON logging.
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&config.log_level));
    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .json()
        .init();

    // ... pipeline construction, warmup ...

    // 6. Report ready (Phase 2 adds HTTP server start here).
    tracing::info!("hephaestus ready");
    Ok(())
}
```

**Apply:** Convert `fn main()` to `#[tokio::main] async fn main()`. Replace step 2 (tracing init) with `hephaestus_api::telemetry::init(&config)` call inside the tokio runtime. Replace step 6 with `Arc<AppState>` construction, readiness flip, `axum::serve()` with graceful shutdown. Keep the numbered-step comment style and `anyhow::Context` pattern.

---

### `crates/hephaestus-api/Cargo.toml` (new crate config)

**Analog:** `crates/hephaestus-core/Cargo.toml` (not read, but workspace pattern from root `Cargo.toml`)

**Workspace dependency pattern** (root Cargo.toml lines 1-33):
```toml
[workspace]
members = ["crates/*"]
resolver = "3"

[workspace.dependencies]
# ONNX inference
ort = "2.0.0-rc.13"
ndarray = "0.17"
# ...
```

**Apply:** New crate Cargo.toml uses `dep.workspace = true` for shared deps (serde, serde_json, tracing, anyhow, thiserror, tokio). Add axum, tower, tower-http, metrics, metrics-exporter-prometheus, opentelemetry, opentelemetry_sdk, opentelemetry-otlp, tracing-opentelemetry to workspace deps first, then reference from crate. Depend on `hephaestus-core` via path.

---

## Shared Patterns

### Error Handling Convention
**Source:** `crates/hephaestus-core/src/error.rs` + `crates/hephaestus/src/main.rs`
**Apply to:** All files in hephaestus-api (thiserror) and main.rs (anyhow)

- Library crates (`hephaestus-core`, `hephaestus-api`): `thiserror` derive for error enums
- Binary crate (`hephaestus`): `anyhow::Context` for error propagation with `.context("message")?`

### Doc Comment Style
**Source:** All existing files
**Apply to:** All new files

Every file starts with `//!` module-level doc comment. Every public type and method has `///` doc comment with `# Errors` section where applicable.

### Test Structure
**Source:** `crates/hephaestus/src/config.rs` lines 103-214, `crates/hephaestus-core/src/pipeline.rs` lines 278-363
**Apply to:** All new files with testable logic

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptive_test_name() {
        // Arrange
        // ...

        // Act
        // ...

        // Assert
        // ...
    }
}
```

Pattern: Arrange/Act/Assert comments, descriptive test names, helper constructors for test fixtures.

### Tracing Pattern
**Source:** `crates/hephaestus/src/main.rs` lines 25-29
**Apply to:** All handlers and significant operations

```rust
tracing::info!(
    model_id = %config.model_id,
    execution_provider = %config.execution_provider,
    "configuration loaded"
);
```

Structured fields with `%` formatting, message as last argument.

## No Analog Found

| File | Role | Data Flow | Reason |
|------|------|-----------|--------|
| `crates/hephaestus-api/src/routes.rs` | controller | request-response | No HTTP layer exists yet; use RESEARCH.md Pattern "Router Construction" |
| `crates/hephaestus-api/src/handlers.rs` | controller | request-response | No HTTP handlers exist yet; use RESEARCH.md "Complete Inference Handler" |
| `crates/hephaestus-api/src/state.rs` | provider | shared-state | No shared state pattern exists yet; use RESEARCH.md Pattern 1 "Shared Application State" |
| `crates/hephaestus-api/src/metrics.rs` | utility | event-driven | No metrics code exists yet; use RESEARCH.md Pattern 4 "Deep-Module Timer Abstraction" |
| `crates/hephaestus-api/src/telemetry.rs` | config | event-driven | No OTel code exists yet; use RESEARCH.md Pattern 2 "Conditional OTel Layer" |

## Metadata

**Analog search scope:** `crates/` directory (9 Rust source files)
**Files scanned:** 9
**Pattern extraction date:** 2026-08-23
