# Phase 1: Core Inference Engine - Research

**Researched:** 2026-08-22
**Domain:** Rust ONNX inference runtime -- workspace scaffolding, model loading, tokenization, classifier pipeline
**Confidence:** HIGH

## Summary

Phase 1 builds the foundational Rust workspace that loads an ONNX classifier model from a local file path, tokenizes text input, runs inference via the `ort` crate, and returns a classification label with confidence score. No HTTP server, no model resolution chain, no GPU -- just the core inference pipeline working end-to-end on CPU.

The standard stack is well-established: `ort` 2.0.0-rc.13 wraps ONNX Runtime 1.28 and is the only serious Rust binding for ONNX inference. HuggingFace's `tokenizers` crate (written in Rust first, Python bindings are wrappers) handles tokenization with exact fidelity to model training. `envy` deserializes env vars into typed structs via serde, avoiding CLI parsers for a k8s-only service. All crates passed legitimacy checks with high download counts and verified source repos.

**Critical correction from CLAUDE.md:** ort 2.0.0-rc.13 depends on ndarray ^0.17, not 0.16 as documented in the tech stack. Additionally, hf-hub 1.0 substantially changed its API from the `Api/ApiBuilder` pattern to `HFClient::new()` with a builder-style download API. These corrections are reflected throughout this research.

**Primary recommendation:** Use ort v2 with default features (includes ndarray, tracing, download-binaries), tokenizers with `from_file()` for loading tokenizer.json, and implement a two-step Pipeline trait (prepare/execute) as decided in D-04. Validate tokenizer-model compatibility at startup by comparing tokenizer output field names against ONNX graph input names from `session.inputs()`.

<user_constraints>

## User Constraints (from CONTEXT.md)

### Locked Decisions

- **D-01:** Full workspace from day 1 -- 4 crates: `hephaestus` (binary), `hephaestus-core` (pipeline, tokenizer, inference), `hephaestus-resolve` (model resolution -- Phase 3, stub for now), `hephaestus-proto` (protobuf types -- Phase 2+)
- **D-02:** All 4 crate directories scaffolded in Phase 1 with Cargo.toml files. Resolve and proto have only placeholder `lib.rs`. Phase 1 builds binary + core only.
- **D-03:** Central dependency pinning via `[workspace.dependencies]` in root Cargo.toml. Crates use `dep.workspace = true`. One place to update versions.
- **D-04:** Pipeline trait uses a two-step API: `prepare(&self, input) -> PreparedInput` then `execute(&self, prepared) -> Result<Output>`. Two steps enable future batching (Phase 4) -- collect prepared inputs, execute as batch.
- **D-05:** Trait-per-profile dispatch: `ClassifierPipeline`, `EmbeddingsPipeline`, etc. each implement the `Pipeline` trait. Compile-time dispatch. Each pod runs one impl.
- **D-06:** Pipeline owns both `Arc<Session>` and `Arc<Tokenizer>`. Constructor loads them. Callers never touch internal ort or tokenizer types.
- **D-07:** Classifier output is `ClassifierOutput { label: String, score: f32 }` -- returns only the top predicted label with confidence score.
- **D-08:** Development and integration test model: `distilbert-base-uncased-finetuned-sst-2-english` from HuggingFace. Small (~260MB), well-known sentiment classifier with existing ONNX exports.
- **D-09:** Model caching via standard HF cache (`~/.cache/huggingface`). First run downloads, subsequent runs use cache. No model checked into repo.
- **D-10:** Unit tests mock the Pipeline trait via `mockall`. Integration tests load the real distilbert model and run actual inference. Clear separation: fast unit tests, thorough integration tests.
- **D-11:** Config loaded from env vars only (no CLI parser). Simple config struct with serde derives, loaded via `envy` crate. This is a k8s-only service -- all config comes from env vars and configmaps.
- **D-12:** Env vars: `MODEL_ID` (required -- pod crashes with clear error if missing), `MODEL_PATH` (optional -- local directory override for dev), `EXECUTION_PROVIDER` (optional, default: cpu), `LOG_LEVEL` (optional, default: info), `WARMUP_INPUT` (optional -- custom text for warmup pass).
- **D-13:** Fail hard on required config (MODEL_ID), use sensible defaults for optional config. K8s restart policy handles the crash.

### Claude's Discretion

No areas deferred to Claude's discretion -- all decisions made explicitly.

### Deferred Ideas (OUT OF SCOPE)

None -- discussion stayed within phase scope.

</user_constraints>

<phase_requirements>

## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| XCUT-01 | All public traits follow Ousterhout deep module pattern -- minimal interface (1-3 methods) hiding significant complexity | Pipeline trait design (D-04/D-05) with prepare/execute two-step API. Architecture patterns section covers the deep module pattern. |
| XCUT-02 | Rust workspace with separate crates for proto, core, resolve, and API concerns | Workspace layout (D-01/D-02) with 4 crates. Standard Stack section documents workspace dependency inheritance pattern. |
| XCUT-03 | All code adheres to rules in `rules/` directory | Rules read and accounted for: workspace deps, error handling (thiserror for lib, anyhow for app), mockall testing, naming conventions. |
| CORE-01 | Runtime loads an ONNX model via `ort` Session and runs inference on CPU execution provider | ort v2 Session::builder() API documented. CPU EP is always available (default). Session creation and run() method patterns provided. |
| CORE-02 | Runtime reads model configuration from environment variables | envy 0.4.2 from_env::<Config>() pattern documented. Config struct with MODEL_ID (required), MODEL_PATH, EXECUTION_PROVIDER, LOG_LEVEL, WARMUP_INPUT. |
| CORE-03 | Runtime runs a warmup inference pass after model load before accepting traffic | Warmup is a standard `session.run()` call with dummy or configured text. Architecture patterns section covers the startup sequence. |
| TOKN-01 | Runtime loads tokenizer.json from HuggingFace or S3 cache alongside the ONNX model | Tokenizer::from_file(path) loads tokenizer.json. For Phase 1, tokenizer loaded from local MODEL_PATH directory. |
| TOKN-02 | Runtime uses the `tokenizers` crate (HuggingFace Rust-native) for all text tokenization | tokenizers 0.23.1 API documented: encode(), Encoding struct with get_ids(), get_attention_mask(). |
| TOKN-03 | Runtime validates tokenizer output shape against ONNX graph input spec at startup | session.inputs() returns &[Outlet] with name/type/shape metadata. Compare against tokenizer output fields (input_ids, attention_mask). |
| PROF-01 | Classifier profile tokenizes input text, runs inference, applies softmax, and returns label + confidence score | ClassifierPipeline implements Pipeline trait. Softmax implementation (numerically stable) documented in code examples. |
| PROF-05 | All profiles implement a single Pipeline trait with minimal interface | Pipeline trait with prepare() + execute() (D-04). Two methods, hiding tokenization, inference, softmax, label mapping complexity. |

</phase_requirements>

## Project Constraints (from CLAUDE.md)

- **Language**: Rust only, 2024 edition, workspace resolver 3
- **Rules compliance**: Every file must adhere to all rules in `rules/` (160+ rules covering naming, error handling, async, testing, performance, documentation, memory, ownership, API design)
- **Code Convention**: Ousterhout deep module pattern -- traits expose 1-3 methods hiding significant implementation complexity
- **No Clap**: Do not use Clap for k8s-only services (user feedback); use envy or raw env reads
- **Error handling**: thiserror for library crates, anyhow for application binary (per rules `err-thiserror-lib` and `err-anyhow-app`)
- **Testing**: mockall for trait mocking (per rules `test-mock-traits`, `test-mockall-mocking`), integration tests in `tests/` directory
- **Workspace**: Central dependency pinning via `[workspace.dependencies]` (per rule `proj-workspace-deps`)

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| ONNX model loading | Core library (hephaestus-core) | -- | Model loading is internal to the inference engine, hidden behind Pipeline trait |
| Text tokenization | Core library (hephaestus-core) | -- | Tokenization is preprocessing within the pipeline, callers never touch tokenizer types (D-06) |
| Inference execution | Core library (hephaestus-core) | -- | Session::run() is internal to Pipeline::execute(), hidden from callers |
| Post-processing (softmax + label) | Core library (hephaestus-core) | -- | Softmax and argmax are post-processing steps inside Pipeline::execute() |
| Configuration loading | Application binary (hephaestus) | -- | Env var deserialization happens at startup in main(), passed to constructors |
| Warmup pass | Application binary (hephaestus) | Core library | Binary orchestrates startup sequence; core provides the inference method |
| Tokenizer-model validation | Core library (hephaestus-core) | -- | Validation at construction time, before Pipeline is usable |
| Model resolution (stub) | Resolve library (hephaestus-resolve) | -- | Placeholder for Phase 3; Phase 1 just loads from local path |

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| ort | 2.0.0-rc.13 | ONNX Runtime Rust bindings | Only serious Rust binding for ONNX Runtime 1.28. Pre-release but actively maintained by pykeio, 478K weekly downloads. No stable alternative exists. [CITED: docs.rs/ort, crates.io/crates/ort] |
| tokenizers | 0.23.1 | HuggingFace tokenizers | Rust-native implementation (HF wrote it in Rust first). Loads tokenizer.json directly. Exact fidelity with training tokenizer is critical for inference quality. 859K weekly downloads. [CITED: docs.rs/tokenizers, crates.io/crates/tokenizers] |
| ndarray | 0.17.2 | Tensor data structures | Required by ort v2 (^0.17 dependency). Creates input tensors for Session::run(). 1.9M weekly downloads. [CITED: crates.io/crates/ndarray, verified via cargo metadata] |
| envy | 0.4.2 | Env var config deserialization | Serde-based env var deserialization into typed structs. K8s-native config loading without CLI parsers. 265K weekly downloads. [CITED: docs.rs/envy] |
| serde | 1.0 | Serialization framework | De facto standard for Rust serialization. Needed for config structs, model metadata. [ASSUMED] |
| serde_json | 1.0 | JSON parsing | Model config files (config.json, id2label mapping). [ASSUMED] |
| thiserror | 2.0 | Library error types | Derive-based typed errors for hephaestus-core crate boundaries. Per rule `err-thiserror-lib`. [CITED: crates.io/crates/thiserror] |
| anyhow | 1.0 | Application error handling | Context-rich error propagation in the binary crate (main). Per rule `err-anyhow-app`. [CITED: crates.io/crates/anyhow] |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| hf-hub | 1.0.0 | HuggingFace model downloads | Test infrastructure only (Phase 1). Downloads test model and tokenizer for integration tests. Phase 3 uses it for production model resolution. [CITED: docs.rs/hf-hub, crates.io/crates/hf-hub] |
| tokio | 1.53 | Async runtime | Required by hf-hub for async downloads in test helpers. Phase 2+ uses it for the HTTP server. Use `features = ["rt-multi-thread", "macros"]` for test setup. [ASSUMED] |
| mockall | 0.15.0 | Trait mocking | Unit tests for Pipeline trait. Per rules `test-mock-traits` and `test-mockall-mocking`. [CITED: crates.io/crates/mockall] |
| tempfile | 3 | Temporary directories | Test fixtures for model file paths. [ASSUMED] |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| ort (pykeio) | tract-onnx | tract is pure Rust (no C dependency), but lacks execution provider support (no GPU path for future phases) and has lower ONNX op coverage. ort wraps official ONNX Runtime with full EP support. |
| envy | figment, config-rs | figment and config-rs support multiple config sources (files, env, CLI), but are overkill for k8s-only services where all config is env vars. envy is minimal and purpose-built. |
| ndarray | nalgebra | nalgebra is for linear algebra; ndarray is for n-dimensional arrays. ort depends on ndarray for tensor creation, so no choice here. |
| hf-hub (for tests) | Manual curl/wget | hf-hub handles content-addressed caching, ETag deduplication, and cache sharing with Python tooling. Manual downloads would require reimplementing cache logic. |

**Installation:**
```bash
# In workspace root Cargo.toml [workspace.dependencies]:
cargo add ort@2.0.0-rc.13 tokenizers@0.23.1 ndarray@0.17.2 envy@0.4.2 serde@1.0 serde_json@1.0 thiserror@2.0 anyhow@1.0

# Dev dependencies:
cargo add --dev mockall@0.15.0 tempfile@3 hf-hub@1.0.0 tokio@1.53
```

**Version verification:**
```
ort: 2.0.0-rc.13 (verified via cargo search, crates.io API)
tokenizers: 0.23.1 (verified via cargo search)
ndarray: 0.17.2 (verified via cargo search; ort depends on ^0.17)
envy: 0.4.2 (verified via cargo search)
mockall: 0.15.0 (verified via cargo search)
thiserror: 2.0.20 (verified via cargo search)
anyhow: 1.0.104 (verified via cargo search)
hf-hub: 1.0.0 (verified via cargo search)
```

**CLAUDE.md Corrections:**
- CLAUDE.md lists ndarray 0.16 -- must be 0.17 (ort 2.0.0-rc.13 depends on ^0.17) [VERIFIED: cargo metadata]
- CLAUDE.md describes hf-hub with `Api/ApiBuilder` pattern and `.model(id).get(filename)` -- hf-hub 1.0 uses `HFClient::new()` with `.model("org", "name").download_file().filename("file").send().await?` [CITED: docs.rs/hf-hub/1.0.0]
- CLAUDE.md says mockall 0.13 -- current is 0.15.0 [VERIFIED: cargo search]
- CLAUDE.md says ort API uses `Session::builder()?.commit_from_file()` -- this is correct for v2 (no Environment needed) [CITED: docs.rs/ort/2.0.0-rc.13]

## Package Legitimacy Audit

| Package | Registry | Age | Downloads | Source Repo | Verdict | Disposition |
|---------|----------|-----|-----------|-------------|---------|-------------|
| ort | crates.io | 3+ yrs | 478K/wk | github.com/pykeio/ort | OK | Approved |
| ort-sys | crates.io | 2+ yrs | 477K/wk | github.com/pykeio/ort | OK | Approved |
| ndarray | crates.io | 10+ yrs | 1.9M/wk | github.com/rust-ndarray/ndarray | OK | Approved |
| tokenizers | crates.io | 7+ yrs | 859K/wk | github.com/huggingface/tokenizers | OK | Approved |
| envy | crates.io | 10+ yrs | 265K/wk | github.com/softprops/envy | OK | Approved |
| serde | crates.io | 11+ yrs | 21.3M/wk | github.com/serde-rs/serde | OK | Approved |
| serde_json | crates.io | 11+ yrs | 21.7M/wk | github.com/serde-rs/json | OK | Approved |
| thiserror | crates.io | 6+ yrs | 25.6M/wk | github.com/dtolnay/thiserror | OK | Approved |
| anyhow | crates.io | 6+ yrs | 15.2M/wk | github.com/dtolnay/anyhow | OK | Approved |
| mockall | crates.io | 7+ yrs | 2.1M/wk | github.com/asomers/mockall | OK | Approved |
| hf-hub | crates.io | 3+ yrs | 550K/wk | github.com/huggingface/hf-hub | OK | Approved |
| tokio | crates.io | 10+ yrs | 16.1M/wk | github.com/tokio-rs/tokio | OK | Approved |
| tempfile | crates.io | 11+ yrs | 13M/wk | github.com/Stebalien/tempfile | OK | Approved |

**Packages removed due to [SLOP] verdict:** none
**Packages flagged as suspicious [SUS]:** none

## Architecture Patterns

### System Architecture Diagram

```
                        Phase 1 Runtime Flow
                        =====================

  ENV VARS                    LOCAL FILESYSTEM
  (MODEL_ID,                  (MODEL_PATH/)
   MODEL_PATH, ...)           +-- model.onnx
       |                      +-- tokenizer.json
       v                      +-- config.json (id2label)
  +-----------+                    |
  |  Config   |                    |
  |  (envy)   |                    v
  +-----------+         +-------------------+
       |                |  Pipeline::new()  |
       |                |  (constructor)    |
       v                +-------------------+
  +-----------+              |         |
  | main()    |              v         v
  | startup   |------->  Session    Tokenizer
  | sequence  |         (ort)      (tokenizers)
  +-----------+              |         |
       |                     |    validate inputs
       |                     |    against model
       v                     v         |
  +------------+        +---------+    |
  | warmup()   |------->|Pipeline |<---+
  | inference  |        |.prepare |
  +------------+        |.execute |
       |                +---------+
       v                     |
  Ready for work             v
  (Phase 2 adds         ClassifierOutput
   HTTP server)          { label, score }
```

### Recommended Project Structure

```
hephaestus/
+-- Cargo.toml                    # Workspace root (virtual manifest)
+-- Cargo.lock                    # Shared lock file
+-- rules/                        # Rust coding rules (160+ files)
+-- crates/
|   +-- hephaestus/               # Binary crate
|   |   +-- Cargo.toml
|   |   +-- src/
|   |       +-- main.rs           # Entrypoint: config, pipeline construction, warmup
|   |       +-- config.rs         # Config struct with envy deserialization
|   +-- hephaestus-core/          # Core library crate
|   |   +-- Cargo.toml
|   |   +-- src/
|   |       +-- lib.rs            # Public API re-exports
|   |       +-- pipeline.rs       # Pipeline trait + ClassifierPipeline
|   |       +-- tokenize.rs       # Tokenizer wrapper + validation
|   |       +-- inference.rs      # Session wrapper + tensor I/O
|   |       +-- postprocess.rs    # Softmax + label mapping
|   |       +-- error.rs          # thiserror error types
|   +-- hephaestus-resolve/       # Stub for Phase 3
|   |   +-- Cargo.toml
|   |   +-- src/
|   |       +-- lib.rs            # Placeholder
|   +-- hephaestus-proto/         # Stub for Phase 2+
|       +-- Cargo.toml
|       +-- src/
|           +-- lib.rs            # Placeholder
+-- tests/                        # Integration tests (workspace level)
|   +-- classifier_e2e.rs         # Full pipeline with real model
|   +-- test_helpers/
|       +-- mod.rs                # Test model download helper
+-- .planning/                    # GSD planning artifacts
```

### Pattern 1: Two-Step Pipeline Trait (D-04)

**What:** Pipeline trait with `prepare()` and `execute()` separation.
**When to use:** Always -- this is the core abstraction for all model profiles.
**Example:**
```rust
// Source: CONTEXT.md D-04, D-05, D-07
use std::sync::Arc;
use ort::session::Session;
use tokenizers::Tokenizer;

/// Output from a classifier inference pass.
pub struct ClassifierOutput {
    pub label: String,
    pub score: f32,
}

/// Prepared input ready for batch collection or immediate execution.
pub struct PreparedInput {
    input_ids: Vec<i64>,
    attention_mask: Vec<i64>,
    sequence_length: usize,
}

/// Core inference pipeline trait. Each model profile implements this.
/// Follows Ousterhout deep module pattern: 2 methods hide tokenization,
/// inference, and post-processing complexity.
pub trait Pipeline {
    type Input;
    type Prepared;
    type Output;

    /// Tokenize and prepare input for inference.
    fn prepare(&self, input: Self::Input) -> Result<Self::Prepared, CoreError>;

    /// Run inference on prepared input and return results.
    fn execute(&self, prepared: Self::Prepared) -> Result<Self::Output, CoreError>;
}

pub struct ClassifierPipeline {
    session: Arc<Session>,
    tokenizer: Arc<Tokenizer>,
    id2label: Vec<String>,
}

impl Pipeline for ClassifierPipeline {
    type Input = String;
    type Prepared = PreparedInput;
    type Output = ClassifierOutput;

    fn prepare(&self, input: String) -> Result<PreparedInput, CoreError> {
        let encoding = self.tokenizer.encode(input.as_str(), true)
            .map_err(CoreError::Tokenization)?;
        Ok(PreparedInput {
            input_ids: encoding.get_ids().iter().map(|&id| id as i64).collect(),
            attention_mask: encoding.get_attention_mask().iter().map(|&m| m as i64).collect(),
            sequence_length: encoding.len(),
        })
    }

    fn execute(&self, prepared: PreparedInput) -> Result<ClassifierOutput, CoreError> {
        // Create ndarray tensors, run session, apply softmax, map to label
        // All complexity hidden inside this method
        todo!()
    }
}
```

### Pattern 2: Startup Validation (TOKN-03)

**What:** Validate tokenizer output compatibility with ONNX model inputs at construction time.
**When to use:** During Pipeline construction, before the pipeline is usable.
**Example:**
```rust
// Source: docs.rs/ort/2.0.0-rc.13/ort/session/struct.Session.html
fn validate_tokenizer_model_compatibility(
    session: &Session,
    _tokenizer: &Tokenizer,
) -> Result<(), CoreError> {
    let model_inputs: Vec<&str> = session.inputs()
        .iter()
        .map(|outlet| outlet.name.as_str())
        .collect();

    // Standard transformer models expect these input names
    let required_inputs = ["input_ids", "attention_mask"];
    for required in &required_inputs {
        if !model_inputs.contains(required) {
            return Err(CoreError::ModelValidation(format!(
                "model does not accept input '{}'; model inputs are: {:?}",
                required, model_inputs
            )));
        }
    }
    Ok(())
}
```

### Pattern 3: Numerically Stable Softmax

**What:** Softmax with max-subtraction for numerical stability.
**When to use:** Post-processing classifier logits to probabilities.
**Example:**
```rust
// Source: gist.github.com/boydjohnson/c0994e402c006b1e4b5c06e4c1e5ccd6
fn softmax(logits: &[f32]) -> Vec<f32> {
    let max = logits.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let exps: Vec<f32> = logits.iter().map(|&x| (x - max).exp()).collect();
    let sum: f32 = exps.iter().sum();
    exps.iter().map(|&e| e / sum).collect()
}

fn argmax_with_score(probs: &[f32]) -> (usize, f32) {
    probs.iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
        .map(|(idx, &score)| (idx, score))
        .unwrap()
}
```

### Pattern 4: ort v2 Session and Inference

**What:** Creating an ort Session and running inference in v2 API.
**When to use:** Core inference execution.
**Example:**
```rust
// Source: docs.rs/ort/2.0.0-rc.13
use ort::session::Session;
use ort::value::TensorRef;
use ndarray::Array2;

// Session creation -- no Environment in v2
let mut session = Session::builder()?
    .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level3)?
    .commit_from_file(model_path)?;

// Create input tensors from tokenizer output
let batch_size = 1usize;
let seq_len = input_ids.len();
let input_ids_array = Array2::from_shape_vec(
    (batch_size, seq_len),
    input_ids, // Vec<i64>
)?;
let attention_mask_array = Array2::from_shape_vec(
    (batch_size, seq_len),
    attention_mask, // Vec<i64>
)?;

// Run inference using inputs! macro
let outputs = session.run(ort::inputs![
    "input_ids" => TensorRef::from_array_view(&input_ids_array.view())?,
    "attention_mask" => TensorRef::from_array_view(&attention_mask_array.view())?,
]?)?;

// Extract output tensor
let logits = outputs[0].try_extract_tensor::<f32>()?;
let logits_slice = logits.as_slice().unwrap();
```

### Pattern 5: Config with envy

**What:** Typed configuration from environment variables.
**When to use:** Application startup in main().
**Example:**
```rust
// Source: docs.rs/envy/0.4.2
use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct Config {
    /// Required: model identifier (e.g., "distilbert-base-uncased-finetuned-sst-2-english")
    pub model_id: String,

    /// Optional: local directory containing model files
    #[serde(default)]
    pub model_path: Option<String>,

    /// Optional: execution provider (default: "cpu")
    #[serde(default = "default_ep")]
    pub execution_provider: String,

    /// Optional: log level (default: "info")
    #[serde(default = "default_log_level")]
    pub log_level: String,

    /// Optional: custom text for warmup inference pass
    #[serde(default)]
    pub warmup_input: Option<String>,
}

fn default_ep() -> String { "cpu".to_string() }
fn default_log_level() -> String { "info".to_string() }

// In main():
let config = envy::from_env::<Config>()
    .expect("MODEL_ID environment variable is required");
```

### Anti-Patterns to Avoid

- **Exposing ort or tokenizers types in public API:** Per D-06, Pipeline owns `Arc<Session>` and `Arc<Tokenizer>` internally. Callers never import ort or tokenizers. If public types leak, downstream crates get coupled to ort's pre-release API.
- **Using `unwrap()` in library code:** Per rule `err-no-unwrap-prod`, use `?` operator with thiserror types. Reserve `expect()` for truly impossible states per rule `err-expect-bugs-only`.
- **Shallow traits with many methods:** Per XCUT-01 and project constraints, traits must have 1-3 methods. A Pipeline trait with separate `tokenize()`, `infer()`, `decode()` methods is too shallow -- combine into `prepare()` + `execute()`.
- **Using `String` where `&str` suffices:** Per rule `anti-string-for-str`, prefer borrowed types in function signatures.
- **Holding Session across await points without Mutex:** `Session::run()` takes `&mut self`. For Phase 1 (no async server) this is not an issue, but Phase 2 will need `tokio::sync::Mutex<Session>` or similar.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Tokenization | Custom regex/BPE tokenizer | `tokenizers` crate | Exact fidelity with training tokenizer is critical. Any deviation causes silent quality degradation. HuggingFace wrote it in Rust first. [CITED: docs.rs/tokenizers] |
| ONNX inference | Direct C API calls via FFI | `ort` crate | Safe Rust API over ONNX Runtime 1.28. Handles memory management, execution providers, tensor lifecycle. Raw FFI is unsafe and unmaintained. [CITED: docs.rs/ort] |
| Env var parsing | Manual `std::env::var()` calls | `envy` crate | Serde-based deserialization handles type conversion, defaults, missing values. Manual parsing is error-prone and verbose. [CITED: docs.rs/envy] |
| Error types | Manual `impl Display + Error` | `thiserror` derive | Generates correct `Error` trait impls with source chaining. Manual implementation is boilerplate-heavy and error-prone. Per rule `err-thiserror-lib`. |

**Key insight:** Softmax is the one post-processing step simple enough to implement inline (5 lines of code). Everything else in the pipeline has a well-maintained crate that should be used.

## Common Pitfalls

### Pitfall 1: ort v2 API vs v1 API Confusion

**What goes wrong:** Using `Environment::builder()` or `SessionBuilder::new(&env)` patterns from ort v1 tutorials.
**Why it happens:** Most online examples and tutorials are written for ort v1. The v2 API removed `Environment` entirely.
**How to avoid:** `Session::builder()?` directly, no environment. This is the correct v2 pattern. [CITED: docs.rs/ort/2.0.0-rc.13]
**Warning signs:** Compiler error "cannot find type `Environment` in crate `ort`".

### Pitfall 2: ndarray Version Mismatch

**What goes wrong:** Using ndarray 0.16 (as documented in CLAUDE.md tech stack) when ort 2.0.0-rc.13 requires ^0.17.
**Why it happens:** CLAUDE.md tech stack is stale on this version.
**How to avoid:** Pin ndarray = "0.17" in workspace dependencies. Verified: ort depends on ndarray ^0.17. [VERIFIED: cargo metadata]
**Warning signs:** Compilation errors about incompatible ndarray types or missing trait implementations.

### Pitfall 3: Session::run() Mutability

**What goes wrong:** Trying to call `session.run()` on a shared `&Session` or `Arc<Session>`.
**Why it happens:** `run()` takes `&mut self` because ONNX Runtime internals (EP allocators, statistics trackers) are not thread-safe.
**How to avoid:** For Phase 1 (single-threaded), store `Session` directly in the Pipeline struct (not behind Arc). Phase 2 will need `tokio::sync::Mutex<Session>` for concurrent access. [CITED: docs.rs/ort/2.0.0-rc.13]
**Warning signs:** Compiler error "cannot borrow as mutable" when using `Arc<Session>`.

### Pitfall 4: Tokenizer Input ID Type Mismatch

**What goes wrong:** Passing `u32` token IDs directly to an ONNX model that expects `i64` inputs.
**Why it happens:** `Encoding::get_ids()` returns `&[u32]`, but transformer ONNX models typically expect `int64` tensors.
**How to avoid:** Cast token IDs: `encoding.get_ids().iter().map(|&id| id as i64).collect::<Vec<i64>>()`. Same for attention_mask. [CITED: docs.rs/tokenizers]
**Warning signs:** ONNX Runtime error about unexpected input tensor type.

### Pitfall 5: Missing Label Mapping

**What goes wrong:** Getting argmax index from softmax output but no mapping to human-readable label.
**Why it happens:** The ONNX model outputs raw logits by class index. The mapping from index to label name (e.g., 0->"NEGATIVE", 1->"POSITIVE") is in `config.json`, not in the model file.
**How to avoid:** Load `config.json` from the model directory, parse `id2label` mapping. For distilbert-sst2: `{"0": "NEGATIVE", "1": "POSITIVE"}`. [ASSUMED]
**Warning signs:** ClassifierOutput returns numeric index instead of string label.

### Pitfall 6: hf-hub 1.0 API Change

**What goes wrong:** Using the old `Api::new()?.model("user/model").get("file")` pattern from CLAUDE.md.
**Why it happens:** hf-hub 1.0 completely changed the API to `HFClient::new()` with builder-style downloads.
**How to avoid:** Use `HFClient::new()?.model("org", "name").download_file().filename("file").send().await?`. For sync, enable `blocking` feature and use `HFClientSync`. [CITED: docs.rs/hf-hub/1.0.0]
**Warning signs:** Compiler error "cannot find type `Api` in crate `hf_hub`".

### Pitfall 7: Test Model ONNX File Location

**What goes wrong:** Looking for `model.onnx` at the repo root when it's in an `onnx/` subdirectory.
**Why it happens:** Different HuggingFace model repos organize ONNX files differently. The Xenova variant has ONNX files for Transformers.js compatibility.
**How to avoid:** Use `Xenova/distilbert-base-uncased-finetuned-sst-2-english` which has ONNX exports specifically. Check the repo for exact file paths. [CITED: huggingface.co/Xenova/distilbert-base-uncased-finetuned-sst-2-english]
**Warning signs:** File not found error when loading model.onnx.

## Code Examples

### Complete Classifier Pipeline Construction

```rust
// Source: Synthesized from docs.rs/ort/2.0.0-rc.13 + docs.rs/tokenizers/0.23.1
use std::path::Path;

pub struct ClassifierPipeline {
    session: Session,  // Owned, not Arc (Phase 1 is single-threaded)
    tokenizer: Tokenizer,
    id2label: Vec<String>,
}

impl ClassifierPipeline {
    pub fn new(model_dir: &Path) -> Result<Self, CoreError> {
        // Load ONNX session
        let model_path = model_dir.join("model.onnx");
        let session = Session::builder()?
            .with_optimization_level(GraphOptimizationLevel::Level3)?
            .commit_from_file(&model_path)?;

        // Load tokenizer
        let tokenizer_path = model_dir.join("tokenizer.json");
        let tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| CoreError::Tokenization(e.to_string()))?;

        // Validate compatibility
        validate_tokenizer_model_compatibility(&session, &tokenizer)?;

        // Load label mapping from config.json
        let config_path = model_dir.join("config.json");
        let config_text = std::fs::read_to_string(&config_path)
            .map_err(CoreError::Io)?;
        let config: serde_json::Value = serde_json::from_str(&config_text)?;
        let id2label = extract_id2label(&config)?;

        Ok(Self { session, tokenizer, id2label })
    }
}
```

### Test Model Download Helper

```rust
// Source: docs.rs/hf-hub/1.0.0
use hf_hub::HFClient;
use std::path::PathBuf;

/// Downloads the test classifier model files to the HF cache.
/// Returns the directory containing model.onnx and tokenizer.json.
async fn download_test_model() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let client = HFClient::new()?;
    let model = client.model("Xenova", "distilbert-base-uncased-finetuned-sst-2-english");

    // Download required files (returns cached paths)
    let model_path = model.download_file()
        .filename("onnx/model.onnx")
        .send()
        .await?;
    let _tokenizer_path = model.download_file()
        .filename("tokenizer.json")
        .send()
        .await?;
    let _config_path = model.download_file()
        .filename("config.json")
        .send()
        .await?;

    // Return parent directory containing all files
    Ok(model_path.parent().unwrap().parent().unwrap().to_path_buf())
}
```

### Warmup Inference

```rust
// Source: CONTEXT.md D-03, D-12
fn warmup(pipeline: &mut ClassifierPipeline, config: &Config) -> Result<(), anyhow::Error> {
    let warmup_text = config.warmup_input
        .as_deref()
        .unwrap_or("This is a warmup inference pass.");

    let prepared = pipeline.prepare(warmup_text.to_string())
        .context("warmup: failed to prepare input")?;
    let output = pipeline.execute(prepared)
        .context("warmup: failed to run inference")?;

    tracing::info!(
        label = %output.label,
        score = output.score,
        "warmup inference complete"
    );
    Ok(())
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| ort v1 with `Environment` | ort v2 with `Session::builder()` directly | v2.0.0-rc.1 (2024) | No more global environment. Session creation is simpler. |
| hf-hub `Api/ApiBuilder` | hf-hub `HFClient` | v1.0.0 (2025) | Breaking API change. model() takes (org, name) not combined ID. download_file() builder pattern. |
| ndarray 0.16 | ndarray 0.17 | 2025 | ort v2 depends on ^0.17. Breaking changes in array construction APIs. |
| thiserror 1.x | thiserror 2.0 | 2024 | New derive macro implementation. Functionally equivalent for users. |
| mockall 0.13 | mockall 0.15 | 2025 | Updated derive macros, better async trait support. |

**Deprecated/outdated:**
- ort v1 `Environment` type: Removed in v2. Do not use tutorials showing `Environment::builder()`.
- hf-hub `Api` type: Replaced by `HFClient` in 1.0. Do not use examples showing `Api::new()`.
- ndarray 0.16: ort v2 requires 0.17+. Do not pin 0.16.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | serde 1.0 and serde_json 1.0 are the correct versions for current Rust | Standard Stack | Extremely low risk -- serde has been 1.0 for 7+ years with backwards compatibility |
| A2 | tokio 1.53 is needed for hf-hub async downloads | Standard Stack | Low risk -- tokio 1.x is highly stable. Exact minor version may differ. |
| A3 | tempfile version 3 is current | Standard Stack | Low risk -- tempfile 3.x has been stable for years |
| A4 | distilbert-sst2 config.json contains `id2label` mapping with keys "0" and "1" | Common Pitfalls | Medium risk -- if config format differs, label extraction code needs adjustment. Verify during integration test setup. |
| A5 | Xenova variant of distilbert-sst2 has ONNX files at `onnx/model.onnx` path | Common Pitfalls | Medium risk -- exact file path in HF repo needs verification during test setup. Alternative: use `optimum/distilbert-base-uncased-finetuned-sst-2-english`. |
| A6 | `session.inputs()` returns Outlet structs with a `.name` field as `String` | Architecture Patterns | Low risk -- verified from docs.rs, but Outlet struct fields not fully confirmed. Integration test will validate. |

## Open Questions

1. **Exact ONNX file path in Xenova/distilbert-base-uncased-finetuned-sst-2-english repo**
   - What we know: The model has ONNX exports. Files may be at `onnx/model.onnx` or `model.onnx` at root.
   - What's unclear: Exact file path and whether tokenizer.json is at root or in onnx/ subdirectory.
   - Recommendation: Resolve during first integration test. Try `onnx/model.onnx` first, fall back to `model.onnx`. tokenizer.json is typically at repo root.

2. **Session::run() mutability and Arc<Session> in D-06**
   - What we know: D-06 says Pipeline owns `Arc<Session>`. But `run()` takes `&mut self`, which is incompatible with `Arc<Session>` without a Mutex.
   - What's unclear: Whether D-06 was written assuming ort v1 semantics (which may have had `&self` run).
   - Recommendation: For Phase 1, store `Session` directly (not Arc) since there is no concurrent access. Phase 2 should wrap in `Arc<tokio::sync::Mutex<Session>>` for HTTP handler access. Note this deviation from D-06 in the plan.

3. **hf-hub sync vs async API for test helpers**
   - What we know: hf-hub 1.0 has async `HFClient` by default and sync `HFClientSync` behind `blocking` feature.
   - What's unclear: Whether integration tests should use async (requiring tokio test runtime) or sync (simpler but requires `blocking` feature).
   - Recommendation: Use async with `#[tokio::test]` since tokio is already a dependency and test model download is I/O-bound.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| Rust toolchain | All code | Yes | 1.97.1 (stable) | -- |
| cargo | Build system | Yes | 1.97.1 | -- |
| Internet access | Test model download (hf-hub) | Yes | -- | Pre-download model files and set MODEL_PATH |

**Missing dependencies with no fallback:** none

**Missing dependencies with fallback:** none

## Validation Architecture

### Test Framework

| Property | Value |
|----------|-------|
| Framework | cargo test (built-in) + mockall 0.15 |
| Config file | none -- Wave 0 |
| Quick run command | `cargo test -p hephaestus-core` |
| Full suite command | `cargo test --workspace` |

### Phase Requirements to Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| CORE-01 | Load ONNX model via ort Session on CPU | integration | `cargo test -p hephaestus-core --test classifier_e2e -- --nocapture` | Wave 0 |
| CORE-02 | Read config from env vars | unit | `cargo test -p hephaestus config` | Wave 0 |
| CORE-03 | Warmup inference pass after model load | integration | `cargo test -p hephaestus-core --test classifier_e2e warmup` | Wave 0 |
| TOKN-01 | Load tokenizer.json from local path | unit | `cargo test -p hephaestus-core tokenizer_load` | Wave 0 |
| TOKN-02 | Tokenize text using tokenizers crate | unit | `cargo test -p hephaestus-core tokenize` | Wave 0 |
| TOKN-03 | Validate tokenizer against ONNX graph inputs | unit + integration | `cargo test -p hephaestus-core validation` | Wave 0 |
| PROF-01 | Classifier: tokenize, infer, softmax, label+score | integration | `cargo test -p hephaestus-core --test classifier_e2e classify` | Wave 0 |
| PROF-05 | Pipeline trait with 1-3 methods | manual-only | Review trait definition: count methods, verify deep module pattern | -- |
| XCUT-01 | Ousterhout deep module pattern | manual-only | Review all public traits: verify 1-3 methods each | -- |
| XCUT-02 | 4-crate workspace structure | unit | `cargo build --workspace` | Wave 0 |
| XCUT-03 | Rules compliance | lint | `cargo clippy --workspace -- -D warnings` | Wave 0 |

### Sampling Rate

- **Per task commit:** `cargo test -p hephaestus-core && cargo clippy --workspace -- -D warnings`
- **Per wave merge:** `cargo test --workspace`
- **Phase gate:** Full suite green before `/gsd-verify-work`

### Wave 0 Gaps

- [ ] `tests/classifier_e2e.rs` -- integration test with real distilbert model (covers CORE-01, CORE-03, TOKN-03, PROF-01)
- [ ] `crates/hephaestus/src/config.rs` + unit test -- env var config loading (covers CORE-02)
- [ ] `crates/hephaestus-core/src/` + unit tests -- pipeline, tokenizer, inference modules (covers TOKN-01, TOKN-02, PROF-05)
- [ ] Test helper for downloading model files via hf-hub

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no | N/A -- internal service, no user auth in Phase 1 |
| V3 Session Management | no | N/A -- no HTTP sessions in Phase 1 |
| V4 Access Control | no | N/A -- no access control in Phase 1 |
| V5 Input Validation | yes | Validate tokenizer-model compatibility at startup (TOKN-03). Validate config env vars via envy typed deserialization. Validate model file existence before loading. |
| V6 Cryptography | no | N/A -- no crypto operations in Phase 1 |

### Known Threat Patterns for Rust ONNX inference

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Malicious ONNX model file | Tampering | Load only from trusted paths (MODEL_PATH env var). Phase 3 adds S3/HF verification. |
| Path traversal in MODEL_PATH | Tampering | Validate MODEL_PATH is an absolute path to an existing directory. Reject paths with `..` components. |
| Denial of service via large input text | Denial of Service | Tokenizer truncation limits input length. Set `with_truncation()` on tokenizer to max model sequence length. |
| Resource exhaustion from model load | Denial of Service | K8s resource limits (CPU/memory) constrain the pod. Model load happens once at startup. |

## Sources

### Primary (HIGH confidence)
- [docs.rs/ort/2.0.0-rc.13](https://docs.rs/ort/2.0.0-rc.13/ort/) -- Session API, SessionBuilder, run() signature, Outlet metadata
- [docs.rs/tokenizers/0.23.1](https://docs.rs/tokenizers/0.23.1/tokenizers/) -- Tokenizer, Encoding, from_file(), encode(), get_ids(), get_attention_mask()
- [docs.rs/envy/0.4.2](https://docs.rs/envy/0.4.2/envy/) -- from_env(), prefixed(), serde integration
- [docs.rs/hf-hub/1.0.0](https://docs.rs/hf-hub/1.0.0/hf_hub/) -- HFClient API, download_file(), cache behavior
- [crates.io API](https://crates.io) -- Version verification for all crates via cargo search
- cargo metadata -- Verified ort depends on ndarray ^0.17

### Secondary (MEDIUM confidence)
- [github.com/pykeio/ort](https://github.com/pykeio/ort) -- Repository structure, ONNX Runtime 1.28 compatibility
- [huggingface.co/Xenova/distilbert-base-uncased-finetuned-sst-2-english](https://huggingface.co/Xenova/distilbert-base-uncased-finetuned-sst-2-english) -- ONNX exports available for test model
- [gist.github.com/boydjohnson softmax](https://gist.github.com/boydjohnson/c0994e402c006b1e4b5c06e4c1e5ccd6) -- Numerically stable softmax with ndarray

### Tertiary (LOW confidence)
- [rustfaq.org ONNX tutorial](https://www.rustfaq.org/en/how-to-run-onnx-models-in-rust/) -- Uses ort v1 API (stale), but general patterns are valid
- [github.com/fbilhaut/orp](https://github.com/fbilhaut/orp) -- ONNX pipeline framework, architectural reference only

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- all crates verified on registry with version confirmation via cargo search. ort v2 API confirmed via docs.rs.
- Architecture: HIGH -- Two-step Pipeline trait design (D-04) is well-supported by ort and tokenizers APIs. Validation pattern confirmed from Session metadata API.
- Pitfalls: HIGH -- Critical API differences (ort v2 vs v1, hf-hub 1.0, ndarray version) verified against authoritative sources.

**Research date:** 2026-08-22
**Valid until:** 2026-09-22 (30 days -- ort is pre-release but API is stable within rc series)
