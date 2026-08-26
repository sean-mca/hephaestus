---
status: complete
quick_id: 260826-ren
date: "2026-08-26"
duration: 5m
tasks_completed: 2
tasks_total: 2
---

# Quick Task 260826-ren: Wire EXECUTION_PROVIDER config to ort session builder

## What Changed

### Task 1: ExecutionProvider enum and cargo features
- Created `crates/hephaestus-core/src/ep.rs` with `ExecutionProvider` enum (Cpu, Cuda, TensorRt, CoreMl)
- Implemented `FromStr` (case-insensitive), `Display`, and `to_ort_providers()` with `cfg` feature gates
- Added cargo features `cuda`, `tensorrt`, `coreml` to `hephaestus-core` and `hephaestus` crates
- Feature chain: `hephaestus/cuda` -> `hephaestus-core/cuda` -> `ort/cuda`

### Task 2: Wire EP through pipeline and config
- Updated `load_session_and_tokenizer` to accept `&ExecutionProvider` and call `.with_execution_providers()`
- Updated all 4 pipeline constructors to accept and forward the EP parameter
- Added `parsed_execution_provider()` to `Config` with validation at startup
- Threaded EP from `main.rs` config into pipeline construction

## Commits
- `ba2a5b3` feat(quick-01): add ExecutionProvider enum and cargo feature gates
- `549ecfe` feat(quick-01): wire ExecutionProvider through session builder and config

## Verification
- `cargo test --workspace` passes (all existing tests + new EP tests)
- `cargo check --workspace` compiles on default features (CPU only)
- Missing feature produces clear error message at startup
