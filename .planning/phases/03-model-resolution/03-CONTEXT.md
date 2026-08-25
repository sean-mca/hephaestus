# Phase 3: Model Resolution - Context

**Gathered:** 2026-08-24
**Status:** Ready for planning

<domain>
## Phase Boundary

Build the 3-tier model resolution chain inside the `hephaestus-resolve` crate. A single `resolve()` call checks S3 cache, falls back to HuggingFace ONNX downloads, and falls back again to the Forge conversion API (stub in this phase). After resolution, the caller receives a local directory path that `ClassifierPipeline::new()` can consume directly. The resolver also uploads newly-resolved models back to S3 in the background for future pods. No Forge server implementation (Phase 5), no new model profiles, no changes to the HTTP serving layer beyond wiring the resolver into startup.

</domain>

<decisions>
## Implementation Decisions

### S3 Cache Layout
- **D-01:** Flat model ID prefix in S3: `s3://{bucket}/{model_id}/model.onnx`, `tokenizer.json`, `config.json`. Model IDs with org namespaces (e.g., `sentence-transformers/all-MiniLM-L6-v2`) preserve slashes as S3 path segments.
- **D-02:** Full model directory cached to S3 — everything `ClassifierPipeline::new()` needs (model.onnx or onnx/model.onnx, tokenizer.json, config.json). S3 cache hit requires zero HuggingFace contact.
- **D-03:** S3 bucket configured via `S3_BUCKET` env var, following the existing envy config pattern (Phase 1, D-11). Optional — when unset, S3 tier is skipped and resolution starts at HuggingFace.

### Resolution Fallback Behavior
- **D-04:** When HuggingFace has the model but no ONNX export, fail with a clear error message: "model X has no ONNX export and Forge is not configured". Consistent with D-13 (fail hard on missing requirements). Do not silently fall through to the Forge stub.
- **D-05:** Retry within each tier (2-3 attempts with exponential backoff) before moving to the next tier or failing. Prevents transient S3 blips from triggering unnecessary HuggingFace downloads.
- **D-06:** Downloads go to a temporary directory first, then atomically renamed to the final local cache path. Prevents serving partially-downloaded models if the pod crashes mid-download.
- **D-07:** Local model cache uses the HuggingFace cache directory (`~/.cache/huggingface` or `HF_HOME`). Shares cache with `hf-hub`'s built-in caching. Integration tests already use this path.

### Forge Client Contract
- **D-08:** Forge client uses HTTP REST via `reqwest`. Simple POST with model ID, Forge returns S3 paths of converted files. Cross-language compatibility with the Python Forge service. Adds `reqwest` to workspace dependencies.
- **D-09:** Forge configured via optional `FORGE_URL` env var. When set, the third tier (Forge conversion) is active. When unset, resolution stops at tier 2 (HuggingFace) and fails if no ONNX export exists.
- **D-10:** Define a `ForgeClient` trait in `hephaestus-resolve` with a single `convert()` method. Phase 3 ships a stub implementation that returns "Forge unavailable". Phase 5 provides the real HTTP implementation. Testable with mockall.
- **D-11:** Forge conversion request sends only the model ID: `POST {"model_id": "org/model"}`. Forge handles downloading PyTorch weights, converting, uploading to S3, and returning the S3 paths.

### Cache-Back Strategy
- **D-12:** Background async upload to S3 after the pod starts serving. Download from HF → load model → start serving → upload to S3 in a background tokio task. Faster pod startup; if the pod crashes before upload completes, next pod downloads from HF again.
- **D-13:** Upload unconditionally — no HeadObject check before uploading. S3 PutObject is idempotent. Avoids the extra API call. Worst case on concurrent pod starts is redundant uploads, not corruption.
- **D-14:** Retry S3 upload with exponential backoff (2-3 attempts) on failure. On final failure, log a warning and continue serving. Upload failure has no impact on the running pod's inference capability.

### Claude's Discretion
No areas deferred to Claude's discretion — all decisions made explicitly.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Project Context
- `.planning/PROJECT.md` — Core value, constraints, key decisions, model resolution flow description
- `.planning/REQUIREMENTS.md` — Full v1 requirement list (Phase 3 requirements: RSLV-01 through RSLV-05)
- `.planning/ROADMAP.md` — Phase 3 goal, success criteria, dependency chain

### Prior Phase Context
- `.planning/phases/01-core-inference-engine/01-CONTEXT.md` — Workspace layout (D-01 through D-03), Pipeline trait design (D-04 through D-07), config pattern (D-11 through D-13)
- `.planning/phases/02-http-serving-and-observability/02-CONTEXT.md` — API crate structure (D-04), config extensions (D-15), shutdown patterns

### Coding Rules
- `rules/` — Full directory of Rust coding rules. All code must comply.

### Existing Code (Phases 1-2 output)
- `crates/hephaestus-resolve/src/lib.rs` — Stub crate, currently one-line comment. This is where resolution logic goes.
- `crates/hephaestus/src/config.rs` — Config struct with envy. Phase 3 extends with S3_BUCKET, S3_PREFIX, FORGE_URL.
- `crates/hephaestus-core/src/pipeline.rs` — `ClassifierPipeline::new(model_dir: &Path)` — the consumer of resolved model directories.
- `crates/hephaestus/src/main.rs` — Binary entry point. Phase 3 wires resolver call before pipeline construction.
- `Cargo.toml` — Workspace deps. `hf-hub` already present. Needs `aws-sdk-s3`, `aws-config`, `reqwest`.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `hephaestus-resolve` crate: already scaffolded with Cargo.toml and stub lib.rs. Ready for resolution logic.
- `Config` struct with envy: extend with `s3_bucket`, `s3_prefix`, `forge_url` fields following the same `#[serde(default)]` pattern.
- `hf-hub` in workspace deps: already used in integration tests for model downloads. `Api::new()` / `ApiBuilder` pattern available.
- `ClassifierPipeline::new(model_dir: &Path)`: the contract that resolution must satisfy — produce a local directory with model.onnx, tokenizer.json, config.json.

### Established Patterns
- Config from env vars via envy (D-11) — all new config follows this pattern
- thiserror for library errors in crate boundaries, anyhow for binary-level
- Deep module interfaces: `resolve()` should be a single call hiding S3/HF/Forge complexity
- Atomic operations: temp dir + rename pattern for crash safety (new for Phase 3, consistent with fail-hard philosophy)

### Integration Points
- `main.rs`: currently calls `config.model_dir()` which requires `MODEL_PATH`. Phase 3 replaces this with `resolve(config.model_id)` → local path → `ClassifierPipeline::new(path)`.
- `config.rs` line 109: `MODEL_PATH` context message says "model resolution not yet implemented — Phase 3". This gets replaced.
- `Cargo.toml` workspace deps: add `aws-sdk-s3`, `aws-config`, `reqwest`
- `hephaestus-resolve/Cargo.toml`: add dependencies on hf-hub, aws-sdk-s3, aws-config, reqwest, tokio

</code_context>

<specifics>
## Specific Ideas

No specific requirements — open to standard approaches. Key constraint: Ousterhout deep module pattern — `resolve()` is a single call that hides the 3-tier chain (RSLV-05).

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

</deferred>

---

*Phase: 3-Model Resolution*
*Context gathered: 2026-08-24*
