---
phase: 04-additional-profiles-and-dynamic-batching
plan: 01
subsystem: inference
tags: [onnx, embeddings, profile-detection, pipeline, mean-pooling, l2-normalize]

requires:
  - phase: 01-core-inference-engine
    provides: Pipeline trait, ClassifierPipeline, PreparedInput, postprocess utilities
  - phase: 03-model-resolution
    provides: ModelResolver, resolved model directory with config.json

provides:
  - ModelProfile enum with four profile variants
  - detect_profile() function for auto-detecting model profile from config.json
  - EmbeddingsPipeline with mean pooling and L2 normalization
  - PipelineKind enum dispatch for multi-profile serving
  - Generalized AppState holding Mutex<PipelineKind>
  - Model-determined JSON output via serde_json::Value (D-05)
  - MODEL_PROFILE env var override (D-02)
  - is_batching_enabled() stub on AppState

affects: [04-02, 04-03, 05-forge]

tech-stack:
  added: []
  patterns:
    - "PipelineKind enum dispatch instead of trait objects (D-03)"
    - "Shared load_session_and_tokenizer helper for all pipelines"
    - "Shared tokenize_text helper for all text-based pipelines"
    - "Model-determined JSON output via serde_json::Value (D-05)"
    - "Profile detection: architectures suffix matching -> pipeline_tag fallback -> override"

key-files:
  created:
    - crates/hephaestus-core/src/profile.rs
  modified:
    - crates/hephaestus-core/src/pipeline.rs
    - crates/hephaestus-core/src/postprocess.rs
    - crates/hephaestus-core/src/lib.rs
    - crates/hephaestus-api/src/state.rs
    - crates/hephaestus-api/src/handlers.rs
    - crates/hephaestus/src/config.rs
    - crates/hephaestus/src/main.rs
    - crates/hephaestus/Cargo.toml

key-decisions:
  - "Extracted shared load_session_and_tokenizer and tokenize_text helpers to eliminate duplication across pipeline constructors and prepare methods"
  - "Extracted run_onnx_inference helper to share tensor construction and session.run across pipeline execute methods"
  - "Removed InferResponse struct; handler now inserts model_id and latency_ms into PipelineKind::execute() JSON output dynamically"
  - "Added serde_json dependency to binary crate for config.json parsing in main.rs"

patterns-established:
  - "PipelineKind enum dispatch: new profiles add a variant, implement Pipeline trait, and add a match arm in prepare/execute"
  - "Profile detection: detect_profile() checks override -> architectures -> pipeline_tag; returns CoreError::Config on failure"
  - "Handler inserts metadata (model_id, latency_ms) into model-determined JSON output post-execution"

requirements-completed: [PROF-02]

coverage:
  - id: D1
    description: "Profile detection module with ModelProfile enum and detect_profile() supporting architecture suffix matching, pipeline_tag fallback, and MODEL_PROFILE override"
    requirement: "PROF-02"
    verification:
      - kind: unit
        ref: "crates/hephaestus-core/src/profile.rs#test_detect_classifier_from_architectures"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-core/src/profile.rs#test_detect_embeddings_from_architectures"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-core/src/profile.rs#test_detect_seq2seq_from_architectures"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-core/src/profile.rs#test_detect_token_classifier_from_architectures"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-core/src/profile.rs#test_override_takes_precedence"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-core/src/profile.rs#test_fallback_to_pipeline_tag"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-core/src/profile.rs#test_unknown_architecture_returns_error"
        status: pass
    human_judgment: false

  - id: D2
    description: "EmbeddingsPipeline with mean pooling and L2 normalization post-processing implementing Pipeline trait"
    requirement: "PROF-02"
    verification:
      - kind: unit
        ref: "crates/hephaestus-core/src/postprocess.rs#test_mean_pool_excludes_padding"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-core/src/postprocess.rs#test_mean_pool_single_token"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-core/src/postprocess.rs#test_l2_normalize_unit_vector"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-core/src/postprocess.rs#test_l2_normalize_zero_vector"
        status: pass
    human_judgment: false

  - id: D3
    description: "PipelineKind enum dispatch with Classifier and Embeddings variants returning model-determined JSON"
    verification:
      - kind: unit
        ref: "crates/hephaestus-core/src/pipeline.rs#test_pipeline_kind_variant_sizes"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-api/src/handlers.rs#model_determined_output_accepts_model_id_and_latency"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-api/src/handlers.rs#embeddings_output_accepts_model_id_and_latency"
        status: pass
    human_judgment: false

  - id: D4
    description: "Runtime generalized from ClassifierPipeline to PipelineKind with profile detection at startup"
    verification:
      - kind: unit
        ref: "cargo build --workspace (exits 0)"
        status: pass
      - kind: unit
        ref: "cargo clippy --workspace -- -D warnings (exits 0)"
        status: pass
    human_judgment: false

duration: 8min
completed: 2026-08-26
status: complete
---

# Phase 04 Plan 01: Embeddings Profile and Multi-Profile Dispatch Summary

**Multi-profile inference dispatch with embeddings pipeline, profile auto-detection from config.json, and model-determined JSON output via PipelineKind enum**

## Performance

- **Duration:** 8 min
- **Started:** 2026-08-26T15:30:18Z
- **Completed:** 2026-08-26T15:39:14Z
- **Tasks:** 2
- **Files modified:** 9

## Accomplishments
- Created profile detection module with auto-detection from config.json architectures field, pipeline_tag fallback, and MODEL_PROFILE env var override
- Implemented EmbeddingsPipeline with mean pooling and L2 normalization post-processing for sentence embedding models
- Generalized runtime from single-profile ClassifierPipeline to multi-profile PipelineKind enum dispatch
- Changed handler to return model-determined JSON (serde_json::Value) instead of fixed InferResponse struct
- Extracted shared helpers (load_session_and_tokenizer, tokenize_text, run_onnx_inference) eliminating code duplication
- All 78 workspace tests passing, clippy clean

## Task Commits

Each task was committed atomically:

1. **Task 1: Profile detection, PipelineKind enum, EmbeddingsPipeline, and post-processing** - `d972428` (feat)
2. **Task 2: Generalize AppState, handler, config, and main.rs for PipelineKind dispatch** - `57a55b0` (feat)

## Files Created/Modified
- `crates/hephaestus-core/src/profile.rs` - New: ModelProfile enum, detect_profile(), parse_profile_string(), 13 unit tests
- `crates/hephaestus-core/src/pipeline.rs` - Added EmbeddingsPipeline, PipelineKind enum, shared helpers (load_session_and_tokenizer, tokenize_text, run_onnx_inference)
- `crates/hephaestus-core/src/postprocess.rs` - Added mean_pool() and l2_normalize() with 4 unit tests
- `crates/hephaestus-core/src/lib.rs` - Added profile module, updated re-exports for new types
- `crates/hephaestus-api/src/state.rs` - Changed pipeline field from Mutex<ClassifierPipeline> to Mutex<PipelineKind>, added is_batching_enabled() stub
- `crates/hephaestus-api/src/handlers.rs` - Changed return type to Json<serde_json::Value>, removed InferResponse struct, added dynamic model_id/latency_ms insertion
- `crates/hephaestus/src/config.rs` - Added model_profile: Option<String> field (D-02)
- `crates/hephaestus/src/main.rs` - Added profile detection step, PipelineKind construction match, updated warmup to use PipelineKind methods
- `crates/hephaestus/Cargo.toml` - Added serde_json workspace dependency

## Decisions Made
- Extracted shared load_session_and_tokenizer and tokenize_text helpers rather than duplicating across ClassifierPipeline and EmbeddingsPipeline constructors -- follows DRY principle while keeping each pipeline's execute() method profile-specific
- Extracted run_onnx_inference helper to share tensor construction and session.run logic across pipeline execute methods, requiring lifetime annotations
- Removed InferResponse struct entirely; handler now inserts model_id and latency_ms into the serde_json::Value returned by PipelineKind::execute(), preserving backward compatibility for classifiers while supporting any profile's output shape
- Added serde_json dependency to the binary crate for parsing config.json during profile detection in main.rs

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Added serde_json dependency to binary crate**
- **Found during:** Task 2 (main.rs profile detection)
- **Issue:** main.rs uses serde_json::Value for parsing config.json but the binary crate did not list serde_json as a dependency
- **Fix:** Added `serde_json.workspace = true` to crates/hephaestus/Cargo.toml
- **Files modified:** crates/hephaestus/Cargo.toml
- **Verification:** cargo build --workspace passes
- **Committed in:** 57a55b0 (Task 2 commit)

**2. [Rule 3 - Blocking] Fixed ort Shape API lifetime and indexing**
- **Found during:** Task 1 (EmbeddingsPipeline execute and run_onnx_inference helper)
- **Issue:** ort Shape struct derefs to [i64], not [usize]; run_onnx_inference needed explicit lifetime annotations for SessionOutputs
- **Fix:** Used shape.len() for dimension count and shape[2] as usize for hidden_dim; added lifetime parameter to run_onnx_inference
- **Files modified:** crates/hephaestus-core/src/pipeline.rs
- **Verification:** cargo test -p hephaestus-core --lib passes
- **Committed in:** d972428 (Task 1 commit)

---

**Total deviations:** 2 auto-fixed (2 blocking)
**Impact on plan:** Both fixes necessary for compilation. No scope creep.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- PipelineKind has Classifier and Embeddings variants; Plan 02 will add Seq2Seq and TokenClassifier variants
- is_batching_enabled() stub returns false; Plan 03 will implement the batcher
- Profile detection covers all four profile types; Seq2Seq and TokenClassifier arms in main.rs bail with clear messages until Plan 02

## Self-Check: PASSED

All 9 created/modified files verified on disk. Both task commits (d972428, 57a55b0) verified in git log.

---
*Phase: 04-additional-profiles-and-dynamic-batching*
*Completed: 2026-08-26*
