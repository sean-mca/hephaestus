# Phase 4: Additional Profiles and Dynamic Batching - Pattern Map

**Mapped:** 2026-08-26
**Files analyzed:** 9 (3 new, 6 modified)
**Analogs found:** 9 / 9

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|-------------------|------|-----------|----------------|---------------|
| `crates/hephaestus-core/src/pipeline.rs` (modify) | service | request-response | self (existing) | exact |
| `crates/hephaestus-core/src/postprocess.rs` (modify) | utility | transform | self (existing) | exact |
| `crates/hephaestus-core/src/profile.rs` (new) | utility | transform | `pipeline.rs` lines 249-276 (config.json parsing) | role-match |
| `crates/hephaestus-core/src/error.rs` (modify) | model | N/A | self (existing) | exact |
| `crates/hephaestus-core/src/lib.rs` (modify) | config | N/A | self (existing) | exact |
| `crates/hephaestus-api/src/state.rs` (modify) | provider | request-response | self (existing) | exact |
| `crates/hephaestus-api/src/handlers.rs` (modify) | controller | request-response | self (existing) | exact |
| `crates/hephaestus-api/src/batcher.rs` (new) | service | batch | no direct analog | none |
| `crates/hephaestus/src/config.rs` (modify) | config | N/A | self (existing) | exact |
| `crates/hephaestus/src/main.rs` (modify) | controller | request-response | self (existing) | exact |

## Pattern Assignments

### `crates/hephaestus-core/src/pipeline.rs` (modify -- add 3 pipelines + PipelineKind enum)

**Analog:** Self -- `ClassifierPipeline` is the exact pattern to replicate.

**Pipeline struct pattern** (lines 77-82):
```rust
pub struct ClassifierPipeline {
    session: Session,
    tokenizer: Tokenizer,
    id2label: Vec<String>,
}
```
New pipelines follow the same shape: `Session` + `Tokenizer` + profile-specific metadata. `EmbeddingsPipeline` needs no extra metadata. `Seq2SeqPipeline` needs no extra metadata (tokenizer handles decode). `TokenClassifierPipeline` needs `id2label: Vec<String>`.

**Constructor pattern** (lines 83-163):
```rust
pub fn new(model_dir: &Path) -> Result<Self, CoreError> {
    // 1. Resolve ONNX model file (onnx/ subdirectory fallback)
    // 2. Load ONNX session with GraphOptimizationLevel::Level3
    // 3. Load tokenizer from tokenizer.json
    // 4. Configure truncation to 512
    // 5. Validate model inputs contain "input_ids" and "attention_mask"
    // 6. Load profile-specific metadata from config.json
    Ok(Self { session, tokenizer, ... })
}
```
Steps 1-5 are identical across all profiles. Only step 6 differs. Extract a shared helper or duplicate (implementer's choice per Ousterhout -- a helper is fine since these are in the same module).

**prepare() pattern** (lines 170-191):
```rust
fn prepare(&self, input: String) -> Result<PreparedInput, CoreError> {
    let encoding = self.tokenizer
        .encode(input.as_str(), true)
        .map_err(|e| CoreError::Tokenization(e.to_string()))?;
    let input_ids: Vec<i64> = encoding.get_ids().iter().map(|&id| i64::from(id)).collect();
    let attention_mask: Vec<i64> = encoding.get_attention_mask().iter().map(|&m| i64::from(m)).collect();
    let sequence_length = encoding.len();
    Ok(PreparedInput { input_ids, attention_mask, sequence_length })
}
```
All profiles use this exact `prepare()` -- tokenization is identical. Can share via a helper function.

**execute() pattern** (lines 193-242):
```rust
fn execute(&mut self, prepared: PreparedInput) -> Result<ClassifierOutput, CoreError> {
    let seq_len = prepared.sequence_length;
    // 1. Create ndarray tensors (1, seq_len)
    let input_ids_array = Array2::from_shape_vec((1, seq_len), prepared.input_ids)
        .map_err(|e| CoreError::Inference(e.to_string()))?;
    let attention_mask_array = Array2::from_shape_vec((1, seq_len), prepared.attention_mask)
        .map_err(|e| CoreError::Inference(e.to_string()))?;
    // 2. Create TensorRef, run session
    let outputs = self.session.run(ort::inputs![
        "input_ids" => input_ids_tensor,
        "attention_mask" => attention_mask_tensor,
    ]).map_err(|e| CoreError::Inference(e.to_string()))?;
    // 3. Extract output tensor
    let logits = outputs[0].try_extract_tensor::<f32>()
        .map_err(|e| CoreError::Inference(e.to_string()))?;
    // 4. Profile-specific post-processing
    // ... softmax + argmax for classifier
    // ... mean_pool + l2_normalize for embeddings
    // ... decode token IDs for seq2seq
    // ... argmax per token + BIO merge for token classifier
}
```
Steps 1-3 are shared across profiles. Only post-processing (step 4) differs.

**Test pattern** (lines 278-363):
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_something() {
        // Arrange
        // Act
        // Assert
    }
}
```

---

### `crates/hephaestus-core/src/postprocess.rs` (modify -- add mean_pool, l2_normalize, merge_bio_spans)

**Analog:** Self -- existing `softmax` and `argmax_with_score`.

**Function signature pattern** (lines 15-23, 32-43):
```rust
/// Doc comment explaining purpose.
///
/// # Panics
///
/// Panics if `input` is empty.
pub(crate) fn softmax(logits: &[f32]) -> Vec<f32> {
    // implementation
}

pub(crate) fn argmax_with_score(probs: &[f32]) -> (usize, f32) {
    // implementation
}
```
New functions follow the same visibility (`pub(crate)`), take slice inputs, return owned values. Doc comments include panics section.

**Test pattern** (lines 46-136):
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_softmax_basic() {
        // Arrange
        let logits = [1.0_f32, 2.0, 3.0];
        // Act
        let probs = softmax(&logits);
        // Assert
        let sum: f32 = probs.iter().sum();
        assert!((sum - 1.0).abs() < 1e-6, "softmax must sum to 1.0, got {sum}");
    }
}
```

---

### `crates/hephaestus-core/src/profile.rs` (new -- profile detection)

**Analog:** `pipeline.rs` lines 249-276 (`extract_id2label`) -- same pattern of parsing config.json fields.

**Config.json parsing pattern** (pipeline.rs lines 249-276):
```rust
fn extract_id2label(config: &serde_json::Value) -> Result<Vec<String>, CoreError> {
    let id2label_obj = config
        .get("id2label")
        .and_then(|v| v.as_object())
        .ok_or_else(|| {
            CoreError::ModelValidation("config.json missing 'id2label' object".to_string())
        })?;
    // parse and return
}
```
`detect_profile` follows same pattern: take `&serde_json::Value`, use `.get()` + `.and_then()`, return `Result<ModelProfile, CoreError>`.

**Error pattern** (error.rs lines 25-26):
```rust
#[error("config error: {0}")]
Config(String),
```
Use `CoreError::Config` for profile detection failures.

---

### `crates/hephaestus-api/src/state.rs` (modify -- generalize to PipelineKind)

**Analog:** Self.

**Key change points:**

Line 10 import:
```rust
// BEFORE:
use hephaestus_core::ClassifierPipeline;
// AFTER:
use hephaestus_core::PipelineKind;
```

Line 23 field:
```rust
// BEFORE:
pipeline: Mutex<ClassifierPipeline>,
// AFTER:
pipeline: Mutex<PipelineKind>,
```

Line 45-49 constructor:
```rust
// BEFORE:
pub fn new(pipeline: ClassifierPipeline, ...) -> Self {
// AFTER:
pub fn new(pipeline: PipelineKind, ...) -> Self {
```

Line 92-94 lock return type:
```rust
// BEFORE:
pub async fn lock_pipeline(&self) -> tokio::sync::MutexGuard<'_, ClassifierPipeline> {
// AFTER:
pub async fn lock_pipeline(&self) -> tokio::sync::MutexGuard<'_, PipelineKind> {
```

Add optional batcher handle field + `is_batching_enabled()` and `submit_batch()` accessors following existing accessor pattern (lines 62-94).

---

### `crates/hephaestus-api/src/handlers.rs` (modify -- generalize /infer response)

**Analog:** Self.

**Current handler pattern** (lines 54-128):
```rust
#[tracing::instrument(skip(state, req), fields(text_len = req.text.len()))]
pub async fn infer(
    State(state): State<Arc<AppState>>,
    Json(req): Json<InferRequest>,
) -> Result<Json<InferResponse>, ApiError> {
    if !state.is_ready() { return Err(ApiError::NotReady); }
    if req.text.is_empty() { return Err(ApiError::BadRequest(...)); }
    let request_start = Instant::now();
    let timer = StageTimer::new(state.model_id().to_string());
    let result = tokio::time::timeout(state.request_timeout(), async {
        let mut pipeline = state.lock_pipeline().await;
        let prepared = timer.time("tokenization", || pipeline.prepare(req.text))?;
        let output = timer.time("inference", || pipeline.execute(prepared))?;
        Ok::<_, ApiError>(output)
    }).await;
    // ... timeout handling, metrics, response
}
```

Changes:
1. Return type becomes `Result<Json<serde_json::Value>, ApiError>` (D-05)
2. Add batching path branch (D-06/D-07): if batching enabled, prepare then submit to batcher channel; else direct path as now
3. Response serialization delegates to `PipelineKind::execute()` which returns `serde_json::Value`

---

### `crates/hephaestus-api/src/batcher.rs` (new -- channel-based dynamic batcher)

**No direct analog in codebase.** Use patterns from RESEARCH.md (Pattern 2).

**Imports pattern** (from state.rs and handlers.rs conventions):
```rust
use std::sync::Arc;
use std::time::Duration;

use hephaestus_core::{CoreError, PipelineKind, PreparedInput};
use tokio::sync::{Mutex, mpsc, oneshot};
```

**Error handling pattern** (from handlers.rs):
```rust
.map_err(|e| CoreError::Inference(e.to_string()))?;
```

**Struct pattern** (from state.rs -- deep module accessors):
```rust
pub struct Batcher {
    tx: mpsc::Sender<BatchRequest>,
}

impl Batcher {
    pub fn new(...) -> Self { ... }
    pub async fn submit(&self, prepared: PreparedInput) -> Result<serde_json::Value, CoreError> { ... }
}
```

---

### `crates/hephaestus/src/config.rs` (modify -- add batch config fields)

**Analog:** Self.

**Field + default pattern** (lines 33-35, 80-82):
```rust
/// ONNX execution provider (default: `"cpu"`).
#[serde(default = "default_ep")]
pub execution_provider: String,

fn default_ep() -> String {
    "cpu".to_string()
}
```

New fields follow identical pattern:
```rust
#[serde(default)]
pub model_profile: Option<String>,

#[serde(default)]
pub batch_enabled: bool,

#[serde(default = "default_batch_max_size")]
pub batch_max_size: u32,

#[serde(default = "default_batch_max_wait_ms")]
pub batch_max_wait_ms: u64,

fn default_batch_max_size() -> u32 { 8 }
fn default_batch_max_wait_ms() -> u64 { 50 }
```

**Test helper pattern** (lines 156-171):
```rust
fn config_with_model_path(model_path: Option<&str>) -> Config {
    Config {
        model_id: "test-model".to_string(),
        // ... all fields with defaults
    }
}
```
Must update this helper to include the new fields.

---

### `crates/hephaestus/src/main.rs` (modify -- add profile detection + batcher init)

**Analog:** Self.

**Startup sequence** (lines 70-82):
```rust
// 4. Construct the classifier pipeline.
let pipeline = ClassifierPipeline::new(&model_dir)
    .context("failed to construct classifier pipeline")?;

// 5. Build shared state.
let state = Arc::new(AppState::new(
    pipeline,
    config.model_id.clone(),
    Duration::from_secs(config.request_timeout_secs),
    metrics_handle,
));
```

Phase 4 inserts profile detection between steps 3 and 4:
```rust
// 3b. Detect model profile from config.json.
let config_json = std::fs::read_to_string(model_dir.join("config.json"))?;
let model_config: serde_json::Value = serde_json::from_str(&config_json)?;
let profile = hephaestus_core::detect_profile(&model_config, config.model_profile.as_deref())?;

// 4. Construct the appropriate pipeline based on detected profile.
let pipeline_kind = match profile {
    ModelProfile::Classifier => PipelineKind::Classifier(ClassifierPipeline::new(&model_dir)?),
    ModelProfile::Embeddings => PipelineKind::Embeddings(EmbeddingsPipeline::new(&model_dir)?),
    // ...
};
```

Then optionally init batcher after state construction:
```rust
// 5b. Initialize batcher if enabled.
if config.batch_enabled {
    let batcher = Batcher::new(state.pipeline_arc(), config.batch_max_size, ...);
    state.set_batcher(batcher);
    tokio::spawn(batcher_loop);
}
```

## Shared Patterns

### Error Handling
**Source:** `crates/hephaestus-core/src/error.rs` (lines 1-35)
**Apply to:** All core crate files (pipeline.rs, postprocess.rs, profile.rs)
```rust
use crate::error::CoreError;

// Map external errors to CoreError variants:
.map_err(|e| CoreError::Inference(e.to_string()))?;
.map_err(|e| CoreError::Tokenization(e.to_string()))?;
```

### ONNX Session Inference
**Source:** `crates/hephaestus-core/src/pipeline.rs` (lines 193-221)
**Apply to:** All pipeline execute() methods
```rust
// Tensor construction
let input_ids_array = Array2::from_shape_vec((1, seq_len), prepared.input_ids)
    .map_err(|e| CoreError::Inference(e.to_string()))?;
let attention_mask_array = Array2::from_shape_vec((1, seq_len), prepared.attention_mask)
    .map_err(|e| CoreError::Inference(e.to_string()))?;

// TensorRef creation + session run
let input_ids_tensor = TensorRef::from_array_view(input_ids_array.view())
    .map_err(|e| CoreError::Inference(e.to_string()))?;
let attention_mask_tensor = TensorRef::from_array_view(attention_mask_array.view())
    .map_err(|e| CoreError::Inference(e.to_string()))?;
let outputs = self.session.run(ort::inputs![
    "input_ids" => input_ids_tensor,
    "attention_mask" => attention_mask_tensor,
]).map_err(|e| CoreError::Inference(e.to_string()))?;

// Output extraction
let tensor = outputs[0].try_extract_tensor::<f32>()
    .map_err(|e| CoreError::Inference(e.to_string()))?;
```

### Config Field Pattern
**Source:** `crates/hephaestus/src/config.rs` (lines 24-78)
**Apply to:** New config fields
```rust
#[serde(default = "default_fn_name")]
pub field_name: Type,

fn default_fn_name() -> Type { value }
```

### Tokenization (prepare)
**Source:** `crates/hephaestus-core/src/pipeline.rs` (lines 170-191)
**Apply to:** All pipeline prepare() implementations (shared helper)
```rust
let encoding = self.tokenizer
    .encode(input.as_str(), true)
    .map_err(|e| CoreError::Tokenization(e.to_string()))?;
let input_ids: Vec<i64> = encoding.get_ids().iter().map(|&id| i64::from(id)).collect();
let attention_mask: Vec<i64> = encoding.get_attention_mask().iter().map(|&m| i64::from(m)).collect();
```

## No Analog Found

| File | Role | Data Flow | Reason |
|------|------|-----------|--------|
| `crates/hephaestus-api/src/batcher.rs` | service | batch | No async channel-based batch collector exists in the codebase. Use RESEARCH.md Pattern 2 (channel-based batcher) as the reference pattern. |

## Metadata

**Analog search scope:** `crates/hephaestus-core/src/`, `crates/hephaestus-api/src/`, `crates/hephaestus/src/`
**Files scanned:** 8 existing source files
**Pattern extraction date:** 2026-08-26
