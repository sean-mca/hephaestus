---
phase: 08-inference-quality-and-concurrency
plan: 01
subsystem: inference
tags: [onnx, ner, softmax, postprocessing, token-classifier]

requires:
  - phase: 04-pipeline-profiles
    provides: token classifier pipeline with argmax_per_token

provides:
  - softmax_argmax_per_token function for normalized NER scores
  - corrected single and batch token classifier inference paths

affects: [08-03-integration-tests]

tech-stack:
  added: []
  patterns:
    - softmax normalization before argmax for probability scores in token classifiers

key-files:
  created: []
  modified:
    - crates/hephaestus-core/src/postprocess.rs
    - crates/hephaestus-core/src/pipeline.rs

key-decisions:
  - "Retained argmax_per_token with #[allow(dead_code)] for future raw-logit use cases"

patterns-established:
  - "Per-token softmax before argmax: always use softmax_argmax_per_token for token classifier outputs"

requirements-completed: [SC-01, SC-04]

coverage:
  - id: D1
    description: "Token classifier pipeline returns softmax-normalized scores in [0,1] instead of raw logits"
    requirement: SC-01
    verification:
      - kind: unit
        ref: "crates/hephaestus-core/src/postprocess.rs#test_softmax_argmax_per_token_known_logits"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-core/src/postprocess.rs#test_softmax_argmax_per_token_single_token"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-core/src/postprocess.rs#test_softmax_argmax_per_token_scores_sum_to_one"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-core/src/postprocess.rs#test_softmax_argmax_per_token_empty_returns_error"
        status: pass
    human_judgment: false
  - id: D2
    description: "Dead code in TokenClassifierPipeline::execute removed (original_text binding and comments)"
    requirement: SC-04
    verification:
      - kind: other
        ref: "grep -n original_text pipeline.rs returns empty (verified)"
        status: pass
    human_judgment: false

duration: 2min
completed: 2026-08-27
status: complete
---

# Phase 08 Plan 01: NER Score Normalization Summary

**Per-token softmax normalization for token classifier scores, replacing raw logit output with probabilities in [0,1]**

## Performance

- **Duration:** 2 min
- **Started:** 2026-08-27T00:39:51Z
- **Completed:** 2026-08-27T00:42:15Z
- **Tasks:** 2
- **Files modified:** 2

## Accomplishments

- Added `softmax_argmax_per_token` function that composes existing `softmax` and `argmax_with_score` per token row
- Replaced both single-inference and batch-inference token classifier paths to use softmax-normalized scores
- Removed dead `original_text` binding and meandering comments from `TokenClassifierPipeline::execute`
- 4 new unit tests verify scores are probabilities in [0,1], not raw logits

## Task Commits

Each task was committed atomically:

1. **Task 1: Add softmax_argmax_per_token (TDD RED)** - `34d0c7b` (test)
2. **Task 1: Add softmax_argmax_per_token (TDD GREEN)** - `541654d` (feat)
3. **Task 2: Replace call sites and remove dead code** - `baf4aeb` (fix)

## Files Created/Modified

- `crates/hephaestus-core/src/postprocess.rs` - Added `softmax_argmax_per_token` function and 4 unit tests; `#[allow(dead_code)]` on `argmax_per_token`
- `crates/hephaestus-core/src/pipeline.rs` - Replaced `argmax_per_token` with `softmax_argmax_per_token` in both `execute()` and `batch_postprocess_token_classifier()`; removed dead `original_text` block

## Decisions Made

- Retained `argmax_per_token` with `#[allow(dead_code)]` rather than deleting it, as the plan specified keeping it for potential future raw-logit use cases

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Added #[allow(dead_code)] to argmax_per_token**
- **Found during:** Task 2 (after replacing both call sites)
- **Issue:** `cargo clippy -D warnings` failed because `argmax_per_token` became unused after both call sites were replaced with `softmax_argmax_per_token`
- **Fix:** Added `#[allow(dead_code)]` annotation with explanatory comment, per plan instruction to keep the function
- **Files modified:** `crates/hephaestus-core/src/postprocess.rs`
- **Verification:** `cargo clippy -p hephaestus-core -- -D warnings` passes clean
- **Committed in:** `baf4aeb` (Task 2 commit)

---

**Total deviations:** 1 auto-fixed (1 blocking)
**Impact on plan:** Minimal -- annotation required to satisfy clippy while retaining the function per plan instruction.

## Issues Encountered

None.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- NER scores now return proper softmax probabilities, ready for integration testing in 08-03
- Both single and batch inference paths produce consistent normalized output

---
*Phase: 08-inference-quality-and-concurrency*
*Completed: 2026-08-27*
