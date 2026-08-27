# Roadmap: Hephaestus

## Overview

Hephaestus replaces Minerva's scattered Python model-serving runtimes with a single Rust binary that loads any ONNX model and serves inference. The roadmap follows the dependency chain: build the inference engine first, make it HTTP-servable, add self-building model resolution, expand to multiple model types with optional batching, then complete the resolution chain with the Forge conversion service.

## Phases

**Phase Numbering:**

- Integer phases (1, 2, 3): Planned milestone work
- Decimal phases (2.1, 2.2): Urgent insertions (marked with INSERTED)

Decimal phases appear between their surrounding integers in numeric order.

- [x] **Phase 1: Core Inference Engine** - Rust workspace with ONNX model loading, tokenization, and classifier inference (completed 2026-08-23)
- [x] **Phase 2: HTTP Serving and Observability** - Deployable HTTP service with health probes, metrics, logging, and tracing (completed 2026-08-24)
- [x] **Phase 3: Model Resolution** - Self-building S3 cache with HuggingFace fallback and cache-back (completed 2026-08-26)
- [x] **Phase 4: Additional Profiles and Dynamic Batching** - Embeddings, seq2seq, and NER model types with optional request batching (completed 2026-08-26)
- [x] **Phase 5: Forge Conversion Service** - Python service for auto-converting non-ONNX models to ONNX format (completed 2026-08-26)
- [x] **Phase 7: Production Hardening** - token_type_ids support, request body limits, resilient warmup/shutdown, smart retry (completed 2026-08-26)
- [ ] **Phase 8: Inference Quality and Concurrency** - NER score normalization, pipeline mutex split for concurrent tokenization, real integration tests

## Phase Details

### Phase 1: Core Inference Engine

**Goal**: Users can load an ONNX classifier model and run text classification inference programmatically
**Mode:** mvp
**Depends on**: Nothing (first phase)
**Requirements**: XCUT-01, XCUT-02, XCUT-03, CORE-01, CORE-02, CORE-03, TOKN-01, TOKN-02, TOKN-03, PROF-01, PROF-05
**Success Criteria** (what must be TRUE):

  1. Developer can build the workspace with `cargo build` and all crates compile without errors
  2. Runtime loads an ONNX classifier model from a local path when given MODEL_ID env var and runs a warmup inference pass before accepting work
  3. Developer can pass a text string and receive a classification label with confidence score
  4. Tokenizer is validated against ONNX graph input spec at startup, rejecting mismatched tokenizer/model pairs
  5. All public traits expose Ousterhout-style deep module interfaces (Pipeline trait has 1-3 methods hiding tokenization, inference, and post-processing complexity)

**Plans:** 3/3 plans complete
Plans:
**Wave 1**

- [x] 01-01-PLAN.md — Workspace scaffold, Pipeline trait contracts, and failing E2E test (RED)

**Wave 2** *(blocked on Wave 1 completion)*

- [x] 01-02-PLAN.md — Core ClassifierPipeline implementation (GREEN)

**Wave 3** *(blocked on Wave 2 completion)*

- [x] 01-03-PLAN.md — Binary entry point with config and warmup

### Phase 2: HTTP Serving and Observability

**Goal**: Users can deploy Hephaestus as a Kubernetes pod and send HTTP requests for model inference with full production monitoring
**Mode:** mvp
**Depends on**: Phase 1
**Requirements**: API-01, API-02, API-03, API-04, CORE-04, OBSV-01, OBSV-02, OBSV-03
**Success Criteria** (what must be TRUE):

  1. User can send an HTTP POST with text input and receive a JSON classification result
  2. Kubernetes liveness probe responds immediately on startup; readiness probe gates on successful model load
  3. Service drains in-flight requests on SIGTERM before exiting; requests exceeding configured timeout return an error
  4. Prometheus scrapes a metrics endpoint showing inference latency histograms, request counts, and error rates; logs are structured JSON with request context (model ID, latency, status)
  5. Distributed traces propagate across the inference pipeline via OpenTelemetry

**Plans:** 4/4 plans complete
Plans:
**Wave 1**

- [x] 02-00-PLAN.md — Test scaffolding: crate skeleton and integration test stubs

**Wave 2** *(blocked on Wave 1 completion)*

- [x] 02-01-PLAN.md — HTTP inference endpoint with health probes, graceful shutdown, and request timeout

**Wave 3** *(blocked on Wave 2 completion)*

- [x] 02-02-PLAN.md — Prometheus metrics, deep-module StageTimer, conditional OTel tracing, structured logging

**Wave 4** *(gap closure)*

- [x] 02-03-PLAN.md — Gap closure: per-request structured log events (OBSV-02) and TraceLayer wiring

### Phase 3: Model Resolution

**Goal**: Users can specify a model name and the runtime automatically resolves ONNX files from S3 cache or HuggingFace, building the cache as models are discovered
**Mode:** mvp
**Depends on**: Phase 1
**Requirements**: RSLV-01, RSLV-02, RSLV-03, RSLV-04, RSLV-05
**Success Criteria** (what must be TRUE):

  1. Runtime loads ONNX model files from S3 cache when available, without contacting HuggingFace
  2. On S3 miss, runtime downloads ONNX exports from HuggingFace and caches them to S3 for future pods
  3. On both S3 and HuggingFace miss, runtime calls the Forge API for conversion (returns a clear error if Forge is unavailable)
  4. Developer configures only MODEL_ID; the 3-tier resolution chain is abstracted behind a single resolve() call

**Plans:** 2/2 plans complete
Plans:
**Wave 1**

- [x] 03-01-PLAN.md — HuggingFace resolution vertical slice: resolve crate + HF download + binary wiring

**Wave 2** *(blocked on Wave 1 completion)*

- [x] 03-02-PLAN.md — S3 cache tier, background cache-back upload, and Forge client stub

### Phase 4: Additional Profiles and Dynamic Batching

**Goal**: Users can serve multiple model types beyond classifiers and optionally enable request batching for throughput
**Mode:** mvp
**Depends on**: Phase 1, Phase 2
**Requirements**: PROF-02, PROF-03, PROF-04, BTCH-01, BTCH-02, BTCH-03
**Success Criteria** (what must be TRUE):

  1. User can deploy an embeddings model and receive L2-normalized float vectors from text input
  2. User can deploy a seq2seq model and receive generated text from text input
  3. User can deploy a token classifier model and receive per-token labels (NER/POS) from text input
  4. User can enable dynamic batching via configuration, collecting requests over a time window for batched inference
  5. Batching is disabled by default; when enabled, max batch size and max wait time are configurable per deployment

**Plans:** 4/4 plans complete
Plans:
**Wave 1**

- [x] 04-01-PLAN.md — Embeddings profile with PipelineKind enum dispatch and profile auto-detection

**Wave 2** *(blocked on Wave 1 completion)*

- [x] 04-02-PLAN.md — Seq2Seq and Token Classifier profiles completing all four model types

**Wave 3** *(blocked on Wave 2 completion)*

- [x] 04-03-PLAN.md — Dynamic batching with channel-based request collection

**Wave 4** *(gap closure)*

- [x] 04-04-PLAN.md — Gap closure: config validation, id2label contiguity, NER score averaging, defensive error handling

### Phase 5: Forge Conversion Service

**Goal**: Models without existing ONNX exports are automatically converted and cached by a persistent Python service, completing the full resolution chain
**Mode:** mvp
**Depends on**: Phase 3
**Requirements**: FORG-01, FORG-02, FORG-03, FORG-04
**Success Criteria** (what must be TRUE):

  1. Forge converts a HuggingFace model to ONNX format via optimum when called by Hephaestus
  2. Forge uploads converted ONNX files to S3 so future pods load from cache without reconversion
  3. Forge validates converted ONNX model integrity before uploading, rejecting corrupt conversions
  4. End-to-end: Hephaestus pod starts with a model that has no ONNX export, Forge converts it, and inference succeeds on the converted model

**Plans:** 2/2 plans complete
Plans:
**Wave 1** *(parallel — no file overlap)*

- [x] 05-01-PLAN.md — Forge conversion service: Python FastAPI app with optimum conversion, validation, S3 upload, queue, tests, and Dockerfile
- [x] 05-02-PLAN.md — Rust ForgeClient integration: HttpForgeClient with reqwest, ModelResolver generalization, binary wiring

## Progress

**Execution Order:**
Phases execute in numeric order: 1 -> 2 -> 3 -> 4 -> 5 -> 6 -> 7 -> 8

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. Core Inference Engine | 3/3 | Complete    | 2026-08-23 |
| 2. HTTP Serving and Observability | 4/4 | Complete   | 2026-08-24 |
| 3. Model Resolution | 2/2 | Complete    | 2026-08-26 |
| 4. Additional Profiles and Dynamic Batching | 4/4 | Complete   | 2026-08-26 |
| 5. Forge Conversion Service | 2/2 | Complete    | 2026-08-26 |
| 6. OpenDAL Storage Abstraction | 3/3 | Complete   | 2026-08-26 |
| 7. Production Hardening | 1/1 | Complete   | 2026-08-26 |
| 8. Inference Quality and Concurrency | 0/0 | Not started | — |

### Phase 6: OpenDAL Storage Abstraction

**Goal:** Replace aws-sdk-s3 with Apache OpenDAL for storage operations, enabling multi-cloud and local-fs backends via configuration
**Requirements**: STOR-01
**Depends on:** Phase 3
**Success Criteria** (what must be TRUE):

  1. Runtime resolves models from storage using OpenDAL Operator (S3, fs, or memory backend)
  2. STORAGE_TYPE=fs allows local development without S3 or localstack
  3. STORAGE_TYPE=none disables the storage tier; resolution starts at HuggingFace
  4. Forge uploads converted models via OpenDAL instead of boto3
  5. Both Rust and Python services share the same STORAGE_* env var naming convention
  6. aws-sdk-s3, aws-config, and boto3 are fully removed from dependencies

**Plans:** 3/3 plans complete
Plans:
**Wave 1** *(parallel -- no file overlap)*

- [x] 06-01-PLAN.md — Rust resolve crate: OpenDAL storage module and resolver migration
- [x] 06-03-PLAN.md — Forge Python: OpenDAL migration (storage, config, queue, tests)

**Wave 2** *(blocked on Wave 1 completion)*

- [x] 06-02-PLAN.md — Rust binary crate: Config STORAGE_* fields and Operator wiring

### Phase 7: Production Hardening

**Goal:** Fix production-readiness issues from code review: unlock BERT-family models, harden request handling, improve startup and shutdown resilience
**Depends on:** Phase 6
**Success Criteria** (what must be TRUE):

  1. BERT-family models (with token_type_ids) load and serve inference correctly
  2. Oversized request bodies are rejected before deserialization
  3. Warmup failure does not crash the pod
  4. Shutdown flushes logs and runs OTel cleanup before exiting
  5. HF download retry does not retry auth failures or 404s

**Plans:** 1/1 plans complete
Plans:
**Wave 1**

- [x] 07-01-PLAN.md — token_type_ids support, body limits, resilient warmup/shutdown, smart retry

### Phase 8: Inference Quality and Concurrency

**Goal:** Fix NER score normalization bug, split pipeline mutex to allow concurrent tokenization, and add real integration tests that download models and verify end-to-end inference
**Depends on:** Phase 7
**Success Criteria** (what must be TRUE):

  1. Token classifier pipeline returns softmax-normalized scores in [0,1] matching classifier pipeline behavior
  2. Tokenization runs concurrently across requests without holding the inference mutex
  3. Integration test suite downloads real models and verifies inference output for all four profiles (classifier, embeddings, seq2seq, token_classifier)
  4. Dead code in TokenClassifierPipeline::execute is removed

**Plans:** 3 plans

Plans:
**Wave 1** *(parallel -- no file overlap)*

- [ ] 08-01-PLAN.md — NER score normalization fix (softmax per-token) and dead code removal
- [ ] 08-02-PLAN.md — Pipeline mutex split: Mutex to RwLock for concurrent tokenization

**Wave 2** *(blocked on Wave 1 completion)*

- [ ] 08-03-PLAN.md — Integration tests with real HuggingFace models for classifier, embeddings, and token classifier
