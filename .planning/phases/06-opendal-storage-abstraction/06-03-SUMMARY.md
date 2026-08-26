---
phase: 06-opendal-storage-abstraction
plan: 03
subsystem: storage
tags: [opendal, python, forge, storage-abstraction]

requires:
  - phase: 05-forge-conversion-service
    provides: Forge service with boto3 storage and test suite
provides:
  - OpenDAL-based storage for Forge Python service
  - Unified STORAGE_* env var config matching Hephaestus Rust side
  - Memory-backed test fixtures replacing moto S3 mocks
affects: [06-opendal-storage-abstraction]

tech-stack:
  added: [opendal (Python >=0.47)]
  patterns: [opendal.Operator("memory") for test fixtures, build_operator factory from settings]

key-files:
  created: []
  modified:
    - forge/src/forge/storage.py
    - forge/src/forge/config.py
    - forge/src/forge/queue.py
    - forge/pyproject.toml
    - forge/tests/conftest.py
    - forge/tests/test_storage.py
    - forge/tests/test_api.py
    - forge/uv.lock

key-decisions:
  - "opendal.Operator is synchronous in Python; asyncio.to_thread wrapping preserved for non-blocking upload"
  - "Operator root absorbs storage prefix so callers use model_id/filename paths only"

patterns-established:
  - "build_operator(settings) factory: constructs Operator from ForgeSettings, encapsulates backend selection"
  - "opendal.Operator('memory') for test fixtures: zero-config in-process storage for unit tests"

requirements-completed: [STOR-01]

coverage:
  - id: D1
    description: "Forge uploads converted models via opendal.Operator instead of boto3"
    requirement: "STOR-01"
    verification:
      - kind: unit
        ref: "forge/tests/test_storage.py#test_upload_to_storage_writes_files"
        status: pass
      - kind: unit
        ref: "forge/tests/test_storage.py#test_uploaded_files_are_readable"
        status: pass
    human_judgment: false
  - id: D2
    description: "Forge uses STORAGE_* env vars matching Hephaestus config"
    requirement: "STOR-01"
    verification:
      - kind: unit
        ref: "forge/tests/test_api.py#TestHealthEndpoint.test_health_returns_ok"
        status: pass
    human_judgment: false
  - id: D3
    description: "opendal replaces boto3 as a required dependency"
    verification:
      - kind: other
        ref: "grep -c opendal forge/pyproject.toml returns 1; grep -c boto3 returns 0"
        status: pass
    human_judgment: false
  - id: D4
    description: "Tests use opendal memory backend instead of moto S3 mock"
    verification:
      - kind: unit
        ref: "forge/tests/test_storage.py#test_upload_to_storage_writes_files"
        status: pass
      - kind: unit
        ref: "forge/tests/test_storage.py#test_upload_paths_contain_model_id"
        status: pass
      - kind: unit
        ref: "forge/tests/test_storage.py#test_uploaded_files_are_readable"
        status: pass
      - kind: unit
        ref: "forge/tests/test_storage.py#test_upload_includes_subdirectories"
        status: pass
    human_judgment: false

duration: 3min
completed: 2026-08-26
status: complete
---

# Phase 06 Plan 03: Forge OpenDAL Migration Summary

**Forge Python service migrated from boto3 to OpenDAL Python bindings with memory-backed test fixtures replacing moto S3 mocks**

## Performance

- **Duration:** 3 min
- **Started:** 2026-08-26T22:02:17Z
- **Completed:** 2026-08-26T22:05:17Z
- **Tasks:** 2
- **Files modified:** 8

## Accomplishments
- Replaced boto3 with opendal>=0.47 for all Forge storage operations
- Unified config to STORAGE_* env vars (storage_type, storage_bucket, storage_prefix, storage_root, storage_region) matching Hephaestus Rust config
- Rewrote 4 storage tests to use opendal.Operator("memory") instead of moto S3 mocks
- Full Forge test suite passes: 17/17 tests green

## Task Commits

Each task was committed atomically:

1. **Task 1: Update Forge storage, config, queue, and pyproject.toml** - `e62cbe9` (feat)
2. **Task 2: Update Forge test fixtures and storage tests for OpenDAL memory backend** - `ef0de62` (test)

## Files Created/Modified
- `forge/src/forge/storage.py` - OpenDAL-based build_operator() and upload_to_storage() replacing boto3 upload_to_s3()
- `forge/src/forge/config.py` - ForgeSettings with storage_type/bucket/prefix/root/region replacing s3_bucket/s3_prefix
- `forge/src/forge/queue.py` - Updated imports and _do_convert to use build_operator + upload_to_storage
- `forge/pyproject.toml` - opendal>=0.47 replaces boto3>=1.35; moto[s3] removed from dev deps
- `forge/uv.lock` - Lockfile updated for dependency changes
- `forge/tests/conftest.py` - memory_operator fixture, updated test_settings with STORAGE_* fields
- `forge/tests/test_storage.py` - 4 tests rewritten for upload_to_storage + memory operator
- `forge/tests/test_api.py` - app fixture updated with new ForgeSettings fields

## Decisions Made
- opendal.Operator is synchronous in Python; asyncio.to_thread wrapping preserved for non-blocking upload in the async queue
- Operator root absorbs the storage prefix, so upload paths are simply model_id/filename with no prefix parameter needed

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Fixed test_api.py ForgeSettings constructor**
- **Found during:** Task 2 (test suite verification)
- **Issue:** test_api.py app fixture still passed removed s3_bucket/s3_prefix fields to ForgeSettings, causing ValidationError (extra_forbidden)
- **Fix:** Updated app fixture to use storage_type="memory", storage_bucket="", storage_prefix="models" matching new config schema
- **Files modified:** forge/tests/test_api.py
- **Verification:** Full test suite passes 17/17
- **Committed in:** ef0de62 (Task 2 commit)

---

**Total deviations:** 1 auto-fixed (1 blocking)
**Impact on plan:** Fix was necessary for test suite to pass. test_api.py was not listed in the plan's files_modified but referenced removed config fields. No scope creep.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Forge Python service now uses the same OpenDAL storage abstraction as the Rust side
- Both services share STORAGE_* env var naming convention
- Ready for Phase 06 Plans 01-02 (Rust side migration) to complete the unified storage layer

## Self-Check: PASSED

All 7 modified files exist on disk. Both task commits (e62cbe9, ef0de62) verified in git log.

---
*Phase: 06-opendal-storage-abstraction*
*Completed: 2026-08-26*
