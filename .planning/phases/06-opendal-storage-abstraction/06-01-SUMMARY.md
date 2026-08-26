---
phase: 06-opendal-storage-abstraction
plan: 01
subsystem: storage
tags: [opendal, s3, storage, onnx, model-resolution]

requires:
  - phase: 03-model-resolution
    provides: s3.rs module, ModelResolver with S3 client fields, ResolveError::S3 variant

provides:
  - OpenDAL-based storage.rs module with download_model and upload_model
  - ResolveError::Storage variant replacing ResolveError::S3
  - ModelResolver with Option<opendal::Operator> replacing S3 client/bucket/prefix fields
  - Memory backend test infrastructure for storage operations

affects: [06-02-config-wiring, 06-03-forge-migration]

tech-stack:
  added: [opendal 0.58]
  patterns: [OpenDAL Operator injection, ErrorKind::NotFound for cache miss detection, Memory backend for tests]

key-files:
  created:
    - crates/hephaestus-resolve/src/storage.rs
  modified:
    - crates/hephaestus-resolve/src/error.rs
    - crates/hephaestus-resolve/src/lib.rs
    - crates/hephaestus-resolve/src/resolver.rs
    - crates/hephaestus-resolve/Cargo.toml
    - Cargo.toml

key-decisions:
  - "Operator::new() returns Operator directly in opendal 0.58 (no .finish() method)"
  - "opendal dependency added in Task 1 (not Task 2) to enable cargo check during Task 1 verification"
  - "with_retry removed for storage operations -- RetryLayer on Operator handles retries"

patterns-established:
  - "OpenDAL Memory backend for storage unit tests -- no mocks or external services needed"
  - "ErrorKind::NotFound for cache miss detection instead of string matching on error messages"
  - "format_storage_path(model_id, filename) with no prefix param -- Operator root handles prefix"

requirements-completed: [STOR-01]

coverage:
  - id: D1
    description: "OpenDAL-based storage.rs module with download_model, upload_model, and download_file functions"
    requirement: "STOR-01"
    verification:
      - kind: unit
        ref: "crates/hephaestus-resolve/src/storage.rs#download_file_returns_none_on_miss"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-resolve/src/storage.rs#download_file_returns_bytes_on_hit"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-resolve/src/storage.rs#download_model_returns_none_on_cache_miss"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-resolve/src/storage.rs#download_model_returns_path_on_hit"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-resolve/src/storage.rs#download_model_returns_existing_local_cache"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-resolve/src/storage.rs#upload_model_writes_files"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-resolve/src/storage.rs#upload_model_handles_subdirectories"
        status: pass
    human_judgment: false
  - id: D2
    description: "ResolveError::Storage variant replaces ResolveError::S3 throughout resolve crate"
    requirement: "STOR-01"
    verification:
      - kind: unit
        ref: "grep -r 'ResolveError::S3' crates/hephaestus-resolve/src/ returns no matches"
        status: pass
    human_judgment: false
  - id: D3
    description: "ModelResolver uses Option<opendal::Operator> instead of S3 client/bucket/prefix fields"
    requirement: "STOR-01"
    verification:
      - kind: unit
        ref: "crates/hephaestus-resolve/src/resolver.rs#resolver_new_without_operator"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-resolve/src/resolver.rs#resolver_new_with_operator"
        status: pass
    human_judgment: false
  - id: D4
    description: "aws-sdk-s3 and aws-config removed from resolve crate and workspace dependencies"
    requirement: "STOR-01"
    verification:
      - kind: unit
        ref: "grep -r 'aws_sdk_s3|aws_config' crates/hephaestus-resolve/src/ returns no matches"
        status: pass
    human_judgment: false

duration: 6min
completed: 2026-08-26
status: complete
---

# Phase 06 Plan 01: Resolve Crate OpenDAL Migration Summary

**Replaced aws-sdk-s3 with OpenDAL Operator in the resolve crate -- download/upload via backend-agnostic API with Memory backend tests**

## Performance

- **Duration:** 6 min
- **Started:** 2026-08-26T21:47:53Z
- **Completed:** 2026-08-26T21:54:11Z
- **Tasks:** 2
- **Files modified:** 7 (1 created, 5 modified, 1 deleted)

## Accomplishments
- Created storage.rs with download_model, upload_model, and download_file using opendal::Operator with ErrorKind::NotFound for cache miss detection
- Migrated ModelResolver from S3 client/bucket/prefix fields to single Option<opendal::Operator>
- Removed aws-sdk-s3 and aws-config from workspace and crate dependencies
- Deleted s3.rs completely -- all functionality replaced by storage.rs
- All 41 resolve crate tests pass using OpenDAL Memory backend

## Task Commits

Each task was committed atomically:

1. **Task 1: Create storage.rs module, update error variant, update lib.rs** - `c5706cf` (feat)
2. **Task 2: Migrate resolver.rs to OpenDAL Operator, update Cargo.toml deps, delete s3.rs** - `c766489` (feat)

## Files Created/Modified
- `crates/hephaestus-resolve/src/storage.rs` - NEW: OpenDAL-based download/upload with atomic temp-dir pattern and Memory backend tests
- `crates/hephaestus-resolve/src/error.rs` - Renamed S3(String) to Storage(String) variant
- `crates/hephaestus-resolve/src/lib.rs` - Replaced mod s3 with mod storage
- `crates/hephaestus-resolve/src/resolver.rs` - Replaced S3 fields with Option<Operator>, updated all tier logic
- `crates/hephaestus-resolve/Cargo.toml` - Added opendal, removed aws-sdk-s3 and aws-config
- `Cargo.toml` - Added opendal workspace dep, removed aws-sdk-s3 and aws-config
- `crates/hephaestus-resolve/src/s3.rs` - DELETED (replaced by storage.rs)

## Decisions Made
- Operator::new() in opendal 0.58 returns Operator directly (no .finish() method) -- differs from RESEARCH.md examples which showed .finish()
- Added opendal workspace dependency in Task 1 (ahead of plan's Task 2 assignment) so Task 1 cargo check verification could succeed
- Removed with_retry wrapper for all storage operations -- OpenDAL's RetryLayer (applied at Operator construction in Plan 06-02) handles transient failures

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Added opendal dependency in Task 1 instead of Task 2**
- **Found during:** Task 1 (storage.rs creation)
- **Issue:** Task 1 acceptance criteria requires cargo check to pass, but opendal dep was planned for Task 2
- **Fix:** Added opendal to workspace Cargo.toml and crate Cargo.toml in Task 1
- **Files modified:** Cargo.toml, crates/hephaestus-resolve/Cargo.toml
- **Verification:** cargo check -p hephaestus-resolve succeeds after Task 1
- **Committed in:** c5706cf (Task 1 commit)

**2. [Rule 1 - Bug] Fixed Operator::new() API -- no .finish() in opendal 0.58**
- **Found during:** Task 2 (running cargo test)
- **Issue:** RESEARCH.md examples showed Operator::new(Memory::default()).unwrap().finish() but opendal 0.58 removed .finish() -- Operator::new() returns Operator directly
- **Fix:** Removed .finish() calls in storage.rs test helper and resolver.rs test
- **Files modified:** crates/hephaestus-resolve/src/storage.rs, crates/hephaestus-resolve/src/resolver.rs
- **Verification:** cargo test -p hephaestus-resolve passes all 41 tests
- **Committed in:** c766489 (Task 2 commit)

---

**Total deviations:** 2 auto-fixed (1 blocking, 1 bug)
**Impact on plan:** Both auto-fixes necessary for correctness. No scope creep.

## Issues Encountered
- Workspace-level cargo check fails due to hephaestus binary still referencing old S3 config fields (s3_bucket, s3_prefix) -- this is expected and will be resolved by Plan 06-02 (config wiring)

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- storage.rs module ready for config wiring (Plan 06-02 will build Operator from STORAGE_* env vars and inject into ModelResolver)
- Forge migration (Plan 06-03) depends on config changes from Plan 06-02
- hephaestus binary requires config.rs and main.rs updates (Plan 06-02) before it will compile again

## Self-Check: PASSED

---
*Phase: 06-opendal-storage-abstraction*
*Completed: 2026-08-26*
