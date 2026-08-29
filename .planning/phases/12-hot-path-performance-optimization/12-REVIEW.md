---
phase: 12-hot-path-performance-optimization
reviewed: 2026-08-29T12:00:00Z
depth: standard
files_reviewed: 4
files_reviewed_list:
  - crates/hephaestus-api/src/grpc/inference.rs
  - crates/hephaestus-api/src/handlers.rs
  - crates/hephaestus-api/src/metrics.rs
  - crates/hephaestus-core/src/pipeline.rs
findings:
  critical: 0
  warning: 3
  info: 2
  total: 5
status: issues_found
---

# Phase 12: Code Review Report

**Reviewed:** 2026-08-29T12:00:00Z
**Depth:** standard
**Files Reviewed:** 4
**Status:** issues_found

## Summary

Phase 12 optimizes the hot path by: (1) switching `StageTimer.model_id` from `String` to `Arc<str>` for cheaper clones inside metrics recording, (2) replacing `Array2::from_shape_vec(vec.clone())` with zero-copy `ArrayView2::from_shape(&slice)` in `run_onnx_inference` and the Whisper decode loop, (3) decoupling the `prepared` lifetime from the session lifetime in `run_onnx_inference` to eliminate a defensive `attention_mask.clone()` in `EmbeddingsPipeline::execute`, and (4) removing the dead `raw_samples` field from `PreparedAudio`.

The lifetime decoupling and ArrayView changes are correct -- verified that `ort::SessionOutputs<'s>` borrows only the session, not the input tensors, so the shortened lifetime on `prepared` is sound. The `metrics` crate's `Cow<str>` type natively supports `Arc<str>` via a `Shared` variant that clones with a single `Arc::increment_strong_count` call (no heap allocation), confirming the `Arc<str>` choice is well-matched to the metrics crate internals.

No blockers found. Three warnings identify missed optimization opportunities and a metrics accuracy concern relevant to this phase's performance goals.

## Narrative Findings (AI reviewer)

## Warnings

### WR-01: Per-request Arc<str> allocation from &str -- incomplete optimization

**File:** `crates/hephaestus-api/src/metrics.rs:46-49`, `crates/hephaestus-api/src/handlers.rs:59`, `crates/hephaestus-api/src/grpc/inference.rs:57`
**Issue:** `StageTimer::new(state.model_id())` converts `&str` to `Arc<str>` on every request, which heap-allocates a new `Arc<str>` each time. The `model_id` is constant for the process lifetime (set once at startup in `AppState`). Storing `Arc<str>` in `AppState` and passing a clone (atomic refcount bump, no allocation) to `StageTimer::new` would eliminate this per-request allocation entirely.

The inner clones in `time()` and `finish_request()` are already cheap (the metrics crate's `Cow::from_shared` wraps the Arc and `clone_shared` just calls `Arc::increment_strong_count`), so the only remaining allocation is the initial `&str -> Arc<str>` conversion on each request.

**Fix:**
```rust
// In AppState:
pub struct AppState {
    model_id: String,
    model_id_arc: Arc<str>,  // cached for StageTimer
    // ...
}

impl AppState {
    pub fn model_id_arc(&self) -> Arc<str> {
        self.model_id_arc.clone()  // atomic increment only
    }
}

// In handlers:
let timer = StageTimer::new(state.model_id_arc());
```

### WR-02: StageTimer::time records duration to histograms on error paths

**File:** `crates/hephaestus-api/src/metrics.rs:57-68`
**Issue:** The `time()` method unconditionally records the elapsed duration into the `hephaestus_stage_duration_seconds` histogram, including when the timed closure returns an `Err`. Fast-failing error paths (e.g., tokenization rejecting empty input, malformed encoding) record near-zero durations that distort the histogram's percentile calculations. For a performance optimization phase, accurate stage latency metrics are especially important to validate the optimizations.

**Fix:**
```rust
pub fn time<T, E>(&self, stage: &'static str, f: impl FnOnce() -> Result<T, E>) -> Result<T, E> {
    let start = Instant::now();
    let result = f();
    if result.is_ok() {
        let elapsed = start.elapsed().as_secs_f64();
        metrics::histogram!(
            "hephaestus_stage_duration_seconds",
            "stage" => stage,
            "model_id" => self.model_id.clone(),
        )
        .record(elapsed);
    }
    result
}
```

Alternatively, record with a `status` label to keep error timing without polluting the success histogram:
```rust
"status" => if result.is_ok() { "ok" } else { "error" },
```

### WR-03: Unconditional token_type_ids heap allocation in run_onnx_inference

**File:** `crates/hephaestus-core/src/pipeline.rs:373`
**Issue:** `Array2::<i64>::zeros((1, seq_len))` allocates and zeroes `seq_len * 8` bytes on every `run_onnx_inference` call, including when `needs_token_type_ids` is false (DistilBERT-family models). Since DistilBERT is the primary model in the README examples and likely the most common deployment, this allocation is wasted on the majority of inference calls. This function was modified in this phase (lifetime + ArrayView changes), so the missed optimization is in scope.

**Fix:**
```rust
if needs_token_type_ids {
    let token_type_ids_array = Array2::<i64>::zeros((1, seq_len));
    let token_type_ids_tensor =
        TensorRef::from_array_view(token_type_ids_array.view())
            .map_err(|e| CoreError::Inference(e.to_string()))?;
    session
        .run(ort::inputs![
            "input_ids" => input_ids_tensor,
            "attention_mask" => attention_mask_tensor,
            "token_type_ids" => token_type_ids_tensor,
        ])
        .map_err(|e| CoreError::Inference(e.to_string()))
} else {
    session
        .run(ort::inputs![
            "input_ids" => input_ids_tensor,
            "attention_mask" => attention_mask_tensor,
        ])
        .map_err(|e| CoreError::Inference(e.to_string()))
}
```

## Info

### IN-01: JSON response enrichment allocates heap strings for static keys

**File:** `crates/hephaestus-api/src/handlers.rs:134,138`, `crates/hephaestus-api/src/grpc/inference.rs:130,134`
**Issue:** `"model_id".to_string()` and `"latency_ms".to_string()` allocate two heap `String`s per request for use as `serde_json::Map` keys. This is a limitation of the `serde_json::Map<String, Value>` API (keys must be `String`). Consider restructuring to use a typed response struct with `#[serde(flatten)]` to avoid the JSON map mutation pattern entirely, which would also eliminate the silent no-op risk if `to_json()` ever returns a non-object value.

**Fix:** Define a wrapper struct:
```rust
#[derive(Serialize)]
struct InferResponse {
    model_id: String,
    latency_ms: u64,
    #[serde(flatten)]
    output: serde_json::Value,
}
```

### IN-02: Batch execution processes dummy entries through ONNX inference

**File:** `crates/hephaestus-core/src/pipeline.rs:1435-1439`
**Issue:** When audio `PreparedData` items appear in a text-pipeline batch, dummy `PreparedInput::new_for_test` entries are inserted to maintain index alignment. These dummies are padded, fed through ONNX inference, and post-processed before their results are replaced with errors (lines 1570-1576). This wastes compute on the dummy entries. A more efficient approach would filter audio items out before inference and reconstruct the result vector with errors at the correct indices afterwards.

---

_Reviewed: 2026-08-29T12:00:00Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
