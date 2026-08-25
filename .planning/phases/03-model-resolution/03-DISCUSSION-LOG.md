# Phase 3: Model Resolution - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-08-24
**Phase:** 03-model-resolution
**Areas discussed:** S3 cache layout, Resolution fallback behavior, Forge client contract, Cache-back strategy

---

## S3 Cache Layout

### Q1: How should model files be keyed in S3?

| Option | Description | Selected |
|--------|-------------|----------|
| Flat model ID prefix | s3://{bucket}/{model_id}/model.onnx, tokenizer.json, config.json | ✓ |
| Content-addressed hashes | s3://{bucket}/{sha256}/... — mirrors HF blob store | |
| HF-style repo/revision | s3://{bucket}/{org}/{model}/{revision}/... | |

**User's choice:** Flat model ID prefix

### Q2: Which files should be cached to S3?

| Option | Description | Selected |
|--------|-------------|----------|
| Full model directory | Cache everything ClassifierPipeline::new() needs | ✓ |
| ONNX only, tokenizer from HF | Cache only model.onnx, fetch tokenizer.json from HF each time | |

**User's choice:** Full model directory

### Q3: S3 bucket configuration

| Option | Description | Selected |
|--------|-------------|----------|
| Env var (S3_BUCKET) | Explicit env var following existing envy pattern | ✓ |
| Convention-based | Derive from cluster context | |

**User's choice:** Env var (S3_BUCKET)

### Q4: Org namespace handling in S3 keys

| Option | Description | Selected |
|--------|-------------|----------|
| Preserve slashes as path segments | s3://{bucket}/sentence-transformers/all-MiniLM-L6-v2/model.onnx | ✓ |
| Flatten to single segment | Replace '/' with '--' | |

**User's choice:** Preserve slashes as path segments

---

## Resolution Fallback Behavior

### Q1: Missing ONNX export behavior (pre-Forge)

| Option | Description | Selected |
|--------|-------------|----------|
| Fail with clear error | Return error: "model X has no ONNX export and Forge is not configured" | ✓ |
| Skip to Forge stub, fail there | Let resolution attempt Forge tier first | |

**User's choice:** Fail with clear error

### Q2: Transient failure handling

| Option | Description | Selected |
|--------|-------------|----------|
| Retry within tier, then fail | 2-3 retries with backoff per tier | ✓ |
| No retry, fall through | Immediately try next tier on any error | |
| Crash and let k8s retry | Any failure crashes the pod | |

**User's choice:** Retry within tier, then fail

### Q3: Download atomicity

| Option | Description | Selected |
|--------|-------------|----------|
| Temp dir + atomic rename | Download to temp, verify, rename to final path | ✓ |
| Direct download to cache | Download directly to final path | |

**User's choice:** Temp dir + atomic rename

### Q4: Local cache location

| Option | Description | Selected |
|--------|-------------|----------|
| HF cache directory | Use ~/.cache/huggingface (or HF_HOME) | ✓ |
| Custom cache directory | Dedicated path like /models/{model_id}/ | |
| You decide | Let Claude pick | |

**User's choice:** HF cache directory

---

## Forge Client Contract

### Q1: Protocol

| Option | Description | Selected |
|--------|-------------|----------|
| HTTP REST (reqwest) | Simple POST, cross-language compatible | ✓ |
| gRPC (tonic client) | Typed protobuf contract, consistent with hephaestus-proto | |
| You decide | Let Claude pick | |

**User's choice:** HTTP REST (reqwest)

### Q2: Configuration

| Option | Description | Selected |
|--------|-------------|----------|
| Optional FORGE_URL env var | When set, third tier active. When unset, resolution stops at tier 2 | ✓ |
| Always configured, default disabled | FORGE_URL + FORGE_ENABLED=true/false | |

**User's choice:** Optional FORGE_URL env var

### Q3: Trait vs concrete stub

| Option | Description | Selected |
|--------|-------------|----------|
| Trait + stub impl | ForgeClient trait with convert() method, stub returns "unavailable" | ✓ |
| Concrete stub only | Just a function that returns error, refactor in Phase 5 | |
| You decide | Let Claude pick | |

**User's choice:** Trait + stub impl

### Q4: Conversion request shape

| Option | Description | Selected |
|--------|-------------|----------|
| Model ID only | POST {"model_id": "org/model"} — Forge handles everything | ✓ |
| Model ID + target profile | POST {"model_id": "...", "profile": "classifier"} | |
| You decide | Let Claude pick | |

**User's choice:** Model ID only

---

## Cache-Back Strategy

### Q1: Upload timing

| Option | Description | Selected |
|--------|-------------|----------|
| Synchronous before serving | Upload to S3 before loading model | |
| Background async after serving | Load model, start serving, upload in background | ✓ |
| You decide | Let Claude pick | |

**User's choice:** Background async after serving

### Q2: Duplicate upload prevention

| Option | Description | Selected |
|--------|-------------|----------|
| Check before upload | HeadObject check, skip if exists | |
| Upload unconditionally | S3 PutObject is idempotent, always upload | ✓ |
| You decide | Let Claude pick | |

**User's choice:** Upload unconditionally

### Q3: Upload failure handling

| Option | Description | Selected |
|--------|-------------|----------|
| Log warning, continue serving | No retry, just log | |
| Retry with backoff | 2-3 retries with exponential backoff, then log warning | ✓ |
| You decide | Let Claude pick | |

**User's choice:** Retry with backoff

---

## Claude's Discretion

No areas deferred to Claude's discretion.

## Deferred Ideas

None — discussion stayed within phase scope.
