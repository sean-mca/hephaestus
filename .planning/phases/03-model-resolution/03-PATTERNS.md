# Phase 3: Model Resolution - Pattern Map

**Mapped:** 2026-08-24
**Files analyzed:** 8 new/modified files
**Analogs found:** 5 / 8

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|-------------------|------|-----------|----------------|---------------|
| `crates/hephaestus-resolve/src/lib.rs` | module root | re-exports | `crates/hephaestus-core/src/lib.rs` | exact |
| `crates/hephaestus-resolve/src/error.rs` | model | -- | `crates/hephaestus-core/src/error.rs` | exact |
| `crates/hephaestus-resolve/src/resolver.rs` | service | request-response | `crates/hephaestus-core/src/pipeline.rs` | role-match |
| `crates/hephaestus-resolve/src/s3.rs` | service | file-I/O | -- | no-analog |
| `crates/hephaestus-resolve/src/hf.rs` | service | file-I/O | `crates/hephaestus-core/tests/classifier_e2e.rs` | partial |
| `crates/hephaestus-resolve/src/forge.rs` | service | request-response | -- | no-analog |
| `crates/hephaestus/src/config.rs` (modify) | config | -- | self (extend existing) | exact |
| `crates/hephaestus/src/main.rs` (modify) | entrypoint | -- | self (wire resolver) | exact |

## Pattern Assignments

### `crates/hephaestus-resolve/src/lib.rs` (module root, re-exports)

**Analog:** `crates/hephaestus-core/src/lib.rs`

**Module declaration + re-export pattern** (lines 1-14):
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

**Apply to resolve crate:** Declare `pub mod error;`, `pub(crate) mod s3;`, `pub(crate) mod hf;`, `pub mod forge;` (forge is pub for Phase 5 trait). Re-export `ModelResolver`, `ResolveError`, `ForgeClient`, `StubForgeClient`.

---

### `crates/hephaestus-resolve/src/error.rs` (error types)

**Analog:** `crates/hephaestus-core/src/error.rs` (lines 1-35)

**Error enum pattern:**
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

    /// I/O error reading model files.
    #[error("i/o error")]
    Io(#[from] std::io::Error),

    /// Failed to parse JSON configuration (e.g., config.json).
    #[error("json parse error")]
    JsonParse(#[from] serde_json::Error),
}
```

**Apply to ResolveError:** Same structure with variants: `S3(String)`, `HuggingFace(String)`, `NoOnnxExport { model_id: String }`, `ForgeUnavailable { model_id: String }`, `Io(#[from] std::io::Error)`. Use `thiserror::Error` derive. Doc comment each variant.

---

### `crates/hephaestus-resolve/src/resolver.rs` (deep module service)

**Analog:** `crates/hephaestus-core/src/pipeline.rs`

**Deep module struct pattern** (lines 77-82, 83-162):
```rust
/// Text classification pipeline backed by an ONNX model.
pub struct ClassifierPipeline {
    session: Session,
    tokenizer: Tokenizer,
    id2label: Vec<String>,
}

impl ClassifierPipeline {
    /// Construct a new classifier pipeline from a model directory.
    pub fn new(model_dir: &Path) -> Result<Self, CoreError> {
        // ...multi-step internal logic hidden from caller...
        Ok(Self { session, tokenizer, id2label })
    }
}
```

**Apply to ModelResolver:** Same pattern -- struct with private fields (`s3_client`, `s3_bucket`, `hf_client`, `forge`, `cache_dir`), `new()` constructor, single `resolve()` method. All tier methods are private (`fn try_s3`, `fn try_hf`, `fn try_forge`).

**mockall pattern for internal traits** (lines 44-48 of pipeline.rs):
```rust
#[cfg_attr(test, mockall::automock(
    type Input = String;
    type Prepared = PreparedInput;
    type Output = ClassifierOutput;
))]
pub trait Pipeline { ... }
```

**Apply:** Use `#[cfg_attr(test, mockall::automock)]` on `pub(crate)` S3 and HF traits for unit testing the resolver's tier fallback logic.

---

### `crates/hephaestus-resolve/src/hf.rs` (HuggingFace downloads)

**Analog:** `crates/hephaestus-core/tests/classifier_e2e.rs` (lines 22-59)

**hf-hub download pattern:**
```rust
use hf_hub::HFClient;

async fn download_test_model() -> PathBuf {
    let client = HFClient::new().expect("failed to create HFClient");
    let model = client.model("Xenova", "distilbert-base-uncased-finetuned-sst-2-english");

    let model_path = model
        .download_file()
        .filename("onnx/model.onnx")
        .send()
        .await
        .expect("failed to download onnx/model.onnx");

    let _tokenizer_path = model
        .download_file()
        .filename("tokenizer.json")
        .send()
        .await
        .expect("failed to download tokenizer.json");

    let _config_path = model
        .download_file()
        .filename("config.json")
        .send()
        .await
        .expect("failed to download config.json");

    // Navigate to snapshot root from onnx/model.onnx path
    model_path
        .parent().expect("model.onnx should have parent (onnx/)")
        .parent().expect("onnx/ should have parent (snapshot root)")
        .to_path_buf()
}
```

**Apply:** Same `HFClient::new()` + `client.model(owner, name)` + `download_file().filename().send().await` pattern. Add `HFError::EntryNotFound` match for ONNX detection (D-04). Split model ID on first `/` for owner/name.

---

### `crates/hephaestus/src/config.rs` (modify -- add fields)

**Analog:** Self -- extend existing pattern (lines 24-63)

**Optional config field pattern:**
```rust
/// Custom warmup inference text (optional).
#[serde(default)]
pub warmup_input: Option<String>,

/// OpenTelemetry OTLP exporter endpoint (optional, env `OTEL_EXPORTER_OTLP_ENDPOINT`).
#[serde(default)]
pub otel_exporter_otlp_endpoint: Option<String>,
```

**Apply:** Add three fields following exact same `#[serde(default)]` + `Option<String>` pattern:
- `pub s3_bucket: Option<String>` (env `S3_BUCKET`)
- `pub s3_prefix: Option<String>` (env `S3_PREFIX`)
- `pub forge_url: Option<String>` (env `FORGE_URL`)

Also update `config_with_model_path()` test helper (line 141-153) to include the new fields with `None` defaults.

---

### `crates/hephaestus/src/main.rs` (modify -- wire resolver)

**Analog:** Self -- replace line 43 (lines 42-47)

**Current startup pattern to replace:**
```rust
// 3. Resolve and validate model directory (T-01-01).
let model_dir = config.model_dir()?;

// 4. Construct the classifier pipeline.
let pipeline = ClassifierPipeline::new(&model_dir)
    .context("failed to construct classifier pipeline")?;
```

**Replace with:** `ModelResolver::new()` + `resolver.resolve(&config.model_id).await?` producing a `PathBuf`, then feed to `ClassifierPipeline::new()`. Add `use hephaestus_resolve::ModelResolver;`.

## Shared Patterns

### Error Handling (thiserror for library crates)
**Source:** `crates/hephaestus-core/src/error.rs` lines 1-35
**Apply to:** `crates/hephaestus-resolve/src/error.rs`
- `#[derive(Error, Debug)]` on enum
- `#[error("message: {0}")]` for string-wrapped variants
- `#[from]` for `std::io::Error` conversion
- Doc comment on each variant

### Deep Module (Ousterhout pattern)
**Source:** `crates/hephaestus-core/src/pipeline.rs` lines 49-70 (trait) and 83-163 (impl)
**Apply to:** `ModelResolver` -- single `resolve()` hides 3-tier chain, retry, caching

### Config via envy (optional fields)
**Source:** `crates/hephaestus/src/config.rs` lines 24-63
**Apply to:** Same file -- add `s3_bucket`, `s3_prefix`, `forge_url` with `#[serde(default)]`

### mockall for trait testing
**Source:** `crates/hephaestus-core/src/pipeline.rs` lines 44-48
**Apply to:** Internal S3/HF traits in resolve crate, public `ForgeClient` trait

### Module structure (lib.rs re-exports)
**Source:** `crates/hephaestus-core/src/lib.rs` lines 1-14
**Apply to:** `crates/hephaestus-resolve/src/lib.rs` -- same pattern of `pub mod` + `pub(crate) mod` + `pub use` re-exports

## No Analog Found

| File | Role | Data Flow | Reason |
|------|------|-----------|--------|
| `crates/hephaestus-resolve/src/s3.rs` | service | file-I/O | No S3 operations exist in the codebase yet. Use RESEARCH.md aws-sdk-s3 patterns (get_object, put_object, ByteStream). |
| `crates/hephaestus-resolve/src/forge.rs` | service | request-response | No HTTP client code exists yet. Use RESEARCH.md reqwest pattern. Trait structure follows mockall pattern from pipeline.rs. |

## Metadata

**Analog search scope:** `crates/` directory (all 5 crates)
**Files scanned:** 21 Rust source files
**Pattern extraction date:** 2026-08-24
