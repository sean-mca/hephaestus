---
phase: 10-grpc-inference-api
plan: 01
subsystem: api
tags: [grpc, tonic, prost, protobuf, codegen]

requires:
  - phase: none
    provides: standalone crate, no prior phase dependency
provides:
  - hephaestus-proto crate with InferRequest, InferResponse, InferenceService trait
  - FILE_DESCRIPTOR_SET constant for gRPC server reflection
  - workspace-level tonic/prost dependency declarations
affects: [10-02-grpc-server-integration]

tech-stack:
  added: [tonic 0.14, tonic-prost 0.14, tonic-prost-build 0.14, prost 0.14, prost-types 0.14, tonic-health 0.14, tonic-reflection 0.14]
  patterns: [tonic-prost-build codegen in build.rs, include_proto! for generated types, include_file_descriptor_set! for reflection]

key-files:
  created:
    - crates/hephaestus-proto/proto/hephaestus/v1/inference.proto
    - crates/hephaestus-proto/build.rs
  modified:
    - Cargo.toml
    - crates/hephaestus-proto/Cargo.toml
    - crates/hephaestus-proto/src/lib.rs

key-decisions:
  - "InferResponse uses opaque bytes result_json instead of per-profile proto types -- new model profiles never require proto changes"
  - "tonic-prost runtime dependency required for generated ProstCodec references"

patterns-established:
  - "Proto codegen: tonic-prost-build in build.rs with file_descriptor_set_path for reflection support"
  - "Generic gRPC response: opaque JSON bytes in result_json field mirrors REST API payload"

requirements-completed: [SC-05]

coverage:
  - id: D1
    description: "Proto file compiles to Rust types via tonic-prost-build codegen"
    requirement: "SC-05"
    verification:
      - kind: unit
        ref: "cargo check -p hephaestus-proto"
        status: pass
    human_judgment: false
  - id: D2
    description: "InferRequest and InferResponse roundtrip serialization"
    requirement: "SC-05"
    verification:
      - kind: unit
        ref: "crates/hephaestus-proto/src/lib.rs#infer_request_roundtrip"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-proto/src/lib.rs#infer_response_roundtrip"
        status: pass
    human_judgment: false
  - id: D3
    description: "FILE_DESCRIPTOR_SET available as public constant for reflection"
    verification:
      - kind: unit
        ref: "crates/hephaestus-proto/src/lib.rs#file_descriptor_set_is_nonempty"
        status: pass
    human_judgment: false

duration: 2min
completed: 2026-08-28
status: complete
---

# Phase 10 Plan 01: Proto Crate Summary

**gRPC proto definitions with tonic-prost-build codegen exporting InferenceService, InferRequest, and generic InferResponse (opaque JSON bytes)**

## Performance

- **Duration:** 2 min
- **Started:** 2026-08-28T23:23:56Z
- **Completed:** 2026-08-28T23:26:52Z
- **Tasks:** 2
- **Files modified:** 5

## Accomplishments

- Proto file with InferenceService unary Infer RPC, InferRequest, and InferResponse (result_json bytes)
- tonic-prost-build codegen in build.rs with FILE_DESCRIPTOR_SET for reflection
- lib.rs exporting v1 module and descriptor constant with 3 passing unit tests
- Workspace-level deps for tonic, tonic-health, tonic-reflection, prost, prost-types

## Task Commits

Each task was committed atomically:

1. **Task 1: Proto file, build.rs codegen, and workspace dependencies** - `e85250b` (feat)
2. **Task 2: lib.rs with generated types and unit tests** - `be9acca` (feat)

## Files Created/Modified

- `Cargo.toml` - Added tonic/prost workspace dependencies
- `crates/hephaestus-proto/Cargo.toml` - Dependencies on tonic, tonic-prost, prost; build-dep on tonic-prost-build
- `crates/hephaestus-proto/proto/hephaestus/v1/inference.proto` - gRPC service and message definitions
- `crates/hephaestus-proto/build.rs` - tonic-prost-build codegen with file descriptor set output
- `crates/hephaestus-proto/src/lib.rs` - Generated type exports, FILE_DESCRIPTOR_SET, unit tests

## Decisions Made

- InferResponse uses opaque `bytes result_json` instead of per-profile message types -- new model profiles never require proto changes, matching the REST API's model-determined JSON output pattern
- Added `tonic-prost` runtime dependency -- tonic-prost-build generates code referencing `tonic_prost::ProstCodec` which must be available at compile time

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Added tonic-prost runtime dependency**
- **Found during:** Task 2 (lib.rs with generated types)
- **Issue:** tonic-prost-build generates code referencing `tonic_prost::ProstCodec` which requires the tonic-prost crate as a runtime dependency, not just a build dependency
- **Fix:** Added `tonic-prost = "0.14"` to `[dependencies]` in hephaestus-proto/Cargo.toml
- **Files modified:** crates/hephaestus-proto/Cargo.toml
- **Verification:** `cargo check -p hephaestus-proto` compiles without errors
- **Committed in:** be9acca (Task 2 commit)

---

**Total deviations:** 1 auto-fixed (1 blocking)
**Impact on plan:** Auto-fix necessary for compilation. No scope creep.

## Issues Encountered

None beyond the auto-fixed blocking issue above.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- hephaestus-proto crate ready for Plan 10-02 to implement InferenceService trait
- FILE_DESCRIPTOR_SET available for tonic-reflection server in Plan 10-02
- All workspace deps declared for tonic-health and tonic-reflection integration

## Self-Check: PASSED

All created files verified on disk. All commit hashes found in git log.

---
*Phase: 10-grpc-inference-api*
*Completed: 2026-08-28*
