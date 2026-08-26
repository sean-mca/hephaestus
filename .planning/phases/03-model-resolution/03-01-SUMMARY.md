---
phase: "03"
plan: "01"
subsystem: model-resolution
status: complete
tags: [resolve, hf-hub, model-download, validation, deep-module]
dependency_graph:
  requires: []
  provides: [ModelResolver, ResolveError, validate_model_id, download_from_hf, with_retry]
  affects: [crates/hephaestus/src/main.rs, crates/hephaestus/src/config.rs]
tech_stack:
  added: [hf-hub (regular dep in resolve crate)]
  patterns: [deep-module-resolver, tdd-red-green, async-retry-backoff, model-id-validation]
key_files:
  created:
    - crates/hephaestus-resolve/src/error.rs
    - crates/hephaestus-resolve/src/hf.rs
    - crates/hephaestus-resolve/src/resolver.rs
  modified:
    - crates/hephaestus-resolve/Cargo.toml
    - crates/hephaestus-resolve/src/lib.rs
    - crates/hephaestus/Cargo.toml
    - crates/hephaestus/src/config.rs
    - crates/hephaestus/src/main.rs
    - Cargo.lock
decisions:
  - "Used std::env::var(HOME) instead of dirs crate for home directory to avoid adding a new workspace dependency"
  - "S3 tier logs at debug level as placeholder for Plan 03-02 -- no stub that could confuse callers"
metrics:
  duration: "9m 4s"
  completed: "2026-08-26T13:29:19Z"
  tasks: 2
  files_created: 3
  files_modified: 6
---

# Phase 03 Plan 01: Resolve Crate Foundation with HF Download Summary

Resolve crate with HuggingFace download tier, model ID validation security gate, and async retry utility -- wired into binary startup replacing the MODEL_PATH requirement with automatic MODEL_ID resolution.

## Task Results

| Task | Name | Type | Commit(s) | Status |
|------|------|------|-----------|--------|
| 1 | Resolve crate foundation with HuggingFace download tier | auto (tdd) | `67b158f` (RED), `2e68bd7` (GREEN) | Complete |
| 2 | Wire resolver into binary startup and extend config | auto | `1988ac6` | Complete |

## What Was Built

### Task 1: Resolve Crate Foundation (TDD)

**RED phase** (67b158f): Created stub files with `todo!()` implementations and 17 unit tests covering:
- `validate_model_id()`: 9 tests (path traversal, shell metacharacters, empty string, valid IDs)
- `split_model_id()`: 3 tests (org/name, no-slash, multiple slashes)
- `with_retry()`: 3 tests (retry counting, error exhaustion, first-attempt success)
- Error Display: 2 tests (NoOnnxExport and ForgeUnavailable contain model_id)

All 15 behavior tests failed (todo! panics), 2 data tests passed.

**GREEN phase** (2e68bd7): Implemented all behavior:
- `ResolveError` enum with InvalidModelId, S3, HuggingFace, NoOnnxExport, ForgeUnavailable, Io variants
- `validate_model_id()` rejects empty strings, characters outside `[a-zA-Z0-9\-_/.]`, and `..` path segments (T-03-01)
- `split_model_id()` splits on first `/`, returns `(id, id)` for no-slash case
- `download_from_hf()` uses hf-hub 1.0 API: tries `onnx/model.onnx` then `model.onnx`, returns `NoOnnxExport` on both `EntryNotFound` (D-04)
- `with_retry()` generic async exponential backoff with tracing warn logs
- `ModelResolver::new()` accepts optional s3_bucket, s3_prefix, forge_url
- `ModelResolver::resolve()` calls `validate_model_id()` first (T-03-01), then HF tier with 3 retries at 500ms base delay

All 17 tests pass.

### Task 2: Wire Resolver into Binary

- Added `hephaestus-resolve` dependency to binary crate
- Extended Config with `s3_bucket`, `s3_prefix`, `forge_url` (all `Option<String>` with `#[serde(default)]`)
- Replaced `config.model_dir()` with resolver logic: when `MODEL_PATH` is unset, uses `ModelResolver::new()` + `resolve()` 
- Preserved backward compatibility: `MODEL_PATH` still works as local override
- Added s3_bucket and forge_url to startup log for operator visibility
- Updated `config_with_model_path()` test helper with new fields

All 46 workspace tests pass (11 ignored -- require model files on disk).

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Used std::env::var("HOME") instead of dirs crate**
- **Found during:** Task 1 implementation
- **Issue:** Plan's implementation pattern used `dirs::home_dir()` but `dirs` is not a workspace dependency
- **Fix:** Used `std::env::var("HOME")` with fallback to `"."` -- avoids adding a new dependency for a single call. hf-hub handles its own cache directory resolution internally.
- **Files modified:** `crates/hephaestus-resolve/src/resolver.rs`
- **Commit:** `2e68bd7`

## Known Stubs

| File | Line | Stub | Reason |
|------|------|------|--------|
| `crates/hephaestus-resolve/src/resolver.rs` | 105 | S3 tier placeholder (debug log) | By design -- Plan 03-02 implements S3 tier |

These stubs do not prevent the plan's goal. The resolver works end-to-end for models with HuggingFace ONNX exports.

## TDD Gate Compliance

- RED commit: `67b158f` (test scope prefix)
- GREEN commit: `2e68bd7` (feat scope prefix)
- REFACTOR: Not needed -- code was clean after GREEN

Gate sequence verified in git log.

## Verification Results

- `cargo build -p hephaestus-resolve`: passes (1 expected warning: unused fields reserved for 03-02)
- `cargo test -p hephaestus-resolve`: 17/17 pass
- `cargo build -p hephaestus`: passes
- `cargo test -p hephaestus`: 6/6 pass
- `cargo build --workspace`: passes
- `cargo test --workspace`: 46 pass, 0 fail, 11 ignored

## Self-Check: PASSED

- All created files exist on disk (error.rs, hf.rs, resolver.rs)
- SUMMARY.md exists at `.planning/phases/03-model-resolution/03-01-SUMMARY.md`
- All commit hashes verified in git log: 67b158f, 2e68bd7, 1988ac6
