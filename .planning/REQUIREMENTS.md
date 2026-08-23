# Requirements: Hephaestus

**Defined:** 2026-08-22
**Core Value:** A single Rust binary that takes a model name, resolves it to ONNX files, and serves inference with full pre/post-processing — replacing every per-model Python runtime in the cluster.

## v1 Requirements

Requirements for initial release. Each maps to roadmap phases.

### Core Inference

- [x] **CORE-01**: Runtime loads an ONNX model via `ort` Session and runs inference on CPU execution provider
- [x] **CORE-02**: Runtime reads model configuration from environment variables (MODEL_ID, EXECUTION_PROVIDER, S3_BUCKET, etc.)
- [x] **CORE-03**: Runtime runs a warmup inference pass after model load before accepting traffic
- [ ] **CORE-04**: Runtime enforces request timeouts to prevent runaway inference from blocking the server

### Model Profiles

- [x] **PROF-01**: Classifier profile tokenizes input text, runs inference, applies softmax, and returns label + confidence score
- [ ] **PROF-02**: Embeddings profile tokenizes input text, runs inference, applies L2 normalization, and returns a float vector
- [ ] **PROF-03**: Seq2seq profile tokenizes input text, runs inference, decodes output tokens, and returns generated text
- [ ] **PROF-04**: Token classifier profile tokenizes input text, runs inference, and returns per-token labels (NER, POS)
- [x] **PROF-05**: All profiles implement a single `Pipeline` trait with minimal interface (Ousterhout deep module pattern)

### Model Resolution

- [ ] **RSLV-01**: Runtime checks S3 cache for ONNX model files and loads from S3 if present
- [ ] **RSLV-02**: On S3 miss, runtime checks HuggingFace for existing ONNX exports via `hf-hub` and downloads if available
- [ ] **RSLV-03**: On HuggingFace miss, runtime calls the Forge service to convert the model to ONNX
- [ ] **RSLV-04**: After downloading from HuggingFace or receiving from Forge, runtime uploads ONNX files back to S3 for future pods
- [ ] **RSLV-05**: Model resolution exposes a single `resolve()` method that abstracts the 3-tier chain (Ousterhout deep module pattern)

### Tokenization

- [x] **TOKN-01**: Runtime loads tokenizer.json from HuggingFace or S3 cache alongside the ONNX model
- [x] **TOKN-02**: Runtime uses the `tokenizers` crate (HuggingFace Rust-native) for all text tokenization
- [x] **TOKN-03**: Runtime validates tokenizer output shape against ONNX graph input spec at startup

### API

- [ ] **API-01**: Runtime serves inference requests over HTTP REST (JSON request/response)
- [ ] **API-02**: Runtime exposes liveness probe endpoint that responds immediately on startup
- [ ] **API-03**: Runtime exposes readiness probe endpoint that gates on successful model load
- [ ] **API-04**: Runtime performs graceful shutdown on SIGTERM — drains in-flight requests before exiting

### Observability

- [ ] **OBSV-01**: Runtime exposes Prometheus metrics endpoint with inference latency histograms, request counts, and error rates
- [ ] **OBSV-02**: Runtime emits structured JSON logs with request context (model ID, latency, status)
- [ ] **OBSV-03**: Runtime integrates OpenTelemetry distributed tracing with span propagation across inference pipeline

### Dynamic Batching

- [ ] **BTCH-01**: Runtime supports configurable dynamic batching — collecting requests over a short window and running as a single inference call
- [ ] **BTCH-02**: Dynamic batching is disabled by default; enabled via configuration per deployment
- [ ] **BTCH-03**: Batching configuration includes max batch size and max wait time

### Forge Service

- [ ] **FORG-01**: Forge is a persistent Python service that converts HuggingFace models to ONNX format via `optimum`
- [ ] **FORG-02**: Forge uploads converted ONNX files to S3 after conversion
- [ ] **FORG-03**: Forge exposes an API that Hephaestus calls when S3 and HuggingFace both lack ONNX files
- [ ] **FORG-04**: Forge validates converted ONNX model integrity before uploading to S3

### Cross-Cutting

- [x] **XCUT-01**: All public traits follow Ousterhout deep module pattern — minimal interface (1-3 methods) hiding significant complexity
- [x] **XCUT-02**: Rust workspace with separate crates for proto, core, resolve, and API concerns
- [x] **XCUT-03**: All code adheres to rules in `rules/` directory

## v2 Requirements

Deferred to future release. Tracked but not in current roadmap.

### GPU Support

- **GPU-01**: CUDA and TensorRT execution provider support
- **GPU-02**: GPU container image variants (multi-stage Dockerfile)
- **GPU-03**: GPU memory monitoring and leak detection metrics
- **GPU-04**: EP availability check at startup — fail loudly if expected GPU EP is unavailable

### API Extensions

- **APIX-01**: gRPC API for high-throughput internal callers
- **APIX-02**: Streaming inference (SSE/WebSocket) for seq2seq models

### Additional Profiles

- **PRFX-01**: ASR profile (audio in, text out)
- **PRFX-02**: TTS profile (text in, audio out)
- **PRFX-03**: LLM profile (text in, streamed text out)

### Optimization

- **OPTM-01**: Model quantization at load time
- **OPTM-02**: IO binding for GPU memory optimization

## Out of Scope

| Feature | Reason |
|---------|--------|
| Multi-model per pod | Complexity not justified for internal use; k8s pod scaling is simpler |
| Custom model architectures outside ONNX | Hephaestus only runs ONNX graphs |
| EKS-specific features | Must work on any k8s cluster |
| Public API / multi-tenant auth | Internal Minerva service only |
| Model training or fine-tuning | Inference-only runtime |
| Model versioning / A/B testing | Handled at k8s deployment level, not runtime level |

## Traceability

| Requirement | Phase | Status |
|-------------|-------|--------|
| CORE-01 | Phase 1 | Complete |
| CORE-02 | Phase 1 | Complete |
| CORE-03 | Phase 1 | Complete |
| CORE-04 | Phase 2 | Pending |
| PROF-01 | Phase 1 | Complete |
| PROF-02 | Phase 4 | Pending |
| PROF-03 | Phase 4 | Pending |
| PROF-04 | Phase 4 | Pending |
| PROF-05 | Phase 1 | Complete |
| RSLV-01 | Phase 3 | Pending |
| RSLV-02 | Phase 3 | Pending |
| RSLV-03 | Phase 3 | Pending |
| RSLV-04 | Phase 3 | Pending |
| RSLV-05 | Phase 3 | Pending |
| TOKN-01 | Phase 1 | Complete |
| TOKN-02 | Phase 1 | Complete |
| TOKN-03 | Phase 1 | Complete |
| API-01 | Phase 2 | Pending |
| API-02 | Phase 2 | Pending |
| API-03 | Phase 2 | Pending |
| API-04 | Phase 2 | Pending |
| OBSV-01 | Phase 2 | Pending |
| OBSV-02 | Phase 2 | Pending |
| OBSV-03 | Phase 2 | Pending |
| BTCH-01 | Phase 4 | Pending |
| BTCH-02 | Phase 4 | Pending |
| BTCH-03 | Phase 4 | Pending |
| FORG-01 | Phase 5 | Pending |
| FORG-02 | Phase 5 | Pending |
| FORG-03 | Phase 5 | Pending |
| FORG-04 | Phase 5 | Pending |
| XCUT-01 | Phase 1 | Complete |
| XCUT-02 | Phase 1 | Complete |
| XCUT-03 | Phase 1 | Complete |

**Coverage:**

- v1 requirements: 34 total
- Mapped to phases: 34
- Unmapped: 0

---
*Requirements defined: 2026-08-22*
*Last updated: 2026-08-22 after roadmap creation*
