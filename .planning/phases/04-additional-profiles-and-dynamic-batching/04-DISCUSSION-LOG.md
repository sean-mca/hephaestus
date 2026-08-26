# Phase 4: Additional Profiles and Dynamic Batching - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-08-26
**Phase:** 04-additional-profiles-and-dynamic-batching
**Areas discussed:** Profile selection, API response shape, Batching behavior, Seq2seq scope

---

## Profile Selection

| Option | Description | Selected |
|--------|-------------|----------|
| MODEL_PROFILE env var | Explicit env var, operator sets in k8s manifest | |
| Auto-detect from config.json | Infer profile from model's architectures/pipeline_tag fields | |
| Separate binary entrypoints | Different CLI subcommands or feature-flag compiled binaries | |

**User's choice:** Auto-detect from config.json, with optional MODEL_PROFILE override
**Notes:** User challenged the need for explicit profile config: "why do we need them to specify this? we cant tell from the model?" — auto-detection from config.json is the right default. MODEL_PROFILE kept as optional override for edge cases.

### Dispatch Mechanism

| Option | Description | Selected |
|--------|-------------|----------|
| Enum dispatch | PipelineKind enum wrapping concrete pipelines, match in handler | ✓ |
| Trait object (dyn Pipeline) | Box<dyn Pipeline> with erased associated types | |
| You decide | Let Claude pick | |

**User's choice:** Enum dispatch
**Notes:** No dynamic dispatch overhead, straightforward pattern matching.

---

## API Response Shape

| Option | Description | Selected |
|--------|-------------|----------|
| Single /infer, profile-aware response | One endpoint, response varies by model type | ✓ |
| Separate endpoints per profile | /classify, /embed, /generate, /ner | |
| You decide | Let Claude pick | |

**User's choice:** Single /infer endpoint
**Notes:** User's core principle: "we dont need to decide output shape, the model does that itself." Output shapes are model-determined — Hephaestus reads ONNX output tensors and passes results through after post-processing. No hardcoded response schemas per profile.

---

## Batching Behavior

### Collection Mechanism

| Option | Description | Selected |
|--------|-------------|----------|
| Channel-based collector | tokio::mpsc + oneshot for fan-out | ✓ |
| Mutex + condvar batch slot | Shared buffer behind Mutex | |
| You decide | Let Claude pick | |

**User's choice:** Channel-based collector

### Bypass When Disabled

| Option | Description | Selected |
|--------|-------------|----------|
| Bypass — direct execute | No channel when batching off, zero overhead | ✓ |
| Always use channel | Uniform code path even with batch_size=1 | |

**User's choice:** Bypass when disabled

### Profile Scope

| Option | Description | Selected |
|--------|-------------|----------|
| All profiles | Any profile can opt into batching | ✓ |
| Embeddings + classifier only | Only fixed-shape output profiles | |
| You decide | Let Claude determine | |

**User's choice:** All profiles

### Config

| Option | Description | Selected |
|--------|-------------|----------|
| BATCH_ENABLED + BATCH_MAX_SIZE + BATCH_MAX_WAIT_MS | Three env vars, follows envy pattern | ✓ |
| Single BATCH_SIZE env var | 0=disabled, N=enabled | |
| You decide | Let Claude determine | |

**User's choice:** Three env vars

---

## Seq2seq Scope

| Option | Description | Selected |
|--------|-------------|----------|
| Single-pass only | Fused encoder-decoder ONNX graphs, no decode loop | ✓ |
| Full auto-regressive decode | Separate encoder/decoder, KV cache, beam search | |
| You decide | Let Claude determine | |

**User's choice:** "single pass for now, we'll revisit later"

---

## Claude's Discretion

- NER post-processing details (BIO tag handling, span merging vs raw tokens)
- Embeddings post-processing (L2 normalization, mean pooling strategy)
- Profile detection heuristics for ambiguous models
- Response serialization approach for model-determined output shapes

## Deferred Ideas

- Auto-regressive seq2seq decoding — future phase
- gRPC API for high-throughput internal callers (v2 requirement APIX-01)
