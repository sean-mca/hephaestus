# Hephaestus

## What This Is

Hephaestus is a unified ONNX model inference runtime built in Rust. It replaces Minerva's scattered Python runtimes and Bedrock API calls with a single container that can load, serve, and manage any ONNX-compatible model — classifiers, embeddings, ASR/TTS, LLMs. It pairs with a Python-based conversion service (the Forge) that handles exporting non-ONNX HuggingFace models to ONNX format.

## Core Value

A single Rust binary that takes a model name, resolves it to ONNX files (from S3 cache, HuggingFace, or Forge conversion), and serves inference with full pre/post-processing — replacing every per-model Python runtime in the cluster.

## Requirements

### Validated

(None yet — ship to validate)

### Active

- [ ] Load and serve ONNX models via `ort` (ONNX Runtime Rust bindings)
- [ ] 3-tier model resolution: S3 cache → HuggingFace ONNX exports → Forge conversion
- [ ] Download models from HuggingFace via `hf-hub` crate
- [ ] Cache resolved ONNX models to S3
- [ ] Serve inference over gRPC and HTTP/REST
- [ ] Model type profiles with full pre/post-processing pipelines (classifiers first)
- [ ] Tokenization via HuggingFace `tokenizers` crate (load tokenizer.json from HF/S3)
- [ ] One model per pod, configured via environment variables
- [ ] CPU and GPU execution provider support
- [ ] Configurable dynamic batching (default: single request)
- [ ] Prometheus metrics, OpenTelemetry tracing, structured logging
- [ ] Kubernetes liveness and readiness probes
- [ ] Forge: persistent Python service for ONNX conversion via `optimum`
- [ ] Forge: upload converted models to S3

### Out of Scope

- Multi-model per pod — complexity not justified for internal use
- Custom model architectures outside ONNX — Hephaestus only runs ONNX graphs
- EKS-specific features — must work on any k8s cluster
- Public API / multi-tenant auth — internal Minerva service only

## Context

Minerva's production cluster currently runs multiple Python services for model inference, each with its own runtime, dependencies, and deployment config. Some call AWS Bedrock for tasks (sentiment, intent classification) that local models handle fine. This creates operational overhead, inconsistent interfaces, and unnecessary cloud API costs.

Hephaestus consolidates all of this behind one Rust binary and one deployment pattern. The Forge handles the one thing Rust can't do — converting PyTorch/TensorFlow models to ONNX format — as a separate persistent Python service.

### Model Resolution Flow

1. Pod starts with `MODEL_ID=org/model-name` env var
2. Check S3 cache for ONNX files → found → load and serve
3. S3 miss → check HuggingFace for existing ONNX export → found → download, cache to S3, serve
4. No ONNX anywhere → call Forge service → Forge converts with Python/optimum, uploads to S3 → Hephaestus loads

### Key Crates

- `ort` — ONNX Runtime Rust bindings (pykeio/ort)
- `hf-hub` — HuggingFace model downloads (huggingface/hf-hub)
- `tokenizers` — HuggingFace tokenizers (Rust-native)

## Constraints

- **Language**: Rust 2024 edition, workspace resolver 3 — no exceptions
- **Code style**: All rules in `rules/` must be followed; traits expose Ousterhout-style deep classes
- **Conversion**: ONNX export requires Python (optimum/transformers) — no pure Rust path exists; isolated in the Forge service
- **Model scope (v1)**: Classifiers first, then expand to embeddings, ASR/TTS, LLMs

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| ONNX Runtime via `ort` | Single runtime for all model types, CPU+GPU, mature ecosystem | — Pending |
| Separate Forge service for conversion | Keeps Rust runtime pure; conversion is infrequent, heavy, Python-only | — Pending |
| Persistent Forge (not k8s Job) | Simpler to call, faster response when conversion needed | — Pending |
| Pre/post-processing in Hephaestus (Option A) | Drop-in replacement for existing services — callers shouldn't change | — Pending |
| Model type profiles (not per-model logic) | ~4-5 profiles (classifier, embeddings, seq2seq, etc.) vs per-model custom code | — Pending |
| gRPC + HTTP/REST dual API | gRPC for high-throughput internal callers, REST for quick integrations | — Pending |
| One model per pod via env config | Simple, k8s-native scaling — scale the model by scaling pods | — Pending |
| Configurable dynamic batching | Default single-request for low latency, opt-in batching for throughput | — Pending |
| Renamed from Blacksmith to Hephaestus | Better name — god of the forge, crafting tools | — Pending |

## Evolution

This document evolves at phase transitions and milestone boundaries.

**After each phase transition** (via `/gsd-transition`):
1. Requirements invalidated? → Move to Out of Scope with reason
2. Requirements validated? → Move to Validated with phase reference
3. New requirements emerged? → Add to Active
4. Decisions to log? → Add to Key Decisions
5. "What This Is" still accurate? → Update if drifted

**After each milestone** (via `/gsd-complete-milestone`):
1. Full review of all sections
2. Core Value check — still the right priority?
3. Audit Out of Scope — reasons still valid?
4. Update Context with current state

---
*Last updated: 2026-08-22 after initialization*
