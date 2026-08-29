---
phase: 11-websocket-streaming-asr-pipeline
plan: 01
subsystem: api
tags: [onnx, inference, pipeline, type-system, rust]

requires:
  - phase: 10-grpc-inference-api
    provides: PipelineKind enum dispatch, REST/gRPC handlers, batcher channel
provides:
  - InferenceInput enum (Text/Audio) for multi-modal pipeline dispatch
  - PreparedData enum wrapping text and audio preprocessing output
  - PipelineOutput enum with typed variants for all model profiles
  - CoreError::InvalidInput for modality mismatch errors
  - PreparedAudio struct for future ASR feature preprocessing
affects: [11-websocket-streaming-asr-pipeline]

tech-stack:
  added: []
  patterns:
    - "InferenceInput enum dispatch for multi-modal input routing"
    - "PipelineOutput::to_json() as single JSON conversion point for HTTP/gRPC"
    - "From<String> for InferenceInput enables zero-change backward compat"

key-files:
  created: []
  modified:
    - crates/hephaestus-core/src/pipeline.rs
    - crates/hephaestus-core/src/error.rs
    - crates/hephaestus-core/src/lib.rs
    - crates/hephaestus-api/src/error.rs
    - crates/hephaestus-api/src/handlers.rs
    - crates/hephaestus-api/src/grpc/inference.rs
    - crates/hephaestus-api/src/batcher.rs

key-decisions:
  - "PipelineKind::prepare accepts impl Into<InferenceInput> for backward-compat String callers"
  - "PipelineOutput::to_json() replaces serde_json::json! in PipelineKind::execute"
  - "PreparedAudio fields marked #[allow(dead_code)] until ASR pipeline (Plan 11-03)"
  - "execute_batch handles mixed PreparedData by inserting dummy PreparedInput for audio indices and replacing results with errors"

patterns-established:
  - "Multi-modal dispatch: match on (PipelineKind variant, InferenceInput variant) tuple"
  - "Typed output: PipelineOutput enum with to_json() bridge to JSON API responses"

requirements-completed: [PRFX-01, APIX-02]

coverage:
  - id: D1
    description: "InferenceInput enum with Text/Audio variants and From<String> impl"
    requirement: "PRFX-01"
    verification:
      - kind: unit
        ref: "crates/hephaestus-core/src/pipeline.rs#inference_input_from_string_produces_text"
        status: pass
    human_judgment: false
  - id: D2
    description: "PreparedData enum with into_text() accessor for batcher path"
    requirement: "PRFX-01"
    verification:
      - kind: unit
        ref: "crates/hephaestus-core/src/pipeline.rs#prepared_data_into_text_returns_some_for_text"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-core/src/pipeline.rs#prepared_data_into_text_returns_none_for_audio"
        status: pass
    human_judgment: false
  - id: D3
    description: "PipelineOutput enum with to_json() for all five variants"
    requirement: "APIX-02"
    verification:
      - kind: unit
        ref: "crates/hephaestus-core/src/pipeline.rs#pipeline_output_classifier_to_json"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-core/src/pipeline.rs#pipeline_output_embeddings_to_json"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-core/src/pipeline.rs#pipeline_output_seq2seq_to_json"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-core/src/pipeline.rs#pipeline_output_token_classifier_to_json"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-core/src/pipeline.rs#pipeline_output_asr_to_json"
        status: pass
    human_judgment: false
  - id: D4
    description: "CoreError::InvalidInput maps to ApiError::BadRequest via From impl"
    verification:
      - kind: unit
        ref: "cargo test --workspace -q (all 52 tests pass including error mapping)"
        status: pass
    human_judgment: false
  - id: D5
    description: "All existing REST, gRPC, and batcher handlers work unchanged with text models"
    verification:
      - kind: integration
        ref: "cargo test --workspace -q (52 tests, 0 failures)"
        status: pass
      - kind: unit
        ref: "cargo build -p hephaestus (binary compiles with no changes to main.rs)"
        status: pass
    human_judgment: false

duration: 5min
completed: 2026-08-29
status: complete
---

# Phase 11 Plan 01: Pipeline Type Generalization Summary

**InferenceInput/PreparedData/PipelineOutput type system enabling multi-modal pipeline dispatch with full backward compatibility for text models**

## Performance

- **Duration:** 5 min
- **Started:** 2026-08-29T12:04:29Z
- **Completed:** 2026-08-29T12:09:43Z
- **Tasks:** 2
- **Files modified:** 7

## Accomplishments
- Generalized PipelineKind to accept heterogeneous input types (text and audio) via InferenceInput enum with From<String> for zero-change backward compat
- Replaced raw serde_json::Value returns with typed PipelineOutput enum across execute, execute_batch, and all four batch_postprocess functions
- Migrated all callers (handlers.rs, grpc/inference.rs, batcher.rs) to use PreparedData and PipelineOutput with to_json() bridge
- Added CoreError::InvalidInput variant with ApiError::BadRequest mapping for modality mismatch errors
- All 52 workspace tests pass, binary compiles without changes to main.rs warmup code

## Task Commits

Each task was committed atomically:

1. **Task 1: Define InferenceInput, PreparedData, PipelineOutput types and update error handling** - `ae516c7` (feat)
2. **Task 2: Update PipelineKind dispatch methods and migrate all callers to new types** - `9d94a92` (feat)

## Files Created/Modified
- `crates/hephaestus-core/src/pipeline.rs` - InferenceInput, PreparedAudio, PreparedData, PipelineOutput enums/structs; updated PipelineKind dispatch and batch postprocess functions
- `crates/hephaestus-core/src/error.rs` - Added CoreError::InvalidInput variant
- `crates/hephaestus-core/src/lib.rs` - Re-exports for new types
- `crates/hephaestus-api/src/error.rs` - InvalidInput to BadRequest mapping in From<CoreError>
- `crates/hephaestus-api/src/handlers.rs` - to_json() conversion on PipelineOutput
- `crates/hephaestus-api/src/grpc/inference.rs` - to_json() conversion on PipelineOutput
- `crates/hephaestus-api/src/batcher.rs` - PreparedData/PipelineOutput channel types

## Decisions Made
- PipelineKind::prepare accepts `impl Into<InferenceInput>` so existing String callers compile without changes
- PipelineOutput::to_json() centralizes JSON conversion; serde_json::json! macros removed from PipelineKind::execute
- PreparedAudio fields marked `#[allow(dead_code)]` since they are forward-looking for Plan 11-03 ASR pipeline
- execute_batch handles mixed PreparedData batches by inserting dummy entries for audio indices and replacing those results with InvalidInput errors

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Suppressed dead_code warning on PreparedAudio fields**
- **Found during:** Task 2 (binary compilation check)
- **Issue:** `features` and `raw_samples` fields triggered dead_code warning since no ASR pipeline consumes them yet
- **Fix:** Added `#[allow(dead_code)]` attribute with comment referencing Plan 11-03
- **Files modified:** crates/hephaestus-core/src/pipeline.rs
- **Verification:** `cargo build -p hephaestus` produces zero warnings
- **Committed in:** 9d94a92 (Task 2 commit)

---

**Total deviations:** 1 auto-fixed (1 bug)
**Impact on plan:** Minor warning suppression, no scope change.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Type system foundation complete for ASR pipeline (Plan 11-03)
- InferenceInput::Audio variant ready for mel spectrogram/waveform routing
- PreparedData::Audio variant ready for ASR preprocessing output
- PipelineOutput::Asr variant ready for transcription results

## Self-Check: PASSED

All 7 modified files exist, both task commits (ae516c7, 9d94a92) verified in git log, SUMMARY.md written.

---
*Phase: 11-websocket-streaming-asr-pipeline*
*Completed: 2026-08-29*
