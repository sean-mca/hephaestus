---
phase: 12-hot-path-performance-optimization
plan: 01
subsystem: inference
tags: [ndarray, onnx, zero-copy, arrayview, tensor, performance]

requires:
  - phase: 11-websocket-streaming-asr-pipeline
    provides: ASR pipeline with PreparedAudio struct and Whisper decode loop
provides:
  - Zero-copy ArrayView2 tensor construction in run_onnx_inference
  - Removed PreparedAudio dead raw_samples field
  - Direct Vec consumption in CTC prepare path
  - Zero-copy ArrayView in Whisper decode loop
  - EmbeddingsPipeline pre-clone removal
affects: [hot-path-performance-optimization]

tech-stack:
  added: []
  patterns: [ArrayView2 borrow for ONNX tensor inputs, lifetime separation for session vs prepared data]

key-files:
  created: []
  modified:
    - crates/hephaestus-core/src/pipeline.rs

key-decisions:
  - "Separate lifetimes ('s for session, anonymous for prepared) to decouple borrow scopes"
  - "token_type_ids_array remains Array2::zeros since it has no source to borrow from"

patterns-established:
  - "Zero-copy tensor construction: use ArrayView2::from_shape to borrow existing slices instead of Array2::from_shape_vec with clone"
  - "Lifetime separation: session lifetime decoupled from input lifetime enables post-inference access to prepared fields"

requirements-completed: [XCUT-03]

coverage:
  - id: D1
    description: "Zero-copy ArrayView2 tensor construction in run_onnx_inference eliminates per-request heap clones for input_ids and attention_mask"
    requirement: "XCUT-03"
    verification:
      - kind: unit
        ref: "cargo test --workspace (49 tests)"
        status: pass
    human_judgment: false
  - id: D2
    description: "EmbeddingsPipeline::execute no longer pre-clones attention_mask before inference"
    requirement: "XCUT-03"
    verification:
      - kind: unit
        ref: "cargo test --workspace (49 tests)"
        status: pass
    human_judgment: false
  - id: D3
    description: "PreparedAudio raw_samples dead field removed, saving ~1.9MB clone per ASR window"
    requirement: "XCUT-03"
    verification:
      - kind: unit
        ref: "cargo test --workspace (49 tests) + cargo build with zero dead_code warnings"
        status: pass
    human_judgment: false
  - id: D4
    description: "CTC prepare path consumes input Vec directly without cloning"
    requirement: "XCUT-03"
    verification:
      - kind: unit
        ref: "cargo test --workspace (49 tests)"
        status: pass
    human_judgment: false
  - id: D5
    description: "Whisper decode loop uses ArrayView2::from_shape to borrow token slice instead of cloning per iteration"
    requirement: "XCUT-03"
    verification:
      - kind: unit
        ref: "cargo test --workspace (49 tests)"
        status: pass
    human_judgment: false

duration: 2min
completed: 2026-08-29
status: complete
---

# Phase 12 Plan 01: Hot-Path Pipeline Optimizations Summary

**Zero-copy ArrayView2 tensor construction in ONNX inference, dead PreparedAudio field removal, and direct Vec consumption in ASR pipeline**

## Performance

- **Duration:** 2 min
- **Started:** 2026-08-29T14:28:26Z
- **Completed:** 2026-08-29T14:30:47Z
- **Tasks:** 2
- **Files modified:** 1

## Accomplishments

- Replaced Array2::from_shape_vec clones with ArrayView2::from_shape borrows in run_onnx_inference for input_ids and attention_mask tensors
- Removed EmbeddingsPipeline attention_mask pre-clone by separating session/prepared lifetimes
- Removed dead raw_samples field from PreparedAudio, eliminating ~1.9MB clone per ASR window
- CTC prepare path now consumes input Vec directly instead of cloning
- Whisper decode loop uses ArrayView2 to borrow token slice instead of Array2 with clone per iteration

## Task Commits

Each task was committed atomically:

1. **Task 1: Zero-copy tensor construction in run_onnx_inference and EmbeddingsPipeline pre-clone removal** - `b1c6d31` (perf)
2. **Task 2: Remove PreparedAudio dead field, consume CTC input directly, zero-copy Whisper decode tokens** - `3ecd4f4` (perf)

## Files Created/Modified

- `crates/hephaestus-core/src/pipeline.rs` - Zero-copy ArrayView2 tensor construction, lifetime separation, dead field removal, direct Vec consumption

## Decisions Made

- Separated session lifetime ('s) from prepared input lifetime (anonymous) to decouple borrow scopes, enabling post-inference access to prepared fields without pre-cloning
- Kept token_type_ids_array as Array2::zeros since it is constructed fresh with no source data to borrow from

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Pipeline hot-path optimizations complete for plan 01
- Ready for plan 02 (Arc<str> metrics and remaining optimizations)
- All 49 existing tests pass unchanged

---
## Self-Check: PASSED

- pipeline.rs: FOUND
- 12-01-SUMMARY.md: FOUND
- Commit b1c6d31: FOUND
- Commit 3ecd4f4: FOUND

---
*Phase: 12-hot-path-performance-optimization*
*Completed: 2026-08-29*
