---
phase: 12-hot-path-performance-optimization
verified: 2026-08-29T15:10:00Z
status: passed
score: 9/9 must-haves verified
behavior_unverified: 0
overrides_applied: 0
---

# Phase 12: Hot-Path Performance Optimization Verification Report

**Phase Goal:** Eliminate unnecessary heap allocations and data copying on inference hot paths identified by rules/ audit: remove per-request Vec clones in ONNX tensor construction (pipeline.rs), remove dead raw_samples field and its 1.9MB audio buffer clone (pipeline.rs), and replace String cloning with Arc<str> in metrics recording (metrics.rs)
**Verified:** 2026-08-29T15:10:00Z
**Status:** passed
**Re-verification:** No -- initial verification

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | ONNX tensor construction in run_onnx_inference borrows PreparedInput vecs via ArrayView instead of cloning them | VERIFIED | pipeline.rs:367-372 uses `ndarray::ArrayView2::from_shape` for input_ids_view and attention_mask_view; no `Array2::from_shape_vec` or `.clone()` on input data |
| 2 | EmbeddingsPipeline::execute no longer pre-clones attention_mask | VERIFIED | pipeline.rs:532-555 passes `&prepared.attention_mask` directly to `mean_pool` after inference; no `let attention_mask = prepared.attention_mask.clone()` line present; lifetime separation `<'s>` on run_onnx_inference decouples session and prepared borrows |
| 3 | PreparedAudio has no raw_samples field | VERIFIED | pipeline.rs:69-72 struct has only `features: Array2<f32>`; no `raw_samples`, no `#[allow(dead_code)]` attribute; `grep -n raw_samples pipeline.rs` returns zero matches |
| 4 | ASR CTC prepare path consumes audio Vec directly without cloning | VERIFIED | pipeline.rs:1171 uses `Array2::from_shape_vec((1, num_samples), input)` consuming `input` by value; no `.clone()` on input; PreparedAudio constructed with only `features` field |
| 5 | Whisper decode loop constructs token tensors from borrowed ArrayView instead of cloning per iteration | VERIFIED | pipeline.rs:1272-1274 uses `ndarray::ArrayView2::from_shape((1, seq_len), &tokens)` borrowing `&tokens`; no `tokens.clone()` in the loop; TensorRef::from_array_view(token_view) without extra `.view()` |
| 6 | StageTimer::model_id is Arc<str>; metrics recording does ref-count bumps instead of heap allocs | VERIFIED | metrics.rs:41 field is `model_id: Arc<str>`; lines 64, 79, 85 call `self.model_id.clone()` which is an atomic ref-count increment on Arc<str> |
| 7 | Callers pass &str directly to StageTimer::new without intermediate String allocation | VERIFIED | handlers.rs:59 `StageTimer::new(state.model_id())` -- no `.to_string()`; grpc/inference.rs:57 `StageTimer::new(self.state.model_id())` -- no `.to_string()` |
| 8 | All existing tests pass (cargo test --workspace) | VERIFIED | 49 passed, 0 failed, 0 ignored across all crates |
| 9 | No public API surface changes | VERIFIED | `run_onnx_inference` is private (not `pub`); `StageTimer::new(impl Into<Arc<str>>)` is backward-compatible (String still accepted); `PreparedAudio::new_for_test` parameter change is test-only and all call sites updated |

**Score:** 9/9 truths verified (0 present, behavior-unverified)

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/hephaestus-core/src/pipeline.rs` | Zero-copy tensor construction, dead field removal | VERIFIED | ArrayView2 in run_onnx_inference (L367-372), no raw_samples in PreparedAudio (L69-72), direct Vec consumption in CTC (L1171), ArrayView in Whisper decode (L1272-1274) |
| `crates/hephaestus-api/src/metrics.rs` | Arc<str> model_id | VERIFIED | StageTimer field is Arc<str> (L41), constructor accepts impl Into<Arc<str>> (L46), tests pass with &str literals (L105, 112, 124) |
| `crates/hephaestus-api/src/handlers.rs` | Caller passes &str directly | VERIFIED | StageTimer::new(state.model_id()) at L59 -- no .to_string() |
| `crates/hephaestus-api/src/grpc/inference.rs` | Caller passes &str directly | VERIFIED | StageTimer::new(self.state.model_id()) at L57 -- no .to_string() |

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| run_onnx_inference lifetime separation | EmbeddingsPipeline pre-clone removal | `<'s>` on session, anonymous on prepared | VERIFIED | pipeline.rs:360 `fn run_onnx_inference<'s>(session: &'s mut Session, prepared: &PreparedInput)` enables pipeline.rs:555 `&prepared.attention_mask` post-inference |
| PreparedAudio field removal | new_for_test and constructor sites | Single-field struct | VERIFIED | pipeline.rs:79 takes one parameter; pipeline.rs:2108 calls with one arg; pipeline.rs:1165,1173 construct with only `features` |
| StageTimer::new(impl Into<Arc<str>>) | handlers.rs and grpc/inference.rs callers | &str -> Arc<str> via Into | VERIFIED | Both callers pass `model_id()` (&str) directly; `From<&str> for Arc<str>` handles conversion |
| Arc<str> clone in metrics macros | metrics crate SharedString | From<Arc<str>> impl | VERIFIED | metrics.rs:64,79,85 pass `self.model_id.clone()` (Arc<str>) to metrics macros; metrics crate accepts Arc<str> via SharedString::from |

### Data-Flow Trace (Level 4)

Not applicable -- this phase modifies internal memory management (tensor construction, field removal, type changes). No new data rendering or dynamic content introduced.

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| All workspace tests pass | `cargo test --workspace` | 49 passed, 0 failed | PASS |
| Workspace compiles without dead_code warnings on PreparedAudio | `cargo build --workspace` (implicit in test) | Clean compile | PASS |

### Probe Execution

Step 7c: SKIPPED (no probes declared in PLAN/SUMMARY, no conventional probe scripts found)

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|-----------|-------------|--------|----------|
| XCUT-03 | 12-01-PLAN, 12-02-PLAN | All code adheres to rules in rules/ directory | SATISFIED | Phase eliminates violations identified by rules/ audit: anti-clone-excessive (ArrayView replaces clones), own-borrow-over-clone (Arc<str> replaces String clones), mem-zero-copy (dead field removed). All 49 tests pass. |

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| crates/hephaestus-api/tests/metrics.rs | 21 | `StageTimer::new("test-model".to_string())` -- unnecessary String allocation in integration test | INFO | Test-only; backward-compatible via `From<String> for Arc<str>`. Not a production code path. No functional impact. |

### Human Verification Required

None. All changes are compile-time verifiable via Rust's type system and borrow checker. The test suite (49 tests) validates behavioral correctness. No visual, real-time, or external service components involved.

### Gaps Summary

No gaps found. All 9 must-haves verified against the codebase. The phase goal -- eliminating unnecessary heap allocations on inference hot paths -- is achieved:
- Per-request Vec clones in ONNX tensor construction replaced with ArrayView borrows
- Dead raw_samples field and its ~1.9MB audio buffer clone removed
- String cloning in metrics recording replaced with Arc<str> ref-count bumps

All 3 commits verified: b1c6d31 (zero-copy tensors), 3ecd4f4 (dead field + CTC/Whisper), e1a4869 (Arc<str> StageTimer).

---

_Verified: 2026-08-29T15:10:00Z_
_Verifier: Claude (gsd-verifier)_
