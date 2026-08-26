---
phase: 04-additional-profiles-and-dynamic-batching
plan: 04
subsystem: inference
tags: [onnx, validation, postprocessing, config, robustness]

requires:
  - phase: 04-additional-profiles-and-dynamic-batching
    provides: pipeline profiles, dynamic batching, postprocess utilities

provides:
  - Config::validate() method rejecting invalid batch configuration
  - Result-returning softmax and argmax_with_score (no panic on empty input)
  - Correct arithmetic mean for merged NER entity scores
  - Contiguous id2label key validation
  - Checked u32 casts in seq2seq decode paths
  - Guarded outputs[0] access across all pipeline paths

affects: [phase-05-forge-service]

tech-stack:
  added: []
  patterns:
    - "check_outputs_nonempty() guard before SessionOutputs indexing"
    - "Running sum/count pattern for entity score averaging"
    - "Config::validate() called between from_env() and resource allocation"

key-files:
  created: []
  modified:
    - crates/hephaestus/src/config.rs
    - crates/hephaestus/src/main.rs
    - crates/hephaestus-core/src/postprocess.rs
    - crates/hephaestus-core/src/pipeline.rs
    - crates/hephaestus-api/src/batcher.rs

key-decisions:
  - "Used check_outputs_nonempty() inline guard instead of first_output() helper to avoid lifetime complexity with SessionOutputs"
  - "WR-04 deferred: install_recorder() anyhow::Error in metrics crate boundary is out of scope for this gap closure"

patterns-established:
  - "Result over panic for postprocess functions per err-result-over-panic.md"
  - "Validate config before resource allocation in main startup sequence"

requirements-completed: [PROF-04, BTCH-03]

coverage:
  - id: D1
    description: "Config::validate() rejects batch_max_size outside [1, 64] and batch_max_wait_ms exceeding timeout"
    requirement: BTCH-03
    verification:
      - kind: unit
        ref: "crates/hephaestus/src/config.rs#test_validate_rejects_zero_batch_size"
        status: pass
      - kind: unit
        ref: "crates/hephaestus/src/config.rs#test_validate_rejects_large_batch_size"
        status: pass
      - kind: unit
        ref: "crates/hephaestus/src/config.rs#test_validate_accepts_valid_batch_size"
        status: pass
      - kind: unit
        ref: "crates/hephaestus/src/config.rs#test_validate_skips_when_batching_disabled"
        status: pass
      - kind: unit
        ref: "crates/hephaestus/src/config.rs#test_validate_rejects_wait_exceeding_timeout"
        status: pass
    human_judgment: false
  - id: D2
    description: "softmax and argmax_with_score return Result::Err on empty input instead of panicking"
    verification:
      - kind: unit
        ref: "crates/hephaestus-core/src/postprocess.rs#test_softmax_empty_returns_error"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-core/src/postprocess.rs#test_argmax_empty_returns_error"
        status: pass
    human_judgment: false
  - id: D3
    description: "merge_subword_entities produces correct arithmetic mean across N merged tokens"
    verification:
      - kind: unit
        ref: "crates/hephaestus-core/src/postprocess.rs#test_merge_running_average"
        status: pass
    human_judgment: false
  - id: D4
    description: "extract_id2label rejects non-contiguous keys with ModelValidation error"
    verification:
      - kind: unit
        ref: "crates/hephaestus-core/src/pipeline.rs#test_id2label_rejects_noncontiguous"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-core/src/pipeline.rs#test_id2label_accepts_contiguous"
        status: pass
    human_judgment: false
  - id: D5
    description: "Batch classifier returns CoreError::Inference on out-of-range label index (not empty string)"
    verification: []
    human_judgment: true
    rationale: "No unit test possible without a real ONNX model producing out-of-range argmax; verified by code inspection"
  - id: D6
    description: "Seq2seq decode uses checked u32 conversion (try_from + range check) instead of bare as u32"
    verification: []
    human_judgment: true
    rationale: "No unit test possible without a real ONNX model; verified by code inspection that no bare as u32 casts remain"
  - id: D7
    description: "All outputs[0] accesses guarded by check_outputs_nonempty() in all pipeline paths"
    verification: []
    human_judgment: true
    rationale: "Verified by grep: all 10 outputs[0] sites preceded by check_outputs_nonempty() call"

duration: 19min
completed: 2026-08-26
status: complete
---

# Phase 04 Plan 04: Gap Closure Summary

**Config validation, Result-returning postprocess functions, running average fix, contiguous id2label check, checked seq2seq casts, and guarded output tensor access across all pipeline paths**

## Performance

- **Duration:** 19 min
- **Started:** 2026-08-26T16:38:21Z
- **Completed:** 2026-08-26T16:57:40Z
- **Tasks:** 3
- **Files modified:** 5

## Accomplishments
- Config::validate() rejects invalid batch configuration before resource allocation (CR-01)
- softmax and argmax_with_score return Result on empty input per err-result-over-panic.md (WR-01)
- merge_subword_entities uses running sum/count for correct arithmetic mean (CR-03)
- extract_id2label validates contiguous keys 0..N, rejecting gaps (CR-02)
- Batch classifier returns error on out-of-range label index instead of empty string (CR-04)
- Seq2seq decode uses checked u32 conversion instead of wrapping casts (WR-02)
- All 10 outputs[0] sites guarded by check_outputs_nonempty() (WR-05)

## Task Commits

Each task was committed atomically:

1. **Task 1: Config validation and postprocess robustness** (TDD)
   - `37b96d2` (test: add failing tests for config validation and postprocess robustness)
   - `fb9358f` (feat: implement config validation, Result returns, and running average fix)
2. **Task 2: Pipeline correctness and defensive output access** - `9f72764` (fix)
3. **Task 3: Full workspace validation** - `b79a62f` (chore: fix clippy let_unit_value)

## Files Created/Modified
- `crates/hephaestus/src/config.rs` - Added Config::validate() with batch range and timeout checks
- `crates/hephaestus/src/main.rs` - Call validate() after from_env(), renamed _receiver to receiver
- `crates/hephaestus-core/src/postprocess.rs` - Result returns for softmax/argmax, running average fix
- `crates/hephaestus-core/src/pipeline.rs` - Contiguous id2label check, checked casts, guarded outputs
- `crates/hephaestus-api/src/batcher.rs` - Fixed clippy let_unit_value in test

## Decisions Made
- Used check_outputs_nonempty() inline guard instead of first_output() helper to avoid lifetime complexity with ort SessionOutputs borrowed references
- WR-04 deferred per plan: install_recorder() returning anyhow::Error from library crate is out of scope for this gap closure

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Fixed clippy let_unit_value in batcher.rs test**
- **Found during:** Task 3 (full workspace validation with --all-targets)
- **Issue:** `let _ = handle.await.expect(...)` triggers clippy::let_unit_value
- **Fix:** Removed unnecessary `let _ =` binding
- **Files modified:** crates/hephaestus-api/src/batcher.rs
- **Verification:** cargo clippy --workspace --all-targets -- -D warnings passes clean
- **Committed in:** b79a62f

---

**Total deviations:** 1 auto-fixed (1 blocking)
**Impact on plan:** Anticipated by plan; no scope creep.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- All verification blockers (CR-01, CR-02, CR-03) and code review warnings (CR-04, WR-01, WR-02, WR-05) from Phase 04 are resolved
- Phase 04 can now be marked as verified
- Phase 05 (Forge service) can proceed when ready

---
## Self-Check: PASSED

All 5 modified files exist on disk. All 4 task commits verified in git log.

---
*Phase: 04-additional-profiles-and-dynamic-batching*
*Completed: 2026-08-26*
