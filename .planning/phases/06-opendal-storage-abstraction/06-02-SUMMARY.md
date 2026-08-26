---
phase: 06-opendal-storage-abstraction
plan: 02
subsystem: storage
tags: [opendal, config, operator, env-vars, storage]

requires:
  - phase: 06-opendal-storage-abstraction
    provides: OpenDAL-based storage.rs module, ModelResolver with Option<opendal::Operator> constructor

provides:
  - Config struct with STORAGE_* env var fields (storage_type, storage_bucket, storage_prefix, storage_root, storage_region)
  - Operator construction at startup from config fields via Operator::via_iter
  - RetryLayer with max_times=3 applied to Operator
  - Storage type validation with allowlist and fs backend root check
  - Resolver wired with Option<Operator> from config-driven construction

affects: [deployment-manifests, k8s-config]

tech-stack:
  added: []
  patterns: [Operator::via_iter for dynamic backend selection, STORAGE_PREFIX to OpenDAL root mapping, config-level storage type allowlist]

key-files:
  created: []
  modified:
    - crates/hephaestus/src/config.rs
    - crates/hephaestus/src/main.rs
    - crates/hephaestus/Cargo.toml

key-decisions:
  - "STORAGE_PREFIX maps to OpenDAL root config with leading slash for cloud backends (/{prefix})"
  - "For fs backend, storage_root and storage_prefix are joined ({root}/{prefix}) to form the OpenDAL root"
  - "Config validation runs before Operator construction -- invalid storage_type or missing fs root rejected at startup"

patterns-established:
  - "Dynamic backend selection: Operator::via_iter(storage_type, HashMap) constructs any backend from env vars"
  - "STORAGE_TYPE allowlist validation at config level (T-06-05) before operator construction"
  - "Config-level validation for backend-specific requirements (fs requires STORAGE_ROOT per D-17)"

requirements-completed: [STOR-01]

coverage:
  - id: D1
    description: "Config struct with STORAGE_* fields replacing s3_bucket/s3_prefix, storage_type defaulting to s3"
    requirement: "STOR-01"
    verification:
      - kind: unit
        ref: "crates/hephaestus/src/config.rs#test_storage_type_defaults_to_s3"
        status: pass
      - kind: unit
        ref: "crates/hephaestus/src/config.rs#test_validate_rejects_invalid_storage_type"
        status: pass
      - kind: unit
        ref: "crates/hephaestus/src/config.rs#test_validate_accepts_all_storage_types"
        status: pass
      - kind: unit
        ref: "crates/hephaestus/src/config.rs#test_validate_rejects_fs_without_root"
        status: pass
      - kind: unit
        ref: "crates/hephaestus/src/config.rs#test_validate_accepts_fs_with_root"
        status: pass
    human_judgment: false
  - id: D2
    description: "Operator construction in main.rs from STORAGE_* config fields via Operator::via_iter with RetryLayer"
    requirement: "STOR-01"
    verification:
      - kind: unit
        ref: "cargo build --workspace succeeds with no errors"
        status: pass
      - kind: unit
        ref: "cargo test --workspace passes all 60+ tests"
        status: pass
      - kind: unit
        ref: "grep -r 's3_bucket|s3_prefix' crates/hephaestus/src/ returns no matches"
        status: pass
      - kind: unit
        ref: "grep -r 'aws_sdk_s3|aws_config' crates/ returns no matches"
        status: pass
    human_judgment: false
  - id: D3
    description: "Resolver wired with Option<Operator> from config-driven construction in both forge and stub branches"
    requirement: "STOR-01"
    verification:
      - kind: unit
        ref: "crates/hephaestus/src/main.rs contains operator.clone() passed to new_with_client and new_with_stub"
        status: pass
    human_judgment: false

duration: 4min
completed: 2026-08-26
status: complete
---

# Phase 06 Plan 02: Config Wiring and Operator Construction Summary

**STORAGE_* env var config with Operator::via_iter dynamic backend selection, allowlist validation, and resolver injection with RetryLayer**

## Performance

- **Duration:** 4 min
- **Started:** 2026-08-26T22:09:54Z
- **Completed:** 2026-08-26T22:13:57Z
- **Tasks:** 2
- **Files modified:** 4 (config.rs, main.rs, Cargo.toml, Cargo.lock)

## Accomplishments
- Replaced s3_bucket/s3_prefix config fields with storage_type, storage_bucket, storage_prefix, storage_root, storage_region
- Added storage_type allowlist validation (T-06-05) and fs backend STORAGE_ROOT requirement (D-17)
- Built Operator at startup via Operator::via_iter with RetryLayer (max_times=3)
- Connected config-driven Operator to ModelResolver in both forge and stub branches
- All 60+ workspace tests pass, no s3_bucket/s3_prefix or aws-sdk references remain

## Task Commits

Each task was committed atomically:

1. **Task 1: Update Config struct with STORAGE_* fields and validation** - `e03c6e9` (feat)
2. **Task 2: Wire Operator construction in main.rs, update Cargo.toml** - `b7cdffb` (feat)

## Files Created/Modified
- `crates/hephaestus/src/config.rs` - Replaced s3_bucket/s3_prefix with 5 STORAGE_* fields, added validation, added 5 new tests
- `crates/hephaestus/src/main.rs` - Operator construction from config, updated resolver wiring, updated startup log
- `crates/hephaestus/Cargo.toml` - Added opendal.workspace = true
- `Cargo.lock` - Updated with opendal dependency for hephaestus binary crate

## Decisions Made
- STORAGE_PREFIX maps to OpenDAL root config with leading slash for cloud backends (`/{prefix}`), joined with STORAGE_ROOT for fs backend (`{root}/{prefix}`)
- Config validation runs before Operator construction -- startup fails fast on invalid storage_type or missing fs root
- For cloud backends (s3/gcs/azblob), STORAGE_ROOT is ignored -- only STORAGE_PREFIX drives the OpenDAL root

## Deviations from Plan

None -- plan executed exactly as written.

## Issues Encountered
None.

## User Setup Required
None -- no external service configuration required. K8s manifests need to be updated to use STORAGE_* env vars instead of S3_BUCKET/S3_PREFIX (D-03 clean break).

## Next Phase Readiness
- Full Rust workspace compiles and all tests pass with OpenDAL-based storage
- Phase 06 Rust migration complete (Plans 01 + 02)
- Forge Python migration (Plan 03) already complete
- No s3_bucket, s3_prefix, aws_sdk_s3, or aws_config references remain in the codebase

## Self-Check: PASSED

---
*Phase: 06-opendal-storage-abstraction*
*Completed: 2026-08-26*
