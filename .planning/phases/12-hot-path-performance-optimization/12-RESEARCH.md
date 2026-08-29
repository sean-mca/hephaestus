# Phase 12: Hot-Path Performance Optimization - Research

**Researched:** 2026-08-29
**Domain:** Rust memory optimization, zero-copy tensor construction, reference counting
**Confidence:** HIGH

## Summary

Phase 12 eliminates unnecessary heap allocations and data copies on the inference hot path. The codebase has five specific optimization targets identified by rules/ audit: two Vec clones in `run_onnx_inference` that copy tokenized data into Array2 tensors on every request, a pre-clone of attention_mask in the embeddings pipeline, a dead `raw_samples` field in PreparedAudio that forces a ~1.9MB audio buffer clone on every ASR window, per-iteration Vec clones in the Whisper decode loop, and three per-request String clones of model_id in metrics recording.

All optimizations use existing Rust primitives and ndarray APIs already in the dependency tree. No new crates are needed. The changes are internal to `hephaestus-core` and `hephaestus-api` with no public API surface changes. The primary patterns are: `ndarray::ArrayView2::from_shape` for zero-copy tensor views (replacing `Array2::from_shape_vec` with cloned vecs), dead field removal, and `Arc<str>` for shared string identity in metrics labels (replacing `String` clones with atomic ref-count bumps).

**Primary recommendation:** Replace each `Array2::from_shape_vec(shape, vec.clone())` with `ndarray::ArrayView2::from_shape(shape, &slice)` to construct ONNX input tensors without copying; change `StageTimer::model_id` from `String` to `Arc<str>`; remove the dead `raw_samples` field from `PreparedAudio`.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| ONNX tensor construction | hephaestus-core (pipeline.rs) | -- | Tensor building is internal to pipeline execute/inference functions |
| Metrics recording | hephaestus-api (metrics.rs) | -- | StageTimer is the metrics abstraction used by handlers |
| ASR audio preprocessing | hephaestus-core (pipeline.rs) | -- | PreparedAudio struct and prepare() live in core |
| Whisper autoregressive decode | hephaestus-core (pipeline.rs) | -- | Decode loop is internal to AsrPipeline::execute_whisper |

## Standard Stack

### Core (no new dependencies)

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| ndarray | 0.17.2 | Tensor data views | Already in workspace. `ArrayView2::from_shape(&[T])` creates zero-copy views. [VERIFIED: cargo registry -- ndarray 0.17.2 source confirms `ArrayView::from_shape` at impl_views/constructors.rs:47] |
| std::sync::Arc | stable | Ref-counted strings | Standard library. `Arc<str>` clone is an atomic increment, not a heap alloc. [VERIFIED: Rust std docs] |
| metrics | 0.24.6 | Prometheus metrics | Already in workspace. `SharedString` (alias for `Cow<'static, str>`) has `From<Arc<T>>` impl -- Arc labels avoid cloning. [VERIFIED: metrics-0.24.6 source cow.rs:328] |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| ArrayView2::from_shape | Cow<[i64]> in PreparedInput | Cow adds complexity; ArrayView is the ndarray-native pattern for borrowing |
| Arc\<str\> for model_id | &'static str | Would require leaking; model_id comes from runtime config |
| Remove raw_samples | Keep with lazy init | Field is dead code with allow(dead_code); no justification to keep |

## Architecture Patterns

### System Architecture Diagram

```
Request (text/audio)
    |
    v
[Handler] -- creates StageTimer(Arc<str>)
    |
    v
[Pipeline::prepare] -- tokenizes text / extracts features
    |                   returns PreparedInput (owns Vec<i64>)
    v                   or PreparedAudio (owns Array2<f32>)
[run_onnx_inference] -- constructs ArrayView2 from &PreparedInput.input_ids  <-- ZERO COPY
    |                   constructs ArrayView2 from &PreparedInput.attention_mask  <-- ZERO COPY
    |                   passes views to TensorRef::from_array_view
    |                   passes TensorRefs to session.run()
    v
[Post-processing] -- uses &prepared.attention_mask directly  <-- NO PRE-CLONE
    |                  (softmax, mean_pool, argmax, CTC decode)
    v
[StageTimer::time/finish_request] -- records metrics with Arc<str> clone  <-- REF-COUNT BUMP
    |
    v
Response (JSON/gRPC)
```

### Recommended Project Structure

No structural changes. All modifications are in existing files:
```
crates/
  hephaestus-core/src/
    pipeline.rs        # run_onnx_inference, EmbeddingsPipeline, AsrPipeline, PreparedAudio
  hephaestus-api/src/
    metrics.rs         # StageTimer
    handlers.rs        # StageTimer construction (callers)
    grpc/inference.rs  # StageTimer construction (callers)
    state.rs           # AppState.model_id (optional: internal Arc<str>)
```

### Pattern 1: Zero-Copy Tensor Construction via ArrayView

**What:** Replace `Array2::from_shape_vec(shape, vec.clone())` with `ndarray::ArrayView2::from_shape(shape, &slice)`. The view borrows the existing slice without copying.

**When to use:** Whenever constructing an ndarray from data that is already owned elsewhere and only needs to be read (not mutated or stored).

**Example:**
```rust
// Source: ndarray 0.17.2 impl_views/constructors.rs:47
// BEFORE (clones the Vec):
let input_ids_array = Array2::from_shape_vec((1, seq_len), prepared.input_ids.clone())?;
let tensor = TensorRef::from_array_view(input_ids_array.view())?;

// AFTER (zero-copy view into existing slice):
let input_ids_view = ndarray::ArrayView2::from_shape((1, seq_len), &prepared.input_ids)?;
let tensor = TensorRef::from_array_view(input_ids_view)?;
```

### Pattern 2: Arc\<str\> for Shared Identity Labels

**What:** Store frequently-cloned string identifiers as `Arc<str>` instead of `String`. Each `.clone()` becomes an atomic ref-count increment (no heap allocation).

**When to use:** When the same string value is cloned multiple times per request into metrics labels, log fields, or similar repeated uses.

**Example:**
```rust
// Source: Rust std, metrics crate cow.rs:328
// BEFORE:
pub struct StageTimer {
    model_id: String,
}
// Each metrics call: self.model_id.clone() -> heap alloc + memcpy

// AFTER:
pub struct StageTimer {
    model_id: Arc<str>,
}
// Each metrics call: self.model_id.clone() -> atomic increment
```

### Pattern 3: Lifetime Separation for Borrowed Inference

**What:** Decouple the lifetime of `prepared` from `session` in `run_onnx_inference` so the caller can access `prepared` fields after inference completes.

**When to use:** When `SessionOutputs` borrows `session` but not the input tensors. ONNX Runtime copies input data during `session.run()` and output tensors reference session-internal memory.

**Example:**
```rust
// BEFORE (both tied to 'a):
fn run_onnx_inference<'a>(
    session: &'a mut Session,
    prepared: &'a PreparedInput,
) -> Result<SessionOutputs<'a>, CoreError>

// AFTER (prepared decoupled):
fn run_onnx_inference<'s>(
    session: &'s mut Session,
    prepared: &PreparedInput,
) -> Result<SessionOutputs<'s>, CoreError>
```

### Anti-Patterns to Avoid

- **Cloning data just to reshape it:** `Array2::from_shape_vec` consumes the Vec. If you only need a view, use `ArrayView2::from_shape` which borrows. The clone was a workaround for ownership, not a requirement. [ASSUMED]
- **Pre-cloning fields before a borrow:** The `EmbeddingsPipeline::execute` clones `attention_mask` before passing `&prepared` to inference, because the old lifetime annotation made `prepared` inaccessible afterward. Fix the lifetime, remove the clone. [VERIFIED: codebase analysis -- pipeline.rs:537]
- **Storing dead fields that cause clones:** `PreparedAudio::raw_samples` is `#[allow(dead_code)]` and forces a 1.9MB clone in the CTC prepare path. Dead fields with resource costs must be removed, not annotated. [VERIFIED: codebase analysis -- pipeline.rs:74-76, 1177]

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Zero-copy tensor views | Manual pointer arithmetic | `ndarray::ArrayView2::from_shape` | Handles stride calculation, bounds checking, and safety invariants |
| Shared string identity | Custom ref-counted string | `std::sync::Arc<str>` | Standard library, zero-cost deref to `&str`, well-optimized atomics |
| Clone-on-write strings | Manual string dedup | metrics crate's `SharedString` / `Cow` | Already integrated into the metrics label system |

**Key insight:** Every optimization in this phase uses primitives already in the dependency tree. No new crates, no custom allocators, no unsafe code.

## Common Pitfalls

### Pitfall 1: ArrayView Lifetime Escaping Its Source

**What goes wrong:** Creating an `ArrayView` from a local variable and trying to return it or store it past the source's lifetime.
**Why it happens:** `ArrayView::from_shape` borrows the source slice; the view cannot outlive the slice.
**How to avoid:** Ensure the source data (`PreparedInput.input_ids`, `PreparedInput.attention_mask`) lives at least as long as the `ArrayView`. In `run_onnx_inference`, the views are stack-local and consumed by `session.run()` within the same function scope. No issue.
**Warning signs:** Compiler error "borrowed value does not live long enough."

### Pitfall 2: SessionOutputs Borrowing Input Data

**What goes wrong:** Assuming `SessionOutputs` borrows input tensor data, preventing access to `prepared` after inference.
**Why it happens:** The current function signature ties `prepared` and `session` to the same lifetime `'a`, creating the illusion of a dependency.
**How to avoid:** `SessionOutputs` borrows session (for output tensor memory), not input data. ONNX Runtime reads inputs during `run()` and stores outputs in session-managed buffers. Separate the lifetimes: session gets `'s`, prepared gets anonymous `'_`.
**Warning signs:** The EmbeddingsPipeline pre-clone of `attention_mask` is the symptom of this pitfall.

### Pitfall 3: Arc<str> Creation Cost

**What goes wrong:** Creating `Arc<str>` from `&str` on every request, which allocates once, and thinking this is free.
**Why it happens:** `Arc::from(str_slice)` allocates once to create the Arc. The savings come from subsequent `.clone()` calls being ref-count bumps instead of full copies.
**How to avoid:** If feasible, store `Arc<str>` in `AppState` and clone the Arc into `StageTimer` (one atomic op per request). Otherwise, accept the single `Arc::from()` allocation per request -- it replaces the previous `to_string()` allocation and saves 3 subsequent clones.
**Warning signs:** Benchmarks showing no improvement -- measure the clone sites, not construction.

### Pitfall 4: Whisper Decode Loop Token Array

**What goes wrong:** Using `ArrayView2::from_shape` for the tokens array in the decode loop, but the tokens vec grows each iteration.
**Why it happens:** Each iteration pushes a new token, so the vec's pointer may be invalidated by reallocation.
**How to avoid:** The ArrayView is created fresh each iteration from the current `&tokens` slice. After `session.run()` returns and the view is dropped, `tokens.push()` can reallocate safely. The view does not persist across iterations.
**Warning signs:** None -- the pattern is inherently safe because the view is scoped to one iteration.

### Pitfall 5: Breaking Test Assertions on PreparedAudio

**What goes wrong:** Removing `raw_samples` from `PreparedAudio` breaks `PreparedAudio::new_for_test` and any tests that construct it.
**Why it happens:** The test constructor and test code reference the removed field.
**How to avoid:** Update `new_for_test` to remove the `raw_samples` parameter. Search for all call sites (including downstream crate tests).
**Warning signs:** `cargo test --workspace` compilation errors.

## Code Examples

### Zero-Copy run_onnx_inference (primary optimization)

```rust
// Source: ndarray 0.17.2 ArrayView::from_shape, ort TensorRef::from_array_view
fn run_onnx_inference<'s>(
    session: &'s mut Session,
    prepared: &PreparedInput,
) -> Result<ort::session::SessionOutputs<'s>, CoreError> {
    let seq_len = prepared.sequence_length;
    let needs_token_type_ids = session_expects_token_type_ids(session);

    // Zero-copy views into PreparedInput's owned Vecs.
    let input_ids_view = ndarray::ArrayView2::from_shape(
        (1, seq_len), &prepared.input_ids,
    ).map_err(|e| CoreError::Inference(e.to_string()))?;

    let attention_mask_view = ndarray::ArrayView2::from_shape(
        (1, seq_len), &prepared.attention_mask,
    ).map_err(|e| CoreError::Inference(e.to_string()))?;

    let token_type_ids_array = Array2::<i64>::zeros((1, seq_len));

    let input_ids_tensor = TensorRef::from_array_view(input_ids_view)
        .map_err(|e| CoreError::Inference(e.to_string()))?;
    let attention_mask_tensor = TensorRef::from_array_view(attention_mask_view)
        .map_err(|e| CoreError::Inference(e.to_string()))?;

    // ... rest unchanged
}
```

### EmbeddingsPipeline Without Pre-Clone

```rust
// After lifetime separation, prepared is accessible after run_onnx_inference
fn execute(&mut self, prepared: PreparedInput) -> Result<Vec<f32>, CoreError> {
    // NO CLONE: let attention_mask = prepared.attention_mask.clone();
    let outputs = run_onnx_inference(&mut self.session, &prepared)?;
    check_outputs_nonempty(&outputs)?;
    let tensor = outputs[0].try_extract_tensor::<f32>()?;
    let (shape, data) = tensor;
    // ...
    // Use prepared.attention_mask directly (no longer borrowed by outputs)
    let mut pooled = postprocess::mean_pool(data, &prepared.attention_mask, hidden_dim)?;
    postprocess::l2_normalize(&mut pooled);
    Ok(pooled)
}
```

### StageTimer with Arc\<str\>

```rust
// Source: std::sync::Arc, metrics crate SharedString From<Arc<T>>
pub struct StageTimer {
    model_id: Arc<str>,
}

impl StageTimer {
    pub fn new(model_id: impl Into<Arc<str>>) -> Self {
        Self { model_id: model_id.into() }
    }

    pub fn time<T>(&self, stage: &'static str, f: impl FnOnce() -> T) -> T {
        let start = Instant::now();
        let result = f();
        let elapsed = start.elapsed().as_secs_f64();
        metrics::histogram!(
            "hephaestus_stage_duration_seconds",
            "stage" => stage,
            "model_id" => self.model_id.clone(), // Arc clone = atomic increment
        )
        .record(elapsed);
        result
    }
}
```

### ASR CTC Prepare Without Dead Field Clone

```rust
// After removing raw_samples from PreparedAudio
fn prepare(&self, input: Vec<f32>) -> Result<PreparedAudio, CoreError> {
    if self.feature_extractor == "mel" {
        let mel_features = crate::mel::compute_mel_spectrogram(
            &input, self.n_fft, self.hop_length, self.n_mels, 16000,
        )?;
        Ok(PreparedAudio { features: mel_features })
    } else {
        let num_samples = input.len();
        // Consume input directly -- no clone needed (raw_samples removed)
        let features = Array2::from_shape_vec((1, num_samples), input)
            .map_err(|e| CoreError::Inference(format!("waveform reshape failed: {e}")))?;
        Ok(PreparedAudio { features })
    }
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `Array2::from_shape_vec` with clone | `ArrayView2::from_shape` borrowing | ndarray 0.13+ (always available) | Eliminates per-request Vec allocations in tensor construction |
| `String` for repeated label values | `Arc<str>` or metrics crate `Cow::Shared` | Rust 1.21+ (Arc\<str\> stabilized) | Replaces heap allocs with atomic ref-count bumps |
| Dead fields with `#[allow(dead_code)]` | Remove entirely | Always best practice | Eliminates unnecessary clones and memory waste |

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | ONNX Runtime copies input tensor data during session.run() and SessionOutputs does not borrow input memory | Architecture Patterns (Pattern 3) | If SessionOutputs borrows inputs, the lifetime separation would cause a compile error -- detected at build time, zero runtime risk |
| A2 | ArrayView2::from_shape with (1, seq_len) and a slice of length seq_len will succeed | Code Examples | ShapeError if slice length != product of dimensions -- same validation as from_shape_vec, detected at runtime with existing error handling |

## Open Questions

1. **Should AppState.model_id become Arc\<str\> internally?**
   - What we know: StageTimer.model_id will be Arc\<str\>. Currently handlers do `state.model_id().to_string()` which allocates a String, then `StageTimer::new` would convert to Arc\<str\> (another allocation).
   - What's unclear: Whether the extra complexity of making AppState store Arc\<str\> is worth saving one allocation per request.
   - Recommendation: Keep AppState.model_id as String. The `StageTimer::new(state.model_id())` path does one `Arc::from(&str)` allocation per request, replacing the previous `to_string()` allocation -- net zero new allocations, but saves 3 clones. Changing AppState is optional scope creep.

2. **Should the Whisper encoder output reconstruction avoid to_vec()?**
   - What we know: Line 1267 does `enc_data.to_vec()` to reconstruct a 3D array from the extracted encoder output tensor.
   - What's unclear: Whether `try_extract_tensor` returns data that can be reshaped in-place without copying.
   - Recommendation: Out of scope for this phase. The encoder runs once per audio window (not in the decode loop), so the single copy is not a hot-path bottleneck. Track for a future phase if profiling shows it matters.

## Project Constraints (from CLAUDE.md)

- **Language:** Rust only, 2024 edition, workspace resolver 3
- **Rules compliance:** All rules in `rules/` must be followed -- this phase directly addresses violations of `anti-clone-excessive.md`, `own-borrow-over-clone.md`, `anti-vec-for-slice.md`, `mem-zero-copy.md`, and `own-arc-shared.md`
- **No AI attribution:** Never include Co-Authored-By lines or AI references in commits
- **Deep module pattern:** Traits expose 1-3 methods hiding complexity. StageTimer API surface does not change.
- **GSD workflow:** All edits through GSD commands

## Validation Architecture

### Test Framework

| Property | Value |
|----------|-------|
| Framework | Rust built-in test + cargo test |
| Config file | Cargo.toml workspace |
| Quick run command | `cargo test --workspace` |
| Full suite command | `cargo test --workspace` |

### Phase Requirements to Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| HP-01 | run_onnx_inference uses ArrayView (no clone) | unit | `cargo test --workspace` (existing classifier/embeddings tests exercise this path) | Existing |
| HP-02 | EmbeddingsPipeline::execute no pre-clone | unit | `cargo test --workspace` (existing embeddings tests) | Existing |
| HP-03 | ASR CTC prepare consumes Vec directly | unit | `cargo test --workspace` (PreparedAudio tests) | Existing |
| HP-04 | Whisper decode uses ArrayView | unit | `cargo test --workspace` (ASR tests) | Existing |
| HP-05 | StageTimer::model_id is Arc\<str\> | unit | `cargo test --workspace` (metrics tests) | Existing |
| HP-06 | All existing tests pass | integration | `cargo test --workspace` | Existing |
| HP-07 | No public API surface changes | build | `cargo build --workspace` (compile-time check) | N/A |

### Sampling Rate

- **Per task commit:** `cargo test --workspace`
- **Per wave merge:** `cargo test --workspace`
- **Phase gate:** Full suite green before `/gsd-verify-work`

### Wave 0 Gaps

None -- existing test infrastructure covers all phase requirements. The optimizations are internal implementation changes; if any break the existing behavior, existing tests will catch it.

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no | N/A |
| V3 Session Management | no | N/A |
| V4 Access Control | no | N/A |
| V5 Input Validation | no | No input handling changes |
| V6 Cryptography | no | N/A |

No security implications. All changes are internal memory management optimizations that do not alter input handling, output format, authentication, or network behavior.

## Sources

### Primary (HIGH confidence)
- Codebase analysis: `crates/hephaestus-core/src/pipeline.rs` -- all clone sites identified by line number
- Codebase analysis: `crates/hephaestus-api/src/metrics.rs` -- StageTimer String clones identified
- ndarray 0.17.2 source: `impl_views/constructors.rs:47` -- `ArrayView::from_shape` API confirmed
- metrics 0.24.6 source: `cow.rs:328` -- `From<Arc<T>> for Cow` confirmed, `common.rs` SharedString = `Cow<'static, str>`

### Secondary (MEDIUM confidence)
- rules/ audit: `anti-clone-excessive.md`, `own-borrow-over-clone.md`, `anti-vec-for-slice.md`, `mem-zero-copy.md`, `own-arc-shared.md` -- established project rules guiding the optimizations

### Tertiary (LOW confidence)
- None

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- no new dependencies, all APIs verified in local source
- Architecture: HIGH -- all changes are straightforward substitutions in existing code paths
- Pitfalls: HIGH -- lifetime and ownership patterns verified against Rust compiler behavior

**Research date:** 2026-08-29
**Valid until:** indefinite (Rust stdlib and ndarray 0.17 APIs are stable)
