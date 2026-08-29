# Phase 12: Hot-Path Performance Optimization - Pattern Map

**Mapped:** 2026-08-29
**Files analyzed:** 5 (all modifications, no new files)
**Analogs found:** 5 / 5 (self-analog -- each file is modified in-place)

## File Classification

| Modified File | Role | Data Flow | Closest Analog | Match Quality |
|---------------|------|-----------|----------------|---------------|
| `crates/hephaestus-core/src/pipeline.rs` (run_onnx_inference) | service | request-response | self | exact |
| `crates/hephaestus-core/src/pipeline.rs` (EmbeddingsPipeline::execute) | service | request-response | self | exact |
| `crates/hephaestus-core/src/pipeline.rs` (AsrPipeline::prepare + execute_whisper) | service | streaming | self | exact |
| `crates/hephaestus-api/src/metrics.rs` (StageTimer) | utility | request-response | self | exact |
| `crates/hephaestus-api/src/grpc/inference.rs` (StageTimer caller) | controller | request-response | `crates/hephaestus-api/src/handlers.rs` line 59 | exact |

## Pattern Assignments

### `crates/hephaestus-core/src/pipeline.rs` -- run_onnx_inference (service, request-response)

**Current code to replace** (lines 364-375):
```rust
fn run_onnx_inference<'a>(
    session: &'a mut Session,
    prepared: &'a PreparedInput,
) -> Result<ort::session::SessionOutputs<'a>, CoreError> {
    let seq_len = prepared.sequence_length;
    let needs_token_type_ids = session_expects_token_type_ids(session);

    let input_ids_array =
        Array2::from_shape_vec((1, seq_len), prepared.input_ids.clone())
            .map_err(|e| CoreError::Inference(e.to_string()))?;
    let attention_mask_array =
        Array2::from_shape_vec((1, seq_len), prepared.attention_mask.clone())
            .map_err(|e| CoreError::Inference(e.to_string()))?;
```

**Target pattern -- zero-copy ArrayView:**
```rust
fn run_onnx_inference<'s>(
    session: &'s mut Session,
    prepared: &PreparedInput,
) -> Result<ort::session::SessionOutputs<'s>, CoreError> {
    let seq_len = prepared.sequence_length;
    let needs_token_type_ids = session_expects_token_type_ids(session);

    let input_ids_view = ndarray::ArrayView2::from_shape(
        (1, seq_len), &prepared.input_ids,
    ).map_err(|e| CoreError::Inference(e.to_string()))?;
    let attention_mask_view = ndarray::ArrayView2::from_shape(
        (1, seq_len), &prepared.attention_mask,
    ).map_err(|e| CoreError::Inference(e.to_string()))?;
```

**Key change:** Lifetime separation -- `session` gets `'s`, `prepared` gets anonymous `'_`. This allows callers to access `prepared` fields after `run_onnx_inference` returns.

**Downstream TensorRef calls** (lines 379-383) change from `.view()` on owned Array2 to direct use of the ArrayView:
```rust
// BEFORE:
let input_ids_tensor = TensorRef::from_array_view(input_ids_array.view())
// AFTER:
let input_ids_tensor = TensorRef::from_array_view(input_ids_view)
```

---

### `crates/hephaestus-core/src/pipeline.rs` -- EmbeddingsPipeline::execute (service, request-response)

**Current code to change** (lines 536-538):
```rust
fn execute(&mut self, prepared: PreparedInput) -> Result<Vec<f32>, CoreError> {
    let attention_mask = prepared.attention_mask.clone();
    let outputs = run_onnx_inference(&mut self.session, &prepared)?;
```

**Target pattern -- remove pre-clone:**
```rust
fn execute(&mut self, prepared: PreparedInput) -> Result<Vec<f32>, CoreError> {
    let outputs = run_onnx_inference(&mut self.session, &prepared)?;
    // ... later use prepared.attention_mask directly (lifetime now allows it)
```

This works because the lifetime fix in `run_onnx_inference` means `prepared` is no longer borrowed by `outputs`.

---

### `crates/hephaestus-core/src/pipeline.rs` -- PreparedAudio and AsrPipeline (service, streaming)

**Dead field removal** (lines 69-85):
```rust
// BEFORE:
pub struct PreparedAudio {
    pub(crate) features: Array2<f32>,
    #[allow(dead_code)]
    pub(crate) raw_samples: Option<Vec<f32>>,
}
impl PreparedAudio {
    pub fn new_for_test(features: Array2<f32>, raw_samples: Option<Vec<f32>>) -> Self {
        Self { features, raw_samples }
    }
}

// AFTER:
pub struct PreparedAudio {
    pub(crate) features: Array2<f32>,
}
impl PreparedAudio {
    pub fn new_for_test(features: Array2<f32>) -> Self {
        Self { features }
    }
}
```

**CTC prepare path** (lines 1174-1182) -- consume input directly:
```rust
// BEFORE:
let features = Array2::from_shape_vec((1, num_samples), input.clone())?;
Ok(PreparedAudio { features, raw_samples: Some(input) })

// AFTER:
let features = Array2::from_shape_vec((1, num_samples), input)?;
Ok(PreparedAudio { features })
```

**Whisper decode loop** (lines 1278-1283) -- zero-copy token tensor:
```rust
// BEFORE:
let token_array = Array2::from_shape_vec((1, seq_len), tokens.clone())?;
let token_tensor = TensorRef::from_array_view(token_array.view())?;

// AFTER:
let token_view = ndarray::ArrayView2::from_shape((1, seq_len), &tokens)?;
let token_tensor = TensorRef::from_array_view(token_view)?;
```

---

### `crates/hephaestus-api/src/metrics.rs` -- StageTimer (utility, request-response)

**Current code** (lines 40-48):
```rust
pub struct StageTimer {
    model_id: String,
}
impl StageTimer {
    pub fn new(model_id: String) -> Self {
        Self { model_id }
    }
```

**Target pattern -- Arc\<str\>:**
```rust
pub struct StageTimer {
    model_id: Arc<str>,
}
impl StageTimer {
    pub fn new(model_id: impl Into<Arc<str>>) -> Self {
        Self { model_id: model_id.into() }
    }
```

**Clone sites** (lines 62, 77, 82) -- `self.model_id.clone()` stays syntactically identical but becomes an atomic ref-count bump instead of heap allocation.

**Test updates** (lines 103, 113, 125) -- `StageTimer::new("test-model".to_string())` works unchanged because `String` implements `Into<Arc<str>>`. Alternatively simplify to `StageTimer::new("test-model")` since `&str` also implements `Into<Arc<str>>`.

---

### Caller updates -- handlers.rs and grpc/inference.rs

**handlers.rs line 59:**
```rust
// BEFORE:
let timer = StageTimer::new(state.model_id().to_string());
// AFTER (works unchanged with impl Into<Arc<str>>):
let timer = StageTimer::new(state.model_id().to_string());
// OR (saves the String allocation):
let timer = StageTimer::new(state.model_id());
```

**grpc/inference.rs line 57:** Same pattern as handlers.rs.

## Shared Patterns

### Error Handling (map_err to CoreError::Inference)
**Source:** `crates/hephaestus-core/src/pipeline.rs` lines 372-375
**Apply to:** All ArrayView2::from_shape calls (same error mapping pattern)
```rust
.map_err(|e| CoreError::Inference(e.to_string()))?;
```

### Import additions
**File:** `crates/hephaestus-core/src/pipeline.rs` line 5
**Current:** `use ndarray::Array2;`
**Add:** No new import needed -- `ndarray::ArrayView2::from_shape` uses fully-qualified path in the examples. Alternatively add `use ndarray::ArrayView2;` for brevity.

## No Analog Found

None -- all changes are modifications to existing files using existing patterns.

## Metadata

**Analog search scope:** `crates/hephaestus-core/src/`, `crates/hephaestus-api/src/`
**Files scanned:** 6
**Pattern extraction date:** 2026-08-29
