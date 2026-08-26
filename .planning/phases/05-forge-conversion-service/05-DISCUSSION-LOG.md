# Phase 5: Forge Conversion Service - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-08-26
**Phase:** 05-forge-conversion-service
**Areas discussed:** Conversion flow model, ONNX validation depth, Concurrent conversion, Forge project structure

---

## Conversion Flow Model

### How should the Forge handle long-running conversions?

| Option | Description | Selected |
|--------|-------------|----------|
| Synchronous long-poll | Hephaestus POSTs and blocks until conversion finishes. Simple, matches existing ForgeClient::convert() contract. | ✓ |
| Async task with polling | POST returns a task ID immediately, Hephaestus polls GET /status/{id} until done. | |
| You decide | Let Claude pick. | |

**User's choice:** Synchronous long-poll

### What should the Forge response include beyond S3 paths?

| Option | Description | Selected |
|--------|-------------|----------|
| S3 paths only | Just the list of uploaded S3 keys. Matches current Vec<String> return type. | |
| S3 paths + metadata | S3 keys plus conversion metadata: model architecture, original format, conversion duration, optimum version. | ✓ |
| You decide | Let Claude pick. | |

**User's choice:** S3 paths + metadata

### Should the Forge download the PyTorch model itself?

| Option | Description | Selected |
|--------|-------------|----------|
| Forge downloads itself | Forge receives model_id, downloads from HuggingFace internally. Self-contained. | ✓ |
| Hephaestus passes files | Hephaestus downloads PyTorch weights and sends them to Forge. | |
| You decide | Let Claude pick. | |

**User's choice:** Forge downloads itself

### HTTP timeout for Forge conversion requests?

| Option | Description | Selected |
|--------|-------------|----------|
| 10 minutes | Covers most classifier/embeddings models. | |
| 30 minutes | Conservative — handles even large seq2seq models. | |
| Configurable via env var | FORGE_TIMEOUT_SECS with sensible default. Operators tune per deployment. | ✓ |
| You decide | Let Claude pick. | |

**User's choice:** Configurable via env var

---

## ONNX Validation Depth

### What level of ONNX validation should the Forge perform?

| Option | Description | Selected |
|--------|-------------|----------|
| Graph check only | onnx.checker.check_model() — fast but catches only structural errors. | |
| Graph check + test inference | Graph check plus dummy inference pass via onnxruntime. Catches runtime errors. | ✓ |
| Full comparison with PyTorch | Run same input through both models, compare within tolerance. | |
| You decide | Let Claude pick. | |

**User's choice:** Graph check + test inference

### If validation fails, retry with different settings?

| Option | Description | Selected |
|--------|-------------|----------|
| No retry, fail immediately | Return error with details. Operator investigates. | ✓ |
| One retry with fallback settings | Try again with conservative optimum flags. | |
| You decide | Let Claude pick. | |

**User's choice:** No retry, fail immediately

### Validate tokenizer.json and config.json too?

| Option | Description | Selected |
|--------|-------------|----------|
| Yes, validate all artifacts | Check model.onnx, tokenizer.json, config.json all exist and are parseable. | ✓ |
| Model only, trust HF metadata | Only validate ONNX model file. | |
| You decide | Let Claude pick. | |

**User's choice:** Yes, validate all artifacts

---

## Concurrent Conversion

### How handle multiple simultaneous conversion requests for the same model?

| Option | Description | Selected |
|--------|-------------|----------|
| In-memory lock per model ID | First request converts, others wait and get same result. | ✓ |
| Let them race | Each converts independently. S3 PutObject is idempotent. | |
| External lock (Redis/DynamoDB) | Distributed lock across replicas. | |
| You decide | Let Claude pick. | |

**User's choice:** In-memory lock per model ID

### Single replica or horizontal scaling?

| Option | Description | Selected |
|--------|-------------|----------|
| Single replica for v1 | One Forge pod. In-memory lock works. Conversion is infrequent. | ✓ |
| Multiple replicas from the start | Requires distributed locking. | |
| You decide | Let Claude pick. | |

**User's choice:** Single replica for v1

### Queue or concurrent processing?

| Option | Description | Selected |
|--------|-------------|----------|
| Sequential queue | One conversion at a time. Prevents OOM on single pod. | ✓ |
| Bounded concurrency | Up to N concurrent conversions. | |
| You decide | Let Claude pick. | |

**User's choice:** Sequential queue

---

## Forge Project Structure

### Where should the Forge live in this repo?

| Option | Description | Selected |
|--------|-------------|----------|
| forge/ at repo root | Top-level alongside crates/. Clear separation. | ✓ |
| services/forge/ | Under a services/ directory. | |
| Separate repository | Own repo entirely. | |
| You decide | Let Claude pick. | |

**User's choice:** forge/ at repo root

### Python web framework?

| Option | Description | Selected |
|--------|-------------|----------|
| FastAPI | Async, OpenAPI docs, Pydantic validation. ML ecosystem standard. | ✓ |
| Flask | Simple, synchronous by default. | |
| You decide | Let Claude pick. | |

**User's choice:** FastAPI

### Dockerfile in scope?

| Option | Description | Selected |
|--------|-------------|----------|
| Yes, include Dockerfile | Build a Dockerfile for the Forge. No k8s manifests. | ✓ |
| No, just the Python app | Containerization handled separately. | |
| You decide | Let Claude pick. | |

**User's choice:** Yes, include Dockerfile

### Python dependency management?

| Option | Description | Selected |
|--------|-------------|----------|
| uv + pyproject.toml | Modern, fast, lockfile support. | ✓ |
| Poetry + pyproject.toml | Mature, lockfile, virtual env management. | |
| pip + requirements.txt | Simplest. No lockfile by default. | |
| You decide | Let Claude pick. | |

**User's choice:** uv + pyproject.toml

---

## Claude's Discretion

- FastAPI app structure (routers, middleware, error handling patterns)
- Optimum conversion flags and opset version selection
- Test inference input generation for validation
- Rust ForgeClient return type struct design
- How ModelResolver is generalized (trait object vs generic parameter)
- S3 upload implementation details in Python (boto3 patterns)

## Deferred Ideas

- Horizontal Forge scaling with distributed locking
- Automatic retry with fallback optimum settings on conversion failure
- Auto-regressive seq2seq decode support in converted models
- Full PyTorch output comparison validation
