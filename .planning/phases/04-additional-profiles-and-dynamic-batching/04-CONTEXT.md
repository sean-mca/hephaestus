# Phase 4: Additional Profiles and Dynamic Batching - Context

**Gathered:** 2026-08-26
**Status:** Ready for planning

<domain>
## Phase Boundary

Add three new model type profiles (embeddings, seq2seq, token classifier) and optional dynamic request batching to the existing inference runtime. Each profile implements the existing `Pipeline` trait. The runtime auto-detects which profile to use from the model's config.json. A single `/infer` endpoint serves all profiles — output shapes are determined by the model, not hardcoded by Hephaestus. Batching leverages the existing two-step prepare/execute API with a channel-based collector. No Forge, no GPU, no gRPC, no new HTTP infrastructure beyond generalizing the existing handler.

</domain>

<decisions>
## Implementation Decisions

### Profile Detection
- **D-01:** Auto-detect profile from the model's `config.json` — use `architectures` field (e.g., `*ForSequenceClassification` → classifier, `*ForTokenClassification` → NER, `*ForConditionalGeneration` → seq2seq) and `pipeline_tag` when present. Operators just set `MODEL_ID`. No mandatory profile config.
- **D-02:** Optional `MODEL_PROFILE` env var as an override for ambiguous models. When set, it takes precedence over auto-detection. When unset (the common case), auto-detection handles it.

### Pipeline Dispatch
- **D-03:** Enum dispatch via a `PipelineKind` enum wrapping each concrete pipeline. `AppState` holds `Mutex<PipelineKind>` instead of `Mutex<ClassifierPipeline>`. Match on variant in the handler. No trait objects, no dynamic dispatch overhead.

### API Design
- **D-04:** Single `/infer` endpoint for all profiles. Response shape is determined by the model's output — Hephaestus reads the ONNX graph's output tensors, applies profile-appropriate post-processing, and passes the result through. No hardcoded output schemas per profile.
- **D-05:** Output shapes are model-determined, not prescribed by Hephaestus. The runtime does NOT define fixed response structs per profile. It faithfully represents whatever the ONNX graph produces after post-processing.

### Dynamic Batching
- **D-06:** Channel-based batcher. Handler calls `prepare()` immediately, then sends the `PreparedInput` + a oneshot response sender into a tokio::mpsc channel. Background task collects up to `max_batch_size` or `max_wait_time`, calls `execute()` as a batch, fans results back via oneshot channels.
- **D-07:** When batching is disabled (the default), requests bypass the channel entirely and call `prepare()` then `execute()` directly — identical to the current classifier flow. Zero overhead when batching is off.
- **D-08:** All profiles support batching. Operators enable it per-deployment. Profiles that don't benefit simply won't have it enabled.
- **D-09:** Three env vars following the existing envy config pattern: `BATCH_ENABLED` (bool, default false), `BATCH_MAX_SIZE` (u32, default 8), `BATCH_MAX_WAIT_MS` (u64, default 50).

### Seq2seq Scope
- **D-10:** Single-pass inference only for v1. Support models exported as a single fused ONNX graph (e.g., via optimum with beam search baked in). No auto-regressive decode loop in Hephaestus. Full decode support deferred to a future phase.

### Claude's Discretion
- NER post-processing details (BIO tag handling, span merging vs raw tokens)
- Embeddings post-processing (L2 normalization, mean pooling strategy)
- Profile detection heuristics for ambiguous models (e.g., base encoder models without task heads)
- Response serialization approach for model-determined output shapes

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Project Context
- `.planning/PROJECT.md` — Core value, constraints, key decisions, model resolution flow
- `.planning/REQUIREMENTS.md` — Full v1 requirement list (Phase 4 requirements: PROF-02, PROF-03, PROF-04, BTCH-01, BTCH-02, BTCH-03)
- `.planning/ROADMAP.md` — Phase 4 goal, success criteria, dependency chain

### Prior Phase Context
- `.planning/phases/01-core-inference-engine/01-CONTEXT.md` — Pipeline trait design (D-04: two-step prepare/execute for batching, D-05: trait-per-profile dispatch, D-06: Pipeline owns Session + Tokenizer, D-07: profile-specific output types)
- `.planning/phases/02-http-serving-and-observability/02-CONTEXT.md` — API crate structure (D-04), handler patterns, StageTimer deep module (D-09), config extensions (D-15)
- `.planning/phases/03-model-resolution/03-CONTEXT.md` — Resolution chain, S3 cache, config.json loading patterns

### Coding Rules
- `rules/` — Full directory of Rust coding rules. All code must comply.

### Existing Code (Phases 1-3 output)
- `crates/hephaestus-core/src/pipeline.rs` — `Pipeline` trait with associated types, `ClassifierPipeline`, `PreparedInput`. Phase 4 adds new pipeline impls here.
- `crates/hephaestus-core/src/postprocess.rs` — `softmax()` and `argmax_with_score()`. Phase 4 adds profile-specific post-processing (L2 norm, token decoding).
- `crates/hephaestus-api/src/state.rs` — `AppState` currently holds `Mutex<ClassifierPipeline>`. Phase 4 generalizes to `Mutex<PipelineKind>`.
- `crates/hephaestus-api/src/handlers.rs` — `/infer` handler currently returns classifier-specific `InferResponse`. Phase 4 generalizes to model-determined output.
- `crates/hephaestus/src/config.rs` — Config struct with envy. Phase 4 extends with `MODEL_PROFILE`, `BATCH_ENABLED`, `BATCH_MAX_SIZE`, `BATCH_MAX_WAIT_MS`.
- `crates/hephaestus/src/main.rs` — Binary entry point. Phase 4 adds profile detection and batcher initialization.
- `Cargo.toml` — Workspace deps. May need additions for batching (tokio channels already available via tokio).

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `Pipeline` trait with `prepare()`/`execute()` two-step API: new profiles implement this directly. The batching design uses `prepare()` output as the unit of collection.
- `postprocess::softmax()` and `argmax_with_score()`: classifier-specific but the module is the right home for new post-processing functions (L2 norm, token decoding).
- `StageTimer` deep module: already hides all metrics plumbing. New profiles use the same timer interface — no metrics code in profile implementations.
- `Config` struct with envy: extend with new fields following the established `#[serde(default)]` pattern.

### Established Patterns
- Config from env vars via envy — all new config follows this pattern
- thiserror for library errors in crate boundaries, anyhow for binary-level
- Deep module interfaces: `Pipeline::prepare()` + `Pipeline::execute()` hide all complexity
- `AppState` provides controlled accessors (Ousterhout pattern) — no reaching into internals
- `CoreError` enum maps to HTTP error responses via `ApiError`

### Integration Points
- `state.rs:23` — `pipeline: Mutex<ClassifierPipeline>` → generalize to `Mutex<PipelineKind>`
- `state.rs:46` — `AppState::new()` takes `ClassifierPipeline` → takes `PipelineKind`
- `handlers.rs:54-128` — `/infer` handler calls `pipeline.prepare()` then `pipeline.execute()` → generalize to dispatch on `PipelineKind`, serialize model-determined output
- `main.rs` — startup flow: resolve model → construct pipeline → build state → serve. Phase 4 inserts profile detection between resolve and construct.
- `config.rs` — add `model_profile`, `batch_enabled`, `batch_max_size`, `batch_max_wait_ms` fields

</code_context>

<specifics>
## Specific Ideas

- Output shapes are model-determined. Hephaestus reads the ONNX graph's output tensors and passes results through after profile-appropriate post-processing. The runtime does NOT prescribe response schemas — it faithfully represents what the model produces.
- The existing prepare/execute split was specifically designed for batching (Phase 1, D-04). The batcher collects `PreparedInput` values and executes them as a batch.

</specifics>

<deferred>
## Deferred Ideas

- Auto-regressive seq2seq decoding (separate encoder/decoder ONNX files, token-by-token decode loop with KV cache) — future phase
- gRPC API for high-throughput internal callers (v2 requirement APIX-01)

</deferred>

---

*Phase: 4-Additional Profiles and Dynamic Batching*
*Context gathered: 2026-08-26*
