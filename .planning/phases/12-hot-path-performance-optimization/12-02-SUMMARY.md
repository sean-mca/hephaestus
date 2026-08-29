---
phase: 12-hot-path-performance-optimization
plan: 02
subsystem: api
tags: [arc, metrics, zero-alloc, performance, stage-timer]

requires:
  - phase: 02-observability-metrics
    provides: StageTimer deep-module metrics abstraction

provides:
  - Arc<str> model_id in StageTimer for zero-allocation metrics recording
  - impl Into<Arc<str>> constructor pattern for flexible caller ergonomics

affects: []

tech-stack:
  added: []
  patterns:
    - "Arc<str> for frequently-cloned identifier strings in hot paths"
    - "impl Into<Arc<str>> constructor pattern for zero-copy from &str callers"

key-files:
  created: []
  modified:
    - crates/hephaestus-api/src/metrics.rs
    - crates/hephaestus-api/src/handlers.rs
    - crates/hephaestus-api/src/grpc/inference.rs

key-decisions:
  - "Arc<str> over Cow<str> for model_id: atomic ref-count bump is cheaper than Cow borrow tracking for the clone-heavy metrics path"

patterns-established:
  - "Arc<str> with impl Into<Arc<str>> constructors for shared string identifiers in hot paths"

requirements-completed: [XCUT-03]

coverage:
  - id: D1
    description: "StageTimer model_id changed from String to Arc<str>, eliminating 3 heap allocations per request in metrics recording"
    requirement: "XCUT-03"
    verification:
      - kind: unit
        ref: "crates/hephaestus-api/src/metrics.rs#stage_timer_new_accepts_model_id"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-api/src/metrics.rs#stage_timer_time_returns_closure_result"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-api/src/metrics.rs#stage_timer_time_returns_result_type"
        status: pass
    human_judgment: false

duration: 1min
completed: 2026-08-29
status: complete
---

# Phase 12 Plan 02: Arc str StageTimer Summary

**Zero-allocation metrics recording via Arc<str> model_id in StageTimer, replacing 3 per-request String heap clones with atomic ref-count bumps**

## Performance

- **Duration:** 1 min
- **Started:** 2026-08-29T14:32:58Z
- **Completed:** 2026-08-29T14:34:18Z
- **Tasks:** 1
- **Files modified:** 3

## Accomplishments

- Changed StageTimer::model_id field from String to Arc<str>, converting 3 per-request clone() calls from heap allocations to atomic ref-count increments
- Updated constructor to accept impl Into<Arc<str>>, allowing &str callers to avoid intermediate String allocation
- Removed .to_string() at both HTTP handler and gRPC inference call sites, saving one String allocation per request at each entry point
- Simplified test constructors to pass &str literals directly

## Task Commits

Each task was committed atomically:

1. **Task 1: StageTimer model_id from String to Arc str with caller migration** - `e1a4869` (perf)

## Files Created/Modified

- `crates/hephaestus-api/src/metrics.rs` - StageTimer field type and constructor signature changed to Arc<str>
- `crates/hephaestus-api/src/handlers.rs` - Removed .to_string() from StageTimer::new call
- `crates/hephaestus-api/src/grpc/inference.rs` - Removed .to_string() from StageTimer::new call

## Decisions Made

- Used Arc<str> over Cow<str>: the metrics path clones model_id three times per request; Arc clone is a single atomic increment vs Cow's borrow-tracking overhead. The metrics crate's SharedString has From<Arc<str>> for zero-copy label recording.

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Phase 12 plan 02 completes the hot-path performance optimization phase
- All per-request allocation eliminiation targets addressed across both plans

---
*Phase: 12-hot-path-performance-optimization*
*Completed: 2026-08-29*
