---
phase: quick
plan: 260829-split-pipeline-module
subsystem: core
tags: [rust, refactoring, onnx, pipeline, module-structure]

requires:
  - phase: none
    provides: none
provides:
  - pipeline/ module directory with 5 submodules (classifier, embeddings, seq2seq, token_classifier, asr)
  - Shared resolve_onnx_path and build_onnx_session helpers eliminating 3 duplicated code blocks
affects: [hephaestus-core, pipeline]

tech-stack:
  added: []
  patterns: [module directory structure for large pipeline types, shared ONNX session builder pattern]

key-files:
  created:
    - crates/hephaestus-core/src/pipeline/classifier.rs
    - crates/hephaestus-core/src/pipeline/embeddings.rs
    - crates/hephaestus-core/src/pipeline/seq2seq.rs
    - crates/hephaestus-core/src/pipeline/token_classifier.rs
    - crates/hephaestus-core/src/pipeline/asr.rs
  modified:
    - crates/hephaestus-core/src/pipeline/mod.rs

key-decisions:
  - "Private helper visibility preserved -- Rust module privacy rules allow child submodules to access parent private fns via super::"
  - "pub(crate) on struct fields for PipelineKind cross-module access in execute_batch"
  - "batch_postprocess_* functions colocated with their pipeline structs as pub(super)"
  - "resolve_onnx_path accepts filename parameter for reuse across model.onnx and other ONNX file patterns"

patterns-established:
  - "Pipeline submodule pattern: each pipeline struct in its own file with batch postprocessor"
  - "Shared ONNX helpers: resolve_onnx_path + build_onnx_session for all session construction"

requirements-completed: []

coverage:
  - id: D1
    description: "Split 2184-line pipeline.rs into pipeline/ directory with mod.rs and 5 submodules"
    verification:
      - kind: other
        ref: "ls crates/hephaestus-core/src/pipeline/ shows mod.rs + 5 .rs files"
        status: pass
    human_judgment: false
  - id: D2
    description: "Extract shared resolve_onnx_path and build_onnx_session helpers replacing 3 inline duplicates"
    verification:
      - kind: other
        ref: "grep -c resolve_onnx_path shows 3 call sites (mod.rs, asr.rs new_ctc)"
        status: pass
    human_judgment: false
  - id: D3
    description: "All 189 tests pass unchanged after refactoring"
    verification:
      - kind: unit
        ref: "cargo test --workspace: 189 passed, 0 failed, 9 ignored"
        status: pass
    human_judgment: false

duration: 12min
completed: 2026-08-29
status: complete
---

# Quick Task: Split pipeline.rs Module Summary

**Split 2184-line pipeline.rs into pipeline/ module directory with 5 submodules and 2 shared ONNX helpers, eliminating 3 duplicated code blocks**

## Performance

- **Duration:** 12 min
- **Started:** 2026-08-29T17:07:37Z
- **Completed:** 2026-08-29T17:19:27Z
- **Tasks:** 7
- **Files modified:** 7

## Accomplishments
- Split monolithic pipeline.rs (2184 lines) into pipeline/ directory with mod.rs (1029 lines) and 5 submodules
- Extracted resolve_onnx_path and build_onnx_session shared helpers, replacing 3 inline duplicates (load_session_and_tokenizer, AsrPipeline::new_ctc, AsrPipeline::new_whisper)
- Removed the build_session closure from new_whisper in favor of the shared build_onnx_session
- All 189 tests pass with zero warnings, no behavior changes

## Task Commits

Each task was committed atomically:

1. **Task 1: Create pipeline/ directory, extract shared helpers** - `1caa006` (refactor)
2. **Task 2: Extract ClassifierPipeline** - `fe05ca3` (refactor)
3. **Task 3: Extract EmbeddingsPipeline** - `3f784f5` (refactor)
4. **Task 4: Extract Seq2SeqPipeline** - `d428986` (refactor)
5. **Task 5: Extract TokenClassifierPipeline** - `5ed4225` (refactor)
6. **Task 6: Extract AsrPipeline** - `8e4d0ef` (refactor)
7. **Task 7: Verification** - no commit (verification only, all 189 tests pass)

## Files Created/Modified
- `crates/hephaestus-core/src/pipeline/mod.rs` - Shared types, helpers, Pipeline trait, PipelineKind enum, tests (was pipeline.rs)
- `crates/hephaestus-core/src/pipeline/classifier.rs` - ClassifierPipeline struct, impl, batch postprocessor
- `crates/hephaestus-core/src/pipeline/embeddings.rs` - EmbeddingsPipeline struct, impl, batch postprocessor
- `crates/hephaestus-core/src/pipeline/seq2seq.rs` - Seq2SeqPipeline struct, impl, batch postprocessor
- `crates/hephaestus-core/src/pipeline/token_classifier.rs` - TokenClassifierPipeline struct, impl, batch postprocessor
- `crates/hephaestus-core/src/pipeline/asr.rs` - AsrMode enum, AsrPipeline struct, CTC/Whisper constructors and execution

## Decisions Made
- Rust module privacy allows children to access parent private fns via super:: -- no visibility changes needed for shared helpers
- Used pub(crate) for pipeline struct fields so PipelineKind::execute_batch can access session/tokenizer/id2label across module boundaries
- batch_postprocess_* functions marked pub(super) and colocated with their pipeline structs rather than remaining in mod.rs
- resolve_onnx_path takes a filename parameter ("model.onnx") for reuse across different ONNX file resolution patterns

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Removed unused postprocess import from mod.rs**
- **Found during:** Task 5 (TokenClassifierPipeline extraction)
- **Issue:** After extracting all pipeline structs, mod.rs no longer used crate::postprocess directly but still imported it, causing a compiler warning
- **Fix:** Removed the unused import
- **Files modified:** crates/hephaestus-core/src/pipeline/mod.rs
- **Verification:** cargo check --workspace produces zero warnings
- **Committed in:** 5ed4225 (Task 5 commit)

---

**Total deviations:** 1 auto-fixed (1 bug)
**Impact on plan:** Trivial cleanup of stale import. No scope creep.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Pipeline module structure is clean and modular
- Each pipeline type can now be worked on independently
- Shared helpers reduce duplication for future pipeline additions

## Self-Check: PASSED

All 6 created files verified present. All 6 commit hashes verified in git log. Old pipeline.rs confirmed deleted. 189 tests passing, 0 warnings.

---
*Plan: quick-260829-split-pipeline-module*
*Completed: 2026-08-29*
