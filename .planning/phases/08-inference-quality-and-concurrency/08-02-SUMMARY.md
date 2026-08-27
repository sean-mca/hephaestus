---
phase: 08-inference-quality-and-concurrency
plan: 02
subsystem: api
tags: [rwlock, concurrency, tokio, onnx, inference]

requires:
  - phase: 02-serving-layer
    provides: AppState with Mutex-guarded PipelineKind
provides:
  - RwLock-based pipeline access enabling concurrent tokenization
  - read_pipeline() for shared prepare() access
  - write_pipeline() for exclusive execute() access
affects: [08-03-integration-tests]

tech-stack:
  added: []
  patterns: [RwLock read/write split for concurrent tokenization vs serialized inference]

key-files:
  created: []
  modified:
    - crates/hephaestus-api/src/state.rs
    - crates/hephaestus-api/src/handlers.rs
    - crates/hephaestus-api/src/batcher.rs
    - crates/hephaestus/src/main.rs

key-decisions:
  - "tokio::sync::RwLock (not parking_lot) to stay in async ecosystem with write-preferring fairness"
  - "Block-scoped guards in direct path to drop read lock before acquiring write lock"

patterns-established:
  - "RwLock read/write split: read_pipeline() for &self methods, write_pipeline() for &mut self methods"

requirements-completed: [SC-02]

coverage:
  - id: D1
    description: "Pipeline Mutex replaced with tokio RwLock, enabling concurrent tokenization across requests"
    requirement: SC-02
    verification:
      - kind: unit
        ref: "crates/hephaestus-api/src/batcher.rs#test_batcher_submit_sends_to_channel"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-api/src/batcher.rs#test_batcher_channel_is_bounded"
        status: pass
      - kind: other
        ref: "cargo build --workspace (compilation verifies RwLock API correctness)"
        status: pass
      - kind: other
        ref: "grep -r lock_pipeline crates/ returns zero matches"
        status: pass
    human_judgment: false

duration: 2min
completed: 2026-08-27
status: complete
---

# Phase 8 Plan 2: RwLock Pipeline Split Summary

**Replaced pipeline Mutex with tokio RwLock so tokenization (prepare) runs concurrently across requests while ONNX inference (execute) retains exclusive access**

## Performance

- **Duration:** 2 min
- **Started:** 2026-08-27T00:44:49Z
- **Completed:** 2026-08-27T00:47:40Z
- **Tasks:** 2
- **Files modified:** 4

## Accomplishments
- Replaced tokio::sync::Mutex with tokio::sync::RwLock in AppState for the pipeline field
- Added read_pipeline() (shared read lock for prepare/tokenization) and write_pipeline() (exclusive write lock for execute/inference)
- Migrated all 5 call sites: handlers.rs direct path (read + write), handlers.rs batching path (read), batcher.rs (write), main.rs warmup (read + write)
- All 135 workspace tests pass, zero lock_pipeline references remain

## Task Commits

Each task was committed atomically:

1. **Task 1: Replace Mutex with RwLock in AppState and add read/write accessors** - `5b655f7` (refactor)
2. **Task 2: Update all lock_pipeline call sites to use read/write split** - `3607344` (feat)

## Files Created/Modified
- `crates/hephaestus-api/src/state.rs` - RwLock field, read_pipeline(), write_pipeline() methods
- `crates/hephaestus-api/src/handlers.rs` - Direct path split into read lock (prepare) + write lock (execute); batching path uses read lock
- `crates/hephaestus-api/src/batcher.rs` - execute_batch uses write lock
- `crates/hephaestus/src/main.rs` - Warmup split into read lock (prepare) + write lock (execute)

## Decisions Made
- Used tokio::sync::RwLock (not parking_lot::RwLock) to stay within the async ecosystem; tokio RwLock is write-preferring by default, preventing writer starvation under load
- Block-scoped guards in the direct handler path ensure the read lock is dropped before the write lock is acquired, avoiding deadlock

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- RwLock split complete, concurrent tokenization enabled
- Ready for Phase 08 Plan 03 (integration tests)

## Self-Check: PASSED

---
*Phase: 08-inference-quality-and-concurrency*
*Completed: 2026-08-27*
