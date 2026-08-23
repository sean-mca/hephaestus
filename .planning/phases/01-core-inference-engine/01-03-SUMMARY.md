---
phase: 01-core-inference-engine
plan: 03
subsystem: core
tags: [rust, envy, config, tracing-subscriber, startup, warmup, binary]

requires:
  - phase: 01-core-inference-engine plan 02
    provides: "Working ClassifierPipeline with new(), prepare(), execute() and passing integration test"
provides:
  - Config struct with envy deserialization (MODEL_ID required, optional MODEL_PATH, EXECUTION_PROVIDER, LOG_LEVEL, WARMUP_INPUT)
  - Config::from_env() for typed environment variable loading
  - Config::model_dir() with path validation (absolute, no traversal -- T-01-01)
  - Binary startup sequence: config -> path validation -> pipeline construction -> warmup -> ready
  - Structured JSON logging via tracing-subscriber with LOG_LEVEL/RUST_LOG support
affects: [02-http-serving, 03-model-resolution]

tech-stack:
  added: [tracing-subscriber 0.3 with json+env-filter features]
  patterns: [envy-config-from-env, path-validation-traversal-guard, log-level-env-filter-fallback, warmup-inference-pass]

key-files:
  created:
    - crates/hephaestus/src/config.rs
  modified:
    - Cargo.toml
    - crates/hephaestus/Cargo.toml
    - crates/hephaestus/src/main.rs

key-decisions:
  - "LOG_LEVEL uses EnvFilter fallback: RUST_LOG takes precedence via try_from_default_env, LOG_LEVEL is used only when RUST_LOG is unset"
  - "Config loaded before tracing init so log_level is available for EnvFilter construction"
  - "model_dir() validates existence (is_dir check) in addition to path safety checks"

patterns-established:
  - "envy config pattern: Config struct with Deserialize, from_env() wrapping envy::from_env with anyhow context"
  - "Path validation: absolute check + Component::ParentDir rejection + existence check (T-01-01 mitigation)"
  - "Warmup inference: prepare + execute with default text, logging label and score at info level"
  - "Structured JSON logging: tracing_subscriber::fmt().json() with EnvFilter for LOG_LEVEL/RUST_LOG"

requirements-completed: [CORE-02, CORE-03, XCUT-03]

coverage:
  - id: D1
    description: "Config loads from env vars: MODEL_ID required, others optional with defaults (CORE-02)"
    requirement: "CORE-02"
    verification:
      - kind: unit
        ref: "crates/hephaestus/src/config.rs#from_env_with_defaults_has_correct_defaults"
        status: pass
    human_judgment: false
  - id: D2
    description: "MODEL_PATH validated: absolute, no traversal, exists (T-01-01)"
    verification:
      - kind: unit
        ref: "crates/hephaestus/src/config.rs#model_dir_rejects_relative_path"
        status: pass
      - kind: unit
        ref: "crates/hephaestus/src/config.rs#model_dir_rejects_parent_traversal"
        status: pass
      - kind: unit
        ref: "crates/hephaestus/src/config.rs#model_dir_accepts_valid_absolute_path"
        status: pass
      - kind: unit
        ref: "crates/hephaestus/src/config.rs#model_dir_rejects_nonexistent_path"
        status: pass
      - kind: unit
        ref: "crates/hephaestus/src/config.rs#model_dir_returns_error_when_model_path_is_none"
        status: pass
    human_judgment: false
  - id: D3
    description: "Binary startup sequence: config -> pipeline -> warmup -> ready (CORE-03)"
    requirement: "CORE-03"
    verification:
      - kind: other
        ref: "cargo build -p hephaestus (exits 0, compiles startup sequence)"
        status: pass
    human_judgment: true
    rationale: "Binary startup requires a real model directory to run end-to-end. Build verification confirms compilation but not runtime behavior. Full end-to-end test requires MODEL_ID + MODEL_PATH env vars set with a real model."
  - id: D4
    description: "Full workspace passes build, test, and clippy (XCUT-03)"
    requirement: "XCUT-03"
    verification:
      - kind: other
        ref: "cargo test --workspace (16 unit tests pass, 1 integration test pass)"
        status: pass
      - kind: other
        ref: "cargo clippy --workspace -- -D warnings (exits 0)"
        status: pass
      - kind: other
        ref: "cargo build --workspace (exits 0)"
        status: pass
    human_judgment: false

duration: 3min
completed: 2026-08-23
status: complete
---

# Phase 01 Plan 03: Binary Startup and Config Summary

**Typed env config via envy with path validation, structured JSON logging, and warmup inference pass -- completing the Phase 1 walking skeleton from config to ready**

## Performance

- **Duration:** 3 min
- **Started:** 2026-08-23T17:58:59Z
- **Completed:** 2026-08-23T18:02:33Z
- **Tasks:** 2
- **Files modified:** 5

## Accomplishments
- Implemented Config struct with envy deserialization: MODEL_ID required (crashes with clear error if missing per D-13), MODEL_PATH/EXECUTION_PROVIDER/LOG_LEVEL/WARMUP_INPUT optional with defaults
- Implemented Config::model_dir() with T-01-01 path validation: rejects relative paths, parent traversal (..), and nonexistent directories
- Replaced main.rs placeholder with full startup sequence: config load, path validation, ClassifierPipeline construction, warmup inference pass, ready report
- Added tracing-subscriber with JSON output and dual LOG_LEVEL/RUST_LOG env filter support
- Added 6 unit tests for Config covering all validation paths

## Task Commits

Each task was committed atomically:

1. **Task 1: Implement Config struct and main.rs startup sequence** - `fd2b80c` (feat)
2. **Task 2: Add config unit tests and run full workspace verification** - `09a4311` (test)

## Files Created/Modified
- `Cargo.toml` - Added tracing-subscriber to [workspace.dependencies]
- `crates/hephaestus/Cargo.toml` - Added tracing-subscriber dep and tempfile dev-dep
- `crates/hephaestus/src/config.rs` - Config struct, from_env(), model_dir(), 6 unit tests
- `crates/hephaestus/src/main.rs` - Startup sequence: config -> path validation -> pipeline -> warmup -> ready
- `Cargo.lock` - Updated with tracing-subscriber dependency tree

## Decisions Made
- **LOG_LEVEL/RUST_LOG dual support:** RUST_LOG takes precedence via `EnvFilter::try_from_default_env()`. If RUST_LOG is unset, the config's LOG_LEVEL value is used. This ensures D-12 works while also supporting the standard Rust logging env var.
- **Config loaded before tracing init:** Reordered from plan's suggested sequence so that config.log_level is available for EnvFilter construction. Error before tracing init goes to stderr via anyhow's default Display.
- **model_dir() existence check:** Added is_dir() validation beyond the plan's absolute + no-traversal checks. A nonexistent path would fail at ClassifierPipeline::new() anyway, but failing early with a clear message is better UX.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed LOG_LEVEL env var not being used**
- **Found during:** Task 1 (main.rs startup sequence)
- **Issue:** Plan specified `EnvFilter::from_default_env()` which reads RUST_LOG, not LOG_LEVEL (D-12). The config.log_level field would be parsed but never used, causing a dead_code warning and D-12 non-compliance.
- **Fix:** Changed to `EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.log_level))` -- RUST_LOG takes priority, LOG_LEVEL is the fallback.
- **Files modified:** crates/hephaestus/src/main.rs
- **Verification:** cargo build clean (no dead_code warning), clippy passes
- **Committed in:** fd2b80c

---

**Total deviations:** 1 auto-fixed (Rule 1 - bug)
**Impact on plan:** Necessary fix to make LOG_LEVEL env var functional per D-12. No scope creep.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Walking skeleton complete: developer sets MODEL_ID + MODEL_PATH, binary loads model, classifies warmup text, reports ready
- Phase 2 (HTTP serving) can add axum/tonic endpoints after the "hephaestus ready" log line
- Phase 2 will need tokio::sync::Mutex<Session> for concurrent access (Session::run takes &mut self)
- Phase 3 (model resolution) replaces the MODEL_PATH requirement with automatic S3/HF/Forge resolution

## Self-Check: PASSED

All 4 key files verified present on disk. Both task commits (fd2b80c, 09a4311) verified in git log.

---
*Phase: 01-core-inference-engine*
*Completed: 2026-08-23*
