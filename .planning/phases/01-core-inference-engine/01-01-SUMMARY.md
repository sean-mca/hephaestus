---
phase: 01-core-inference-engine
plan: 01
subsystem: core
tags: [rust, onnx, ort, tokenizers, workspace, pipeline-trait, thiserror]

requires: []
provides:
  - 4-crate Rust workspace with central dependency pinning
  - Pipeline trait contract (prepare + execute two-step API)
  - ClassifierPipeline struct with todo!() stubs (RED state)
  - CoreError enum with thiserror derives
  - Failing integration test with real distilbert model download
affects: [01-02, 01-03, 02-http-serving, 03-model-resolution]

tech-stack:
  added: [ort 2.0.0-rc.13, ndarray 0.17, tokenizers 0.23, serde 1.0, serde_json 1.0, thiserror 2.0, anyhow 1.0, envy 0.4, tracing 0.1, hf-hub 1.0, tokio 1, mockall 0.15, tempfile 3]
  patterns: [workspace-dependency-inheritance, ousterhout-deep-module, two-step-pipeline-api]

key-files:
  created:
    - Cargo.toml
    - .gitignore
    - crates/hephaestus/Cargo.toml
    - crates/hephaestus/src/main.rs
    - crates/hephaestus-core/Cargo.toml
    - crates/hephaestus-core/src/lib.rs
    - crates/hephaestus-core/src/pipeline.rs
    - crates/hephaestus-core/src/error.rs
    - crates/hephaestus-core/tests/classifier_e2e.rs
    - crates/hephaestus-resolve/Cargo.toml
    - crates/hephaestus-resolve/src/lib.rs
    - crates/hephaestus-proto/Cargo.toml
    - crates/hephaestus-proto/src/lib.rs
  modified: []

key-decisions:
  - "PreparedInput made pub (not pub(crate)) because Pipeline trait associated type Prepared must be as visible as the trait; fields remain pub(crate) for opacity"
  - "execute() takes &mut self because ort Session::run() requires mutability (deviates from D-06 Arc suggestion)"
  - "allow(dead_code) on ClassifierPipeline and PreparedInput structs since fields are used only after Plan 02 implements todo!() stubs"

patterns-established:
  - "Workspace dependency inheritance: all versions pinned in root Cargo.toml [workspace.dependencies], crates use dep.workspace = true"
  - "Ousterhout deep module: Pipeline trait exposes exactly 2 methods (prepare + execute), hiding tokenization, inference, and post-processing"
  - "Error handling: thiserror for library crate (CoreError), anyhow reserved for binary crate"
  - "Test model: Xenova/distilbert-base-uncased-finetuned-sst-2-english downloaded via hf-hub 1.0 HFClient async API"
  - "Integration tests marked #[ignore] for expensive/network-dependent operations"

requirements-completed: [XCUT-01, XCUT-02, XCUT-03, PROF-05]

coverage:
  - id: D1
    description: "4-crate Rust workspace compiles with central dependency pinning via [workspace.dependencies]"
    requirement: "XCUT-02"
    verification:
      - kind: automated_ui
        ref: "cargo build --workspace (exits 0)"
        status: pass
    human_judgment: false
  - id: D2
    description: "Pipeline trait with exactly 2 required methods (prepare + execute) following Ousterhout deep module pattern"
    requirement: "XCUT-01"
    verification:
      - kind: other
        ref: "grep confirms 2 required trait methods in crates/hephaestus-core/src/pipeline.rs"
        status: pass
    human_judgment: false
  - id: D3
    description: "ClassifierPipeline stubbed with todo!() -- failing integration test confirms RED state"
    requirement: "PROF-05"
    verification:
      - kind: integration
        ref: "crates/hephaestus-core/tests/classifier_e2e.rs#classify_positive_sentiment"
        status: unknown
    human_judgment: true
    rationale: "Integration test is #[ignore] (requires internet + model download). RED state verified by todo!() presence; actual test run deferred to CI or manual execution."
  - id: D4
    description: "All code passes cargo clippy --workspace -- -D warnings with zero warnings"
    requirement: "XCUT-03"
    verification:
      - kind: other
        ref: "cargo clippy --workspace -- -D warnings (exits 0)"
        status: pass
    human_judgment: false

duration: 7min
completed: 2026-08-23
status: complete
---

# Phase 01 Plan 01: Workspace Scaffold and Pipeline Trait Summary

**4-crate Rust workspace with Pipeline trait (prepare + execute deep module API), CoreError types, and failing classifier integration test using distilbert-sst2 via hf-hub 1.0**

## Performance

- **Duration:** 7 min
- **Started:** 2026-08-23T17:39:07Z
- **Completed:** 2026-08-23T17:46:15Z
- **Tasks:** 2
- **Files modified:** 15

## Accomplishments
- Scaffolded 4-crate workspace (hephaestus, hephaestus-core, hephaestus-resolve, hephaestus-proto) with all 13 Phase 1 dependencies centrally pinned
- Defined Pipeline trait with 2-method Ousterhout interface and ClassifierPipeline struct with todo!() stubs (RED state)
- Created CoreError enum with 6 variants (Tokenization, Inference, ModelLoad, ModelValidation, Config, Io) via thiserror
- Wrote failing integration test that downloads real distilbert-sst2 model via hf-hub 1.0 HFClient async API

## Task Commits

Each task was committed atomically:

1. **Task 1: Scaffold 4-crate Rust workspace with central dependency pinning** - `f93d54b` (feat)
2. **Task 2: Define Pipeline trait contract, error types, and failing integration test** - `c914020` (feat)

## Files Created/Modified
- `Cargo.toml` - Virtual workspace manifest with [workspace.dependencies] pinning all Phase 1 deps
- `.gitignore` - target/, *.swp, .DS_Store
- `crates/hephaestus/Cargo.toml` - Binary crate manifest with core, anyhow, envy, serde, tracing deps
- `crates/hephaestus/src/main.rs` - Placeholder main printing "not yet implemented"
- `crates/hephaestus-core/Cargo.toml` - Core library with ort, ndarray, tokenizers, serde, thiserror deps
- `crates/hephaestus-core/src/lib.rs` - Module declarations and public API re-exports
- `crates/hephaestus-core/src/pipeline.rs` - Pipeline trait, ClassifierPipeline, ClassifierOutput, PreparedInput
- `crates/hephaestus-core/src/error.rs` - CoreError enum with thiserror derives
- `crates/hephaestus-core/tests/classifier_e2e.rs` - Integration test downloading distilbert-sst2 via HFClient
- `crates/hephaestus-resolve/Cargo.toml` - Stub crate manifest (no deps)
- `crates/hephaestus-resolve/src/lib.rs` - Doc comment placeholder
- `crates/hephaestus-proto/Cargo.toml` - Stub crate manifest (no deps)
- `crates/hephaestus-proto/src/lib.rs` - Doc comment placeholder
- `Cargo.lock` - Generated lockfile

## Decisions Made
- **PreparedInput visibility:** Made `pub` (not `pub(crate)`) because the Pipeline trait's associated type `Prepared` must be as visible as the trait itself. Fields remain `pub(crate)` so external callers cannot construct or destructure it -- the type is effectively opaque outside the crate.
- **execute() takes &mut self:** Deviates from D-06 (which suggested `Arc<Session>`). `ort::Session::run()` requires `&mut self` because ONNX Runtime internals are not thread-safe. For Phase 1 (no concurrent access), owned Session is correct. Phase 2 will need `tokio::sync::Mutex<Session>`.
- **ndarray 0.17:** Confirmed ort 2.0.0-rc.13 depends on ndarray ^0.17, not 0.16 as in CLAUDE.md tech stack.
- **mockall 0.15:** Used 0.15.0 (current) rather than 0.13 from CLAUDE.md.

## Deviations from Plan

None - plan executed exactly as written.

## Known Stubs

| File | Line | Stub | Reason |
|------|------|------|--------|
| `crates/hephaestus-core/src/pipeline.rs` | 85 | `todo!("Plan 02 implements")` | ClassifierPipeline::new() -- intentional RED state, implemented in Plan 01-02 |
| `crates/hephaestus-core/src/pipeline.rs` | 95 | `todo!("Plan 02 implements")` | Pipeline::prepare() -- intentional RED state, implemented in Plan 01-02 |
| `crates/hephaestus-core/src/pipeline.rs` | 99 | `todo!("Plan 02 implements")` | Pipeline::execute() -- intentional RED state, implemented in Plan 01-02 |
| `crates/hephaestus/src/main.rs` | 2 | placeholder println | Binary entrypoint -- implemented in Plan 01-03 |
| `crates/hephaestus-resolve/src/lib.rs` | 1 | doc comment only | Stub crate per D-02 -- implemented in Phase 3 |
| `crates/hephaestus-proto/src/lib.rs` | 1 | doc comment only | Stub crate per D-02 -- implemented in Phase 2+ |

All stubs are intentional per the plan design (walking skeleton RED state). The plan's goal is establishing contracts and a failing test, not implementation.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Workspace compiles cleanly, all trait contracts defined
- Plan 01-02 can implement ClassifierPipeline::new(), prepare(), and execute() to turn the RED test GREEN
- Plan 01-03 can implement config loading in the binary crate using envy
- All dependency versions verified against actual crates.io versions and ort compatibility

## Self-Check: PASSED

All 14 created files verified present on disk. Both task commits (f93d54b, c914020) verified in git log.

---
*Phase: 01-core-inference-engine*
*Completed: 2026-08-23*
