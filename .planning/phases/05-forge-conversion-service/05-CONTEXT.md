# Phase 5: Forge Conversion Service - Context

**Gathered:** 2026-08-26
**Status:** Ready for planning

<domain>
## Phase Boundary

Build a persistent Python service (the Forge) that converts non-ONNX HuggingFace models to ONNX format via `optimum`, validates the conversion, uploads all artifacts to S3, and completes the 3-tier resolution chain. On the Rust side, replace `StubForgeClient` with a real HTTP client using `reqwest` and generalize `ModelResolver` to accept the real implementation. End-to-end: a Hephaestus pod starts with a model that has no ONNX export anywhere, the Forge converts it, uploads to S3, and inference succeeds on the converted model. No GPU support, no model training, no multi-Forge replicas.

</domain>

<decisions>
## Implementation Decisions

### Conversion Flow
- **D-01:** Synchronous long-poll API. Hephaestus POSTs `{"model_id": "org/model"}` to the Forge and blocks until conversion finishes. The Forge returns the response when done. Simple, matches the existing `ForgeClient::convert()` contract.
- **D-02:** Response includes S3 paths plus conversion metadata (model architecture, original format, conversion duration, optimum version). The Rust `ForgeClient::convert()` return type changes from `Vec<String>` to a struct that captures both paths and metadata.
- **D-03:** Forge downloads the PyTorch model from HuggingFace internally. Hephaestus sends only the `model_id`. Forge is self-contained — no file transfer between services.
- **D-04:** Configurable HTTP timeout via `FORGE_TIMEOUT_SECS` env var with a sensible default (e.g., 600s). Added to the existing envy-based `Config` struct. Operators tune per deployment based on expected model sizes.

### ONNX Validation
- **D-05:** Two-stage validation before uploading to S3: (1) `onnx.checker.check_model()` to verify graph structure, then (2) a dummy inference pass through the converted model using `onnxruntime` in Python. Catches both structural and runtime errors.
- **D-06:** If validation fails, return an error immediately with failure details. No automatic retry with different optimum settings. Operator investigates.
- **D-07:** Validate all artifacts before uploading — model.onnx, tokenizer.json, and config.json must all exist and be parseable. Hephaestus needs all three to serve.

### Concurrent Conversion
- **D-08:** In-memory lock per model ID. First request converts; subsequent requests for the same model_id block and wait for the first to finish, then receive the same S3 paths. Prevents duplicate work.
- **D-09:** Single Forge replica for v1. In-memory lock is sufficient. Conversion is infrequent (only for new model deployments). Horizontal scaling deferred.
- **D-10:** Sequential conversion queue — one conversion at a time. Model conversion is CPU/memory intensive; running multiple in parallel on a single pod risks OOM.

### Forge Project Structure
- **D-11:** Forge lives in `forge/` at the repo root, alongside `crates/`. Clear separation between Rust and Python code.
- **D-12:** FastAPI for the web framework. Async, auto-generated OpenAPI docs, Pydantic validation. Standard in the ML ecosystem.
- **D-13:** Dockerfile included in this phase for the Forge service. No k8s manifests — those live in the Minerva cluster repo.
- **D-14:** `uv` + `pyproject.toml` for Python dependency management. Modern, fast, lockfile support.

### Claude's Discretion
- FastAPI app structure (routers, middleware, error handling patterns)
- Optimum conversion flags and opset version selection
- Test inference input generation for validation (D-05 dummy inference)
- Rust `ForgeClient` return type struct design (field names, serde derives)
- How `ModelResolver` is generalized (trait object vs generic parameter)
- S3 upload implementation details in Python (boto3 patterns, multipart upload thresholds)

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Project Context
- `.planning/PROJECT.md` — Core value, constraints, key decisions, model resolution flow description
- `.planning/REQUIREMENTS.md` — Full v1 requirement list (Phase 5 requirements: FORG-01, FORG-02, FORG-03, FORG-04)
- `.planning/ROADMAP.md` — Phase 5 goal, success criteria, dependency chain

### Prior Phase Context
- `.planning/phases/03-model-resolution/03-CONTEXT.md` — S3 cache layout (D-01), resolution fallback behavior (D-04/D-05), ForgeClient trait contract (D-08/D-09/D-10/D-11), cache-back strategy (D-12/D-13/D-14)
- `.planning/phases/04-additional-profiles-and-dynamic-batching/04-CONTEXT.md` — Profile auto-detection (D-01/D-02), PipelineKind enum dispatch (D-03), model-determined output shapes (D-05)

### Coding Rules
- `rules/` — Full directory of Rust coding rules. All code must comply for the Rust client changes.

### Existing Rust Code (Forge integration points)
- `crates/hephaestus-resolve/src/forge.rs` — `ForgeClient` trait and `StubForgeClient`. Phase 5 replaces the stub with a real reqwest-based implementation.
- `crates/hephaestus-resolve/src/resolver.rs` — `ModelResolver` with concrete `StubForgeClient` field. Phase 5 generalizes to accept the real client.
- `crates/hephaestus-resolve/src/error.rs` — `ResolveError::ForgeUnavailable` variant. May need new error variants for Forge HTTP failures.
- `crates/hephaestus/src/config.rs` — Config struct with `forge_url: Option<String>`. Phase 5 adds `FORGE_TIMEOUT_SECS`.
- `crates/hephaestus/src/main.rs` — Binary entry point. Wires `forge_url` into `ModelResolver::new()`.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `ForgeClient` trait with `convert(&self, model_id: &str) -> Result<Vec<String>, ResolveError>`: defines the Rust-side contract. Phase 5 changes the return type to include metadata.
- `StubForgeClient`: reference implementation showing the trait pattern and error handling.
- `Config` struct with envy: extend with `forge_timeout_secs` following the established `#[serde(default)]` pattern.
- `ResolveError` enum: extend with Forge-specific HTTP error variants.

### Established Patterns
- Config from env vars via envy — all new config follows this pattern
- thiserror for library errors in crate boundaries, anyhow for binary-level
- Deep module interfaces: `ForgeClient::convert()` is already a single-method trait (Ousterhout pattern)
- `#[cfg_attr(test, mockall::automock)]` on `ForgeClient` trait — maintains testability with mockall

### Integration Points
- `resolver.rs:28` — `forge: StubForgeClient` field → generalize to accept real implementation
- `resolver.rs:84` — `ModelResolver::new()` constructor → accept `ForgeClient` impl instead of hardcoding stub
- `resolver.rs:189` — `self.forge.convert(model_id).await` → already calls through the trait, works with real client
- `config.rs:74-77` — `forge_url: Option<String>` → add `forge_timeout_secs` field
- `main.rs:60` — passes `config.forge_url.as_deref()` to resolver → construct real client when URL is set

</code_context>

<specifics>
## Specific Ideas

- The Forge response carries metadata (architecture, original format, conversion duration, optimum version) for debugging and operational visibility. Hephaestus logs this metadata but doesn't need it for inference — it only needs the S3 paths to download the model.
- Sequential conversion queue with in-memory locking keeps the implementation simple and prevents resource exhaustion on the single-replica Forge pod.
- Validation runs onnxruntime in Python (not just onnx.checker) to catch runtime errors that graph-only validation misses — shape mismatches, missing ops, etc.

</specifics>

<deferred>
## Deferred Ideas

- Horizontal Forge scaling with distributed locking — future phase if conversion volume justifies it
- Automatic retry with fallback optimum settings on conversion failure
- Auto-regressive seq2seq decode support in converted models
- Full PyTorch output comparison validation (beyond graph check + test inference)

</deferred>

---

*Phase: 5-Forge Conversion Service*
*Context gathered: 2026-08-26*
