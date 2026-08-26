---
phase: "03"
plan: "02"
subsystem: model-resolution
status: complete
tags: [resolve, s3-cache, forge-stub, atomic-download, cache-back, 3-tier]
dependency_graph:
  requires: [ModelResolver, ResolveError, validate_model_id, download_from_hf, with_retry]
  provides: [S3 download, S3 upload, ForgeClient, StubForgeClient, spawn_cache_back, full 3-tier resolve]
  affects: [crates/hephaestus/src/main.rs]
tech_stack:
  added: [aws-sdk-s3, aws-config, reqwest]
  patterns: [atomic-temp-rename, fire-and-forget-upload, 3-tier-fallback, deep-module-forge-trait]
key_files:
  created:
    - crates/hephaestus-resolve/src/s3.rs
    - crates/hephaestus-resolve/src/forge.rs
  modified:
    - Cargo.toml
    - Cargo.lock
    - crates/hephaestus-resolve/Cargo.toml
    - crates/hephaestus-resolve/src/lib.rs
    - crates/hephaestus-resolve/src/resolver.rs
    - crates/hephaestus/src/main.rs
decisions:
  - "Used tempfile::TempDir::keep() instead of deprecated into_path() for atomic download pattern"
  - "Used Vec<u8> for S3 file content instead of bytes::Bytes to avoid adding direct bytes dependency"
  - "Concrete StubForgeClient field in ModelResolver instead of generic type parameter -- Phase 5 will generalize"
  - "Recursive upload_files_recursive handles onnx/ subdirectory layout for S3 cache-back"
metrics:
  duration: "11m 43s"
  completed: "2026-08-26T13:47:14Z"
  tasks: 2
  files_created: 2
  files_modified: 5
---

# Phase 03 Plan 02: S3 Cache, Forge Stub, and Full 3-Tier Resolution Summary

S3 cache tier with atomic temp-dir-then-rename downloads, background fire-and-forget cache-back uploads, and ForgeClient trait with stub implementation completing the full S3 -> HuggingFace -> Forge resolution chain.

## Task Results

| Task | Name | Type | Commit(s) | Status |
|------|------|------|-----------|--------|
| 1 | S3 download and upload operations with atomic caching | auto (tdd) | `397dcdf` (RED), `268abd8` (GREEN) | Complete |
| 2 | Forge client stub and full 3-tier orchestration | auto (tdd) | `df2953f` (RED), `293b0c6` (GREEN) | Complete |

## What Was Built

### Task 1: S3 Download/Upload with Atomic Caching (TDD)

**RED phase** (397dcdf): Created `s3.rs` with `todo!()` stubs for `download_model_from_s3` and `upload_model_to_s3`. Added `should_panic` tests verifying stubs are not implemented. Added workspace dependencies: `aws-sdk-s3`, `aws-config`, `reqwest`. Updated `hephaestus-resolve/Cargo.toml` with `aws-sdk-s3`, `aws-config`, `serde`, `serde_json`, `tempfile`.

**GREEN phase** (268abd8): Implemented full S3 operations:
- `download_model_from_s3()`: Downloads model files from S3 using `get_object`, writes to temp dir via `TempDir::new_in(parent)` (same filesystem guarantee), atomically renames to final path, calls `keep()` to prevent destructor cleanup (D-06)
- `upload_model_to_s3()`: Recursively walks model directory, uploads each file via `ByteStream::from_path` and `put_object` (D-13, unconditional -- no `head_object` check)
- `download_s3_file()`: Downloads a single S3 object, returns `Vec<u8>`, detects `NoSuchKey` for cache miss signaling
- `format_s3_key()`: Constructs S3 keys as `{prefix}/{model_id}/{filename}` preserving slashes (D-01)
- Integrated S3 as tier 1 in `ModelResolver::resolve()` with 3 retries at 500ms base delay (D-05)
- Added `spawn_cache_back()` using `tokio::spawn` for fire-and-forget background upload (D-12) with retry (3 attempts, 1s base delay) and warn-level logging on final failure (D-14)
- Made `ModelResolver::new()` async to initialize S3 client from `aws_config::load_defaults()`
- Updated `main.rs` to `.await` the now-async resolver construction

### Task 2: Forge Client Stub and Full 3-Tier Orchestration (TDD)

**RED phase** (df2953f): Created `forge.rs` with `ForgeClient` trait (single `convert()` method per Ousterhout D-10) and `StubForgeClient` implementation. Added `#[cfg_attr(test, mockall::automock)]` for test mocking. Tests verify error messages contain model ID and mention Forge configuration.

**GREEN phase** (293b0c6): Integrated Forge as tier 3 in resolver:
- Added `StubForgeClient` as concrete `forge` field in `ModelResolver`
- Updated `resolve()` with complete 3-tier chain: S3 cache miss -> HF `NoOnnxExport` -> Forge `convert()` -> download converted model from S3
- Each tier logs at info level with `tier = "s3"|"huggingface"|"forge"` field
- Added `reqwest.workspace = true` to resolve crate for Phase 5 readiness (D-08)
- Re-exported `ForgeClient` and `StubForgeClient` from `lib.rs` (public API per D-10)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Used keep() instead of deprecated into_path()**
- **Found during:** Task 1 GREEN phase
- **Issue:** `tempfile::TempDir::into_path()` is deprecated in the current version of tempfile; `keep()` is the replacement
- **Fix:** Used `let _ = temp_dir.keep()` throughout
- **Files modified:** `crates/hephaestus-resolve/src/s3.rs`
- **Commit:** `268abd8`

**2. [Rule 3 - Blocking] Used Vec<u8> instead of bytes::Bytes for S3 content**
- **Found during:** Task 1 GREEN phase
- **Issue:** `download_s3_file` initially returned `bytes::Bytes` but the `bytes` crate is not a direct dependency
- **Fix:** Changed return type to `Vec<u8>` using `.to_vec()` on the aggregated bytes, avoiding a new dependency
- **Files modified:** `crates/hephaestus-resolve/src/s3.rs`
- **Commit:** `268abd8`

## TDD Gate Compliance

### Task 1
- RED commit: `397dcdf` (test scope prefix)
- GREEN commit: `268abd8` (feat scope prefix)
- REFACTOR: Not needed

### Task 2
- RED commit: `df2953f` (test scope prefix)
- GREEN commit: `293b0c6` (feat scope prefix)
- REFACTOR: Not needed

Gate sequence verified in git log.

## Known Stubs

None. All stubs from Plan 03-01 (S3 tier placeholder) have been replaced with real implementations. The `StubForgeClient` is intentional by design -- Phase 5 provides the real HTTP implementation.

## Verification Results

- `cargo build -p hephaestus-resolve`: passes (0 errors, 0 warnings)
- `cargo test -p hephaestus-resolve`: 38/38 pass
- `cargo build --workspace`: passes
- `cargo test --workspace`: 67 pass, 0 fail, 9 ignored
- S3 download uses `TempDir::new_in()` for same-filesystem guarantee (D-06): verified
- S3 download calls `keep()` after rename (D-06): verified
- S3 upload uses `ByteStream::from_path` and `put_object` (D-13): verified
- No `head_object` call in upload (D-13 unconditional): verified
- `tokio::spawn` in `spawn_cache_back` (D-12): verified
- `ForgeClient` trait has exactly one method (`convert`) (D-10): verified
- `StubForgeClient` returns `ForgeUnavailable` (D-04): verified
- Resolver logs tier at info level: verified

## Self-Check: PASSED
