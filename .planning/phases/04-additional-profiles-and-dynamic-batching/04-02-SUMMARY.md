---
phase: 04-additional-profiles-and-dynamic-batching
plan: 02
subsystem: inference
tags: [onnx, seq2seq, ner, token-classification, pipeline, postprocessing]

requires:
  - phase: 01-core-inference-engine
    provides: Pipeline trait, ClassifierPipeline, PreparedInput, postprocess utilities
  - phase: 04-additional-profiles-and-dynamic-batching (plan 01)
    provides: PipelineKind enum, EmbeddingsPipeline, profile detection, shared helpers

provides:
  - Seq2SeqPipeline with fused single-pass inference and tokenizer decode
  - TokenClassifierPipeline with per-token argmax and BIO span merging
  - Entity struct for NER/token classification output
  - argmax_per_token() and merge_subword_entities() postprocessing functions
  - Complete PipelineKind enum with all four profile variants
  - All four ModelProfile variants wired in main.rs

affects: [04-03, 05-forge]

tech-stack:
  added: []
  patterns:
    - "Fused seq2seq: extract output tensor as i64 token IDs, fall back to f32, decode via tokenizer"
    - "Token classification: encoding preserved in PreparedInput for word_id-based subword merging"
    - "Entity struct lives in pipeline.rs alongside other public output types; postprocess.rs imports it"
    - "BIO span merging: first subword token's prediction used for the whole word, consecutive same-type merged"

key-files:
  created: []
  modified:
    - crates/hephaestus-core/src/pipeline.rs
    - crates/hephaestus-core/src/postprocess.rs
    - crates/hephaestus-core/src/lib.rs
    - crates/hephaestus/src/main.rs

key-decisions:
  - "Entity struct defined in pipeline.rs (not postprocess.rs) to keep public types in one module and avoid making postprocess module public"
  - "PreparedInput gains optional encoding field rather than re-tokenizing in execute() -- avoids wasted compute"
  - "Seq2Seq output extraction tries i64 first, falls back to f32 with rounding -- handles both fused model output formats"

patterns-established:
  - "New pipeline types: implement Pipeline trait, add PipelineKind variant, add dispatch arms in prepare/execute, wire in main.rs"
  - "Token classification word reconstruction: decode overlapping token IDs via tokenizer rather than string concatenation"

requirements-completed: [PROF-03, PROF-04]

coverage:
  - id: D1
    description: "Seq2SeqPipeline with fused single-pass ONNX inference, i64/f32 output extraction, and tokenizer decode"
    requirement: "PROF-03"
    verification:
      - kind: unit
        ref: "cargo build --workspace (exits 0)"
        status: pass
      - kind: unit
        ref: "cargo clippy --workspace -- -D warnings (exits 0)"
        status: pass
    human_judgment: false

  - id: D2
    description: "TokenClassifierPipeline with per-token argmax, subword merging via word_ids, BIO span grouping, and entity output"
    requirement: "PROF-04"
    verification:
      - kind: unit
        ref: "crates/hephaestus-core/src/postprocess.rs#test_argmax_per_token_known_logits"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-core/src/postprocess.rs#test_argmax_per_token_single_label"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-core/src/postprocess.rs#test_entity_serialization"
        status: pass
      - kind: unit
        ref: "cargo clippy --workspace -- -D warnings (exits 0)"
        status: pass
    human_judgment: false

  - id: D3
    description: "PipelineKind has all four variants (Classifier, Embeddings, Seq2Seq, TokenClassifier) with correct JSON output shapes"
    verification:
      - kind: unit
        ref: "crates/hephaestus-core/src/pipeline.rs#test_pipeline_kind_variant_sizes"
        status: pass
      - kind: unit
        ref: "cargo build --workspace (exits 0)"
        status: pass
    human_judgment: false

  - id: D4
    description: "main.rs constructs all four pipeline types -- no bail stubs remain"
    verification:
      - kind: unit
        ref: "cargo build --workspace (exits 0)"
        status: pass
    human_judgment: false

duration: 7min
completed: 2026-08-26
status: complete
---

# Phase 04 Plan 02: Seq2Seq and Token Classifier Pipelines Summary

**Fused single-pass seq2seq pipeline with tokenizer decode output and NER token classifier with per-token argmax and BIO span merging, completing all four profile types**

## Performance

- **Duration:** 7 min
- **Started:** 2026-08-26T15:44:11Z
- **Completed:** 2026-08-26T15:51:30Z
- **Tasks:** 2
- **Files modified:** 4

## Accomplishments
- Implemented Seq2SeqPipeline with fused single-pass ONNX inference per D-10 -- extracts output token IDs (i64 with f32 fallback) and decodes via tokenizer
- Implemented TokenClassifierPipeline with per-token argmax labeling, subword merging via word_ids, and BIO span grouping into Entity structs
- Added Entity struct, argmax_per_token(), and merge_subword_entities() postprocessing functions
- Completed PipelineKind enum with all four profile variants -- Classifier, Embeddings, Seq2Seq, TokenClassifier
- All main.rs profile arms now construct real pipelines (no bail stubs remain)
- All 81 workspace tests passing, clippy clean

## Task Commits

Each task was committed atomically:

1. **Task 1: Seq2SeqPipeline with fused single-pass inference and token decoding** - `dfe90b1` (feat)
2. **Task 2: TokenClassifierPipeline with per-token labeling and BIO span merging** - `6804e4e` (feat)

## Files Created/Modified
- `crates/hephaestus-core/src/pipeline.rs` - Added Seq2SeqPipeline, TokenClassifierPipeline, Entity struct, PipelineKind::Seq2Seq and TokenClassifier variants with dispatch arms, encoding field on PreparedInput
- `crates/hephaestus-core/src/postprocess.rs` - Added argmax_per_token(), merge_subword_entities(), 3 new unit tests
- `crates/hephaestus-core/src/lib.rs` - Updated re-exports: Seq2SeqPipeline, TokenClassifierPipeline, Entity
- `crates/hephaestus/src/main.rs` - Replaced Seq2Seq and TokenClassifier bail stubs with pipeline construction

## Decisions Made
- Entity struct defined in pipeline.rs alongside ClassifierOutput and other public types, imported by postprocess.rs -- avoids making postprocess module public while keeping Entity accessible through the Pipeline trait's Output type
- PreparedInput gains an optional encoding field (Option<tokenizers::Encoding>) set to Some by TokenClassifierPipeline::prepare() and None by all others -- avoids re-tokenizing in execute() at the cost of one extra field
- Seq2Seq output extraction tries i64 first (the common format for fused models), falls back to f32 with rounding -- handles both output tensor types without failing on either

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Entity struct placement for public visibility**
- **Found during:** Task 2 (TokenClassifierPipeline)
- **Issue:** Plan specified Entity as pub(crate) in postprocess.rs, but Pipeline trait Output type must be publicly accessible since the trait is pub
- **Fix:** Moved Entity definition to pipeline.rs (where ClassifierOutput lives) and had postprocess.rs import it via `use crate::pipeline::Entity`
- **Files modified:** crates/hephaestus-core/src/pipeline.rs, crates/hephaestus-core/src/postprocess.rs
- **Verification:** cargo build --workspace passes, Entity accessible from lib.rs re-exports
- **Committed in:** 6804e4e (Task 2 commit)

**2. [Rule 1 - Bug] Fixed Rust 2024 pattern matching syntax**
- **Found during:** Task 2 (clippy + build)
- **Issue:** Rust 2024 edition disallows explicit dereference patterns in implicitly-borrowing contexts; clippy flagged collapsible if
- **Fix:** Changed `&(start, end)` to `(start, end)` with `*start`/`*end` dereferences; collapsed nested if per clippy
- **Files modified:** crates/hephaestus-core/src/pipeline.rs, crates/hephaestus-core/src/postprocess.rs
- **Verification:** cargo clippy --workspace -- -D warnings passes
- **Committed in:** 6804e4e (Task 2 commit)

---

**Total deviations:** 2 auto-fixed (1 blocking, 1 bug)
**Impact on plan:** Both fixes necessary for compilation and clippy compliance. No scope creep.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- All four pipeline types implemented: Classifier, Embeddings, Seq2Seq, TokenClassifier
- PipelineKind enum complete with dispatch for all profiles
- Plan 03 can implement dynamic batching on top of the existing prepare/execute split
- is_batching_enabled() stub still returns false; Plan 03 will activate the batcher

## Self-Check: PASSED

All 4 modified files verified on disk. Both task commits (dfe90b1, 6804e4e) verified in git log.

---
*Phase: 04-additional-profiles-and-dynamic-batching*
*Completed: 2026-08-26*
