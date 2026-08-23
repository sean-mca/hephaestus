# Phase 1: Core Inference Engine - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-08-22
**Phase:** 1-Core Inference Engine
**Areas discussed:** Workspace layout, Pipeline trait shape, Test model fixture, Config surface area

---

## Workspace Layout

| Option | Description | Selected |
|--------|-------------|----------|
| Full workspace from day 1 | Create workspace with separate crates now: hephaestus (binary), hephaestus-core, hephaestus-resolve, hephaestus-proto. Matches XCUT-02. | ✓ |
| Minimal workspace + grow | Start with 2 crates, split later as boundaries become clear. | |
| Single crate + modules | One crate with internal modules. Fastest to start but needs later migration. | |

**User's choice:** Full workspace from day 1
**Notes:** None

| Option | Description | Selected |
|--------|-------------|----------|
| 4 crates: binary + core + resolve + proto | hephaestus (bin), hephaestus-core, hephaestus-resolve (stub), hephaestus-proto (stub). Phase 1 builds binary + core. | ✓ |
| 3 crates: binary + core + config | Resolve and proto added when needed. | |
| You decide | Let Claude determine optimal split. | |

**User's choice:** 4 crates: binary + core + resolve + proto
**Notes:** None

| Option | Description | Selected |
|--------|-------------|----------|
| All 4 scaffolded | Create all 4 crate dirs with Cargo.toml, resolve + proto have placeholder lib.rs. | ✓ |
| Only active crates | Create just hephaestus + hephaestus-core. Add others when their phases start. | |
| You decide | Let Claude determine based on build complexity. | |

**User's choice:** All 4 scaffolded
**Notes:** None

| Option | Description | Selected |
|--------|-------------|----------|
| Central pinning | Root Cargo.toml declares [workspace.dependencies] with exact versions. Crates use dep.workspace = true. | ✓ |
| Per-crate pinning | Each crate pins its own dependency versions. | |
| You decide | Let Claude pick based on best practices. | |

**User's choice:** Central pinning with [workspace.dependencies]
**Notes:** None

---

## Pipeline Trait Shape

| Option | Description | Selected |
|--------|-------------|----------|
| Single process() -> typed result | One call does tokenize + infer + decode. Maximum Ousterhout depth. | |
| Single run() -> generic output | Generic enum input/output. One trait for all profiles, runtime dispatch. | |
| Separate build + run | prepare() -> PreparedInput, then execute(prepared) -> Result<Output>. Two steps for batching flexibility. | ✓ |

**User's choice:** Separate build + run
**Notes:** User chose two-step API over single-method deep module. Rationale: enables future dynamic batching (Phase 4) by collecting prepared inputs before executing as a batch.

| Option | Description | Selected |
|--------|-------------|----------|
| Trait with impls | trait Pipeline with ClassifierPipeline, EmbeddingsPipeline, etc. Compile-time dispatch. | ✓ |
| Enum dispatch | One Pipeline struct with ModelType enum. Runtime match on model type. | |
| You decide | Let Claude determine based on ort patterns. | |

**User's choice:** Trait with impls per profile type
**Notes:** None

| Option | Description | Selected |
|--------|-------------|----------|
| Pipeline owns both | Pipeline struct holds Arc<Session> and Arc<Tokenizer>. Constructor loads them. | ✓ |
| Pipeline borrows via references | Methods take &Session and &Tokenizer as params. | |
| Pipeline owns Session, tokenizer injected | Owns Session, takes shared tokenizer ref. | |

**User's choice:** Pipeline owns both
**Notes:** None

| Option | Description | Selected |
|--------|-------------|----------|
| Top label + score | ClassifierOutput { label: String, score: f32 }. Only returns the winning class. | ✓ |
| All labels + scores, sorted | Vec<Prediction> sorted by confidence. Caller picks top-k. | |
| You decide | Let Claude pick based on Minerva's existing callers. | |

**User's choice:** Top label + score
**Notes:** None

---

## Test Model Fixture

| Option | Description | Selected |
|--------|-------------|----------|
| distilbert-sst2 from HuggingFace | ~260MB sentiment classifier with ONNX exports. Real model, real tokenizer. | ✓ |
| Tiny synthetic ONNX model | Hand-crafted minimal ONNX graph as test fixture. Fast CI, no network. | |
| A Minerva production model | Whatever classifier Minerva runs in production. | |

**User's choice:** distilbert-sst2 from HuggingFace
**Notes:** None

| Option | Description | Selected |
|--------|-------------|----------|
| HF cache | Let hf-hub cache in ~/.cache/huggingface. Standard behavior. | ✓ |
| Git LFS fixture | Check ONNX model into repo via Git LFS. | |
| CI download + cache | CI pipeline downloads and caches. Local dev uses HF cache. | |

**User's choice:** HF cache
**Notes:** None

| Option | Description | Selected |
|--------|-------------|----------|
| Mock for unit, real for integration | Unit tests mock Pipeline trait via mockall. Integration tests load real model. | ✓ |
| Real models everywhere | All tests use real ONNX models. | |
| You decide | Let Claude determine test split. | |

**User's choice:** Mock for unit, real for integration
**Notes:** None

---

## Config Surface Area

| Option | Description | Selected |
|--------|-------------|----------|
| MODEL_PATH | Local model directory override for dev. | ✓ |
| EXECUTION_PROVIDER | Select ort execution provider (cpu/cuda/tensorrt). Default: cpu. | ✓ |
| LOG_LEVEL | Control tracing subscriber filter level. | ✓ |
| WARMUP_INPUT | Custom text for warmup inference pass. | ✓ |

**User's choice:** All four env vars selected (in addition to required MODEL_ID)
**Notes:** None

| Option | Description | Selected |
|--------|-------------|----------|
| Simple config struct + envy | Plain struct with serde derives, loaded via envy. No CLI parser. K8s-native. | ✓ |
| Clap with env-only mode | Clap derive with #[arg(env)] on every field. Works but unnecessary dependency. | |
| You decide | Let Claude pick lightest approach. | |

**User's choice:** Simple config struct + envy
**Notes:** User pointed out that Clap doesn't make sense for a k8s-only service — all config comes from env vars and configmaps, never CLI args.

| Option | Description | Selected |
|--------|-------------|----------|
| Fail hard on required, default optional | MODEL_ID missing = crash. Others have sensible defaults. K8s restarts handle it. | ✓ |
| Fail hard on everything | All env vars must be explicitly set. | |
| You decide | Let Claude determine required vs optional splits. | |

**User's choice:** Fail hard on required, default optional
**Notes:** None

---

## Claude's Discretion

No areas deferred to Claude's discretion.

## Deferred Ideas

None — discussion stayed within phase scope.
