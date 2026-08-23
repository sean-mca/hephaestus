# Phase 1: Core Inference Engine - Context

**Gathered:** 2026-08-22
**Status:** Ready for planning

<domain>
## Phase Boundary

Build the foundational Rust workspace that loads an ONNX classifier model from a local path, tokenizes text input, runs inference via ort, and returns classification results programmatically. No HTTP server, no model resolution chain, no GPU — just the core inference pipeline working end-to-end on CPU.

</domain>

<decisions>
## Implementation Decisions

### Workspace Layout
- **D-01:** Full workspace from day 1 — 4 crates: `hephaestus` (binary), `hephaestus-core` (pipeline, tokenizer, inference), `hephaestus-resolve` (model resolution — Phase 3, stub for now), `hephaestus-proto` (protobuf types — Phase 2+)
- **D-02:** All 4 crate directories scaffolded in Phase 1 with Cargo.toml files. Resolve and proto have only placeholder `lib.rs`. Phase 1 builds binary + core only.
- **D-03:** Central dependency pinning via `[workspace.dependencies]` in root Cargo.toml. Crates use `dep.workspace = true`. One place to update versions.

### Pipeline Trait Design
- **D-04:** Pipeline trait uses a two-step API: `prepare(&self, input) -> PreparedInput` then `execute(&self, prepared) -> Result<Output>`. Two steps enable future batching (Phase 4) — collect prepared inputs, execute as batch.
- **D-05:** Trait-per-profile dispatch: `ClassifierPipeline`, `EmbeddingsPipeline`, etc. each implement the `Pipeline` trait. Compile-time dispatch. Each pod runs one impl.
- **D-06:** Pipeline owns both `Arc<Session>` and `Arc<Tokenizer>`. Constructor loads them. Callers never touch internal ort or tokenizer types.
- **D-07:** Classifier output is `ClassifierOutput { label: String, score: f32 }` — returns only the top predicted label with confidence score.

### Test Strategy
- **D-08:** Development and integration test model: `distilbert-base-uncased-finetuned-sst-2-english` from HuggingFace. Small (~260MB), well-known sentiment classifier with existing ONNX exports.
- **D-09:** Model caching via standard HF cache (`~/.cache/huggingface`). First run downloads, subsequent runs use cache. No model checked into repo.
- **D-10:** Unit tests mock the Pipeline trait via `mockall`. Integration tests load the real distilbert model and run actual inference. Clear separation: fast unit tests, thorough integration tests.

### Configuration
- **D-11:** Config loaded from env vars only (no CLI parser). Simple config struct with serde derives, loaded via `envy` crate. This is a k8s-only service — all config comes from env vars and configmaps.
- **D-12:** Env vars: `MODEL_ID` (required — pod crashes with clear error if missing), `MODEL_PATH` (optional — local directory override for dev), `EXECUTION_PROVIDER` (optional, default: cpu), `LOG_LEVEL` (optional, default: info), `WARMUP_INPUT` (optional — custom text for warmup pass).
- **D-13:** Fail hard on required config (MODEL_ID), use sensible defaults for optional config. K8s restart policy handles the crash.

### Claude's Discretion
No areas deferred to Claude's discretion — all decisions made explicitly.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Project Context
- `.planning/PROJECT.md` — Core value, constraints, key decisions
- `.planning/REQUIREMENTS.md` — Full v1 requirement list with traceability to phases
- `.planning/ROADMAP.md` — Phase 1 goal, success criteria, and dependency chain

### Coding Rules
- `rules/` — Full directory of Rust coding rules. All code must comply.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
None — greenfield project. No existing Rust code.

### Established Patterns
None yet. Phase 1 establishes the patterns for all subsequent phases.

### Integration Points
None for Phase 1 — this is the foundation. Phase 2 (HTTP serving) and Phase 3 (model resolution) build on top of the core crate.

</code_context>

<specifics>
## Specific Ideas

No specific requirements — open to standard approaches. Key constraint: Ousterhout deep module pattern for all public traits (1-3 methods hiding significant complexity).

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

</deferred>

---

*Phase: 1-Core Inference Engine*
*Context gathered: 2026-08-22*
