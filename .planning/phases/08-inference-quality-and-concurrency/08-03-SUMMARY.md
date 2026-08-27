---
phase: 08-inference-quality-and-concurrency
plan: 03
subsystem: testing
tags: [integration-tests, onnx, hf-hub, feature-flag, ner, classifier, embeddings]

requires:
  - phase: 08-01
    provides: softmax_argmax_per_token fix for correct NER entity scores
provides:
  - Feature-gated integration test suite for classifier, embeddings, and token_classifier profiles
  - Validation that NER entity scores are in [0.0, 1.0] after softmax fix
affects: []

tech-stack:
  added: []
  patterns: [feature-gated integration tests, shared model download helper]

key-files:
  created:
    - crates/hephaestus-core/tests/integration.rs
  modified:
    - crates/hephaestus-core/Cargo.toml

key-decisions:
  - "Seq2seq integration test excluded due to lack of reliable fused ONNX model in Xenova namespace"
  - "Empty feature flag (no dependencies) since hf-hub and tokio already in dev-dependencies"

patterns-established:
  - "Feature-gated integration tests: use cfg(feature = 'integration') to gate tests requiring network/model downloads"
  - "Shared download_model helper: reusable async function for HF model download across test functions"

requirements-completed: [SC-03]

coverage:
  - id: D1
    description: "Integration feature flag in Cargo.toml that gates test compilation"
    requirement: SC-03
    verification:
      - kind: integration
        ref: "cargo test -p hephaestus-core (without --features integration) compiles 0 integration tests"
        status: pass
    human_judgment: false
  - id: D2
    description: "Classifier integration tests (positive and negative sentiment) with score range assertions"
    requirement: SC-03
    verification:
      - kind: integration
        ref: "crates/hephaestus-core/tests/integration.rs#test_classifier_positive_sentiment"
        status: pass
      - kind: integration
        ref: "crates/hephaestus-core/tests/integration.rs#test_classifier_negative_sentiment"
        status: pass
    human_judgment: false
  - id: D3
    description: "Embeddings integration test asserting 768-dim output, L2 norm ~1.0, all finite values"
    requirement: SC-03
    verification:
      - kind: integration
        ref: "crates/hephaestus-core/tests/integration.rs#test_embeddings_dimension_and_norm"
        status: pass
    human_judgment: false
  - id: D4
    description: "Token classifier NER test asserting scores in [0,1], PER/ORG labels, valid spans"
    requirement: SC-03
    verification:
      - kind: integration
        ref: "crates/hephaestus-core/tests/integration.rs#test_token_classifier_ner_entities"
        status: pass
    human_judgment: false

duration: 2min
completed: 2026-08-27
status: complete
---

# Phase 08 Plan 03: Integration Test Suite Summary

**Feature-gated integration tests downloading real HuggingFace models to verify classifier, embeddings, and NER inference end-to-end**

## Performance

- **Duration:** 2 min
- **Started:** 2026-08-27T00:51:11Z
- **Completed:** 2026-08-27T00:53:19Z
- **Tasks:** 2
- **Files modified:** 2

## Accomplishments
- Added `integration` feature flag to hephaestus-core Cargo.toml gating test compilation
- Created 4 integration tests covering classifier (positive/negative), embeddings (768-dim, unit norm), and token classifier (NER entities with scores in [0,1])
- Validated that Plan 08-01 softmax fix produces correct NER entity scores (all in [0.0, 1.0])
- Tests download real models from HuggingFace using the same HFClient pattern as existing classifier_e2e.rs

## Task Commits

Each task was committed atomically:

1. **Task 1: Add integration feature flag** - `71e4c3c` (chore)
2. **Task 2: Create integration test file** - `64ddfd0` (test)

## Files Created/Modified
- `crates/hephaestus-core/Cargo.toml` - Added `integration = []` feature flag
- `crates/hephaestus-core/tests/integration.rs` - 4 integration tests for classifier, embeddings, and NER profiles

## Decisions Made
- Excluded seq2seq integration test: no reliable fused ONNX model available in Xenova namespace with compatible output format
- Used empty feature flag since hf-hub and tokio are already in dev-dependencies

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Fixed double-reference in filename parameter**
- **Found during:** Task 2 (integration test file)
- **Issue:** Iterating over `&[&str]` yields `&&str`, but `HFRepositoryDownloadFileBuilder::filename()` expects `impl Into<String>` which requires `&str`
- **Fix:** Added dereference `*filename` in the download helper loop
- **Files modified:** `crates/hephaestus-core/tests/integration.rs`
- **Verification:** Compilation succeeds, all 4 tests pass
- **Committed in:** 64ddfd0 (Task 2 commit)

---

**Total deviations:** 1 auto-fixed (1 blocking)
**Impact on plan:** Minor type fix required for compilation. No scope change.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Phase 08 complete: all 3 plans executed (NER score fix, RwLock split, integration tests)
- Integration test suite provides regression safety for all future pipeline changes
- Tests run via `cargo test -p hephaestus-core --features integration`

---
*Phase: 08-inference-quality-and-concurrency*
*Completed: 2026-08-27*
