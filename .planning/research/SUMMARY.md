# Project Research Summary

**Project:** Blacksmith (Hephaestus ONNX Model Inference Runtime)
**Domain:** ML model inference serving (Rust binary with ONNX Runtime)
**Researched:** 2026-08-22
**Confidence:** MEDIUM

## Executive Summary

Hephaestus is a Rust-based ONNX inference runtime that replaces scattered Python model-serving services with a single binary. The approach is well-grounded: `ort` (pykeio's ONNX Runtime bindings) is the only serious Rust option and is production-viable at v2.0.0-rc.13, HuggingFace's `tokenizers` crate is the reference Rust tokenizer implementation, and the serving stack (tonic + axum + tower) is the canonical Rust gRPC/HTTP combination. The one-model-per-pod design aligns with Kubernetes-native scaling and sidesteps the multi-model complexity that makes Triton heavy. The self-building S3 cache layer -- where models are resolved once from HuggingFace, cached to S3, and served to all future pods -- is the core differentiator over existing platforms that require pre-built model repositories.

The recommended approach is to build bottom-up: workspace scaffolding and proto codegen first, then the core inference engine with a single classifier profile, then model resolution (S3 only for MVP), then the HTTP API with health/metrics. gRPC, HuggingFace resolution, GPU execution providers, and dynamic batching are all v1.x additions after the core loop is proven. The Forge Python conversion service is fully independent and can be built in parallel or deferred.

The primary risks are GPU-related: silent EP fallback to CPU (no error, just 10-100x slower), CUDA/cuDNN/driver version matrix hell in containers, and GPU memory that never releases after peak usage. All three are well-documented with clear mitigations. The secondary risk is tokenizer/model input shape mismatch causing silently wrong results -- this requires integration tests against Python reference outputs for every model profile.

## Key Findings

### Recommended Stack

The stack is pure Rust except for the Forge conversion service (Python/optimum). All crates are verified on crates.io with current versions. The `ort` crate must be pinned exactly (`=2.0.0-rc.13`) because ort and ort-sys versions must match. Everything else uses caret ranges.

**Core technologies:**
- **ort 2.0.0-rc.13**: ONNX Runtime Rust bindings -- the only viable option; wraps ORT 1.28; `Session::run()` requires `&mut self` so needs Mutex wrapping
- **tokenizers 0.23**: HuggingFace's Rust-native tokenizer -- reference implementation, must match training tokenizer exactly
- **tonic 0.14 + axum 0.8**: gRPC and HTTP serving -- both Tower-native, share middleware; maintained by Tokio team (axum) and Hyperium (tonic)
- **hf-hub 1.0**: HuggingFace model downloads -- official client, 1.0 stable, content-addressed cache
- **aws-sdk-s3 1.143**: S3 model cache -- official AWS SDK, IRSA-native for k8s
- **tracing + metrics**: Observability split -- tracing for logs/traces, metrics crate for Prometheus (avoids deprecated opentelemetry-prometheus)

### Expected Features

**Must have (table stakes):**
- ONNX model loading and inference via ort Session
- S3 model resolution (download ONNX from S3 cache)
- Classifier pre/post-processing profile (tokenize, infer, softmax/argmax)
- HTTP REST API following Open Inference Protocol structure
- Health checks (liveness + readiness with model-loaded gating)
- Prometheus metrics (latency, request count, error rate)
- Structured JSON logging, model warmup, graceful shutdown
- Environment-based configuration (MODEL_ID, EXECUTION_PROVIDER, etc.)

**Should have (differentiators):**
- Self-building S3 cache with HuggingFace fallback and cache-back
- Single binary serving all model types via profile abstraction
- gRPC API for high-throughput internal callers
- GPU execution providers (CUDA, TensorRT)
- Configurable dynamic batching (default off)
- OpenTelemetry distributed tracing

**Defer (v2+):**
- Forge conversion service integration
- Seq2seq, ASR, TTS, LLM model profiles
- Streaming inference (SSE/WebSocket)
- Model quantization at load time, IO binding

### Architecture Approach

Four-crate Rust workspace: `hephaestus` (binary), `hephaestus-core` (inference engine + Pipeline trait), `hephaestus-resolve` (S3/HF/Forge resolution), `hephaestus-api` (gRPC + HTTP). Plus a `hephaestus-proto` crate for generated protobuf code and a separate `forge/` Python service. The core abstraction is a `Pipeline` trait that each model type implements (pre-process, post-process), with the inference engine wrapping an `Arc<Mutex<Session>>` for thread-safe access.

**Major components:**
1. **InferenceEngine** -- owns ort Session (Mutex-wrapped) and Pipeline; orchestrates pre-process -> infer -> post-process
2. **Model Resolver** -- 3-tier fallback chain (S3 cache -> HuggingFace ONNX -> Forge conversion); runs once at startup
3. **API Layer** -- dual-protocol serving (axum HTTP + tonic gRPC) with shared Tower middleware; health probes gate on model readiness
4. **Model Profiles** -- trait implementations per model category (classifier, embeddings, seq2seq); encapsulate tokenization and output decoding

### Critical Pitfalls

1. **Silent GPU fallback to CPU** -- check `CUDAExecutionProvider::is_available()` at startup; fail loudly if expected EP is missing
2. **CUDA/cuDNN/driver version matrix** -- pin all four versions (ort, ORT binary, CUDA, driver) in Dockerfile; test exact container on exact hardware
3. **GPU memory never released** -- one-model-per-pod mitigates; set k8s memory limits to known peak; replace pods instead of hot-reloading sessions
4. **Model loading kills readiness probes** -- start health endpoint before model load; use k8s startupProbe with 300s budget
5. **Tokenizer/model input shape mismatch** -- validate tokenizer output against ONNX graph input spec at startup; integration test against Python reference

## Implications for Roadmap

### Phase 1: Workspace Scaffolding and Proto Codegen
**Rationale:** All crates depend on workspace structure and proto types. Build order demands this first.
**Delivers:** Cargo workspace with 4 crates, proto definitions, build scripts, CI skeleton.
**Addresses:** Environment-based configuration (clap), project structure.
**Avoids:** Recompilation cascades from proto changes (isolated proto crate).

### Phase 2: Core Inference Engine
**Rationale:** The Pipeline trait and Session management are the foundation everything else builds on. Dependency chain: core before API, core before resolve integration.
**Delivers:** InferenceEngine with Mutex-wrapped Session, classifier Pipeline profile, tensor helpers, model warmup.
**Addresses:** ONNX model loading, classifier pre/post-processing, model warmup, EP availability check.
**Avoids:** Silent CPU fallback (EP check), tokenizer/model mismatch (startup validation), thread oversubscription (explicit thread count from cgroup).

### Phase 3: Model Resolution (S3)
**Rationale:** Models must be loadable from S3 before the runtime is useful beyond local dev. S3-only for MVP; HF fallback is v1.x.
**Delivers:** S3 download, local disk caching, model manifest parsing.
**Addresses:** S3 model resolution, env-based MODEL_ID configuration.
**Avoids:** HF rate limiting (S3-first architecture), downloading models per request (startup-only resolution).

### Phase 4: HTTP API and Observability
**Rationale:** Depends on core engine (Phase 2) and model resolution (Phase 3). This makes the runtime externally usable.
**Delivers:** REST endpoints (/v2/models/{model}/infer, health, readiness, metrics), structured logging, Prometheus scrape endpoint.
**Addresses:** HTTP REST API, health checks, Prometheus metrics, structured logging, graceful shutdown, request timeout.
**Avoids:** Cold start probe kills (health endpoint starts before model load, startupProbe config).

### Phase 5: gRPC API and HuggingFace Resolution
**Rationale:** gRPC adds throughput for internal callers; HF resolution completes the 3-tier chain. Both are v1.x features after HTTP API is proven.
**Delivers:** tonic gRPC inference service, HF model download with cache-back to S3, authenticated downloads.
**Addresses:** gRPC API, HF model resolution, S3 cache-back.
**Avoids:** HF rate limiting (authenticated token, S3-first fallback).

### Phase 6: GPU Execution Providers and Container Images
**Rationale:** GPU support requires solving the CUDA version matrix and building GPU container images. Deferred until CPU inference is proven.
**Delivers:** CUDA and TensorRT EP support, multi-stage Dockerfiles (CPU + GPU), version compatibility matrix.
**Addresses:** GPU EP support, container build strategy.
**Avoids:** CUDA version matrix hell (pinned versions), GPU memory leaks (monitoring + pod replacement), thread oversubscription (cgroup-aware thread count).

### Phase 7: Embeddings Profile and Production Hardening
**Rationale:** Second model profile proves the Pipeline abstraction generalizes. Production hardening addresses long-running stability.
**Delivers:** Embeddings profile (text in, vector out), OTel tracing, GPU memory monitoring, soak testing.
**Addresses:** Embeddings profile, OpenTelemetry tracing, GPU memory monitoring.
**Avoids:** GPU memory leak (24h soak test with monitoring).

### Phase 8: Dynamic Batching (Optional)
**Rationale:** Only implement after profiling shows GPU utilization is low on single requests. Default off.
**Delivers:** Configurable batch collection with timer + size limit, result scattering.
**Addresses:** Configurable dynamic batching.
**Avoids:** Premature batching latency (data-driven decision to enable).

### Phase Ordering Rationale

- Phases 1-4 follow the strict dependency chain from ARCHITECTURE.md: proto -> core -> resolve -> API
- S3 resolution before HF resolution because S3 is the primary path and HF is fallback
- HTTP before gRPC because HTTP is simpler to debug and test; gRPC adds throughput later
- GPU deferred to Phase 6 because CPU inference proves correctness first; GPU adds version matrix complexity
- Batching is last because research strongly warns against premature batching for small models

### Research Flags

Phases likely needing deeper research during planning:
- **Phase 2 (Core Engine):** ort v2 API is pre-release; Session builder patterns and EP registration need careful validation against current docs
- **Phase 6 (GPU):** CUDA/cuDNN/TensorRT version matrix requires testing on actual hardware; docs alone are insufficient
- **Phase 8 (Batching):** Implementation patterns are well-documented but tuning is workload-specific; needs profiling data

Phases with standard patterns (skip research-phase):
- **Phase 1 (Scaffolding):** Standard Cargo workspace + tonic-build; well-documented
- **Phase 3 (S3 Resolution):** aws-sdk-s3 is stable 1.x with clear API
- **Phase 4 (HTTP API):** axum is mature with extensive examples; health/metrics patterns are standard

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | MEDIUM | All crate versions verified on crates.io; ort is pre-release but the only option |
| Features | MEDIUM | Cross-referenced across 6 inference platforms; strong convergence on table stakes |
| Architecture | MEDIUM | Patterns verified across docs.rs, community examples, and reference implementations |
| Pitfalls | MEDIUM | GPU issues backed by multiple ONNX Runtime GitHub issues; threading docs from official ORT |

**Overall confidence:** MEDIUM

### Gaps to Address

- **ort v2 stability:** Pre-release crate; API may change before 2.0 stable. Pin exactly and monitor releases.
- **TensorRT engine caching:** Unclear how TensorRT compiled engine caching interacts with the S3 cache layer. Needs investigation in Phase 6.
- **Tokenizer configuration per model:** Each ONNX model may need different tokenizer settings (max_length, padding strategy). The manifest/config format for this is not yet defined.
- **Forge service design:** Conversion service is deferred but its gRPC contract should be defined early (Phase 1 proto) to avoid rework.
- **Multi-architecture container builds:** ARM64 support for Apple Silicon dev vs AMD64 for production not addressed in research.

## Sources

### Primary (HIGH confidence)
- [ort crate on crates.io](https://crates.io/crates/ort) -- version verification, feature flags
- [ONNX Runtime official docs](https://onnxruntime.ai/docs/) -- EP configuration, threading, memory management
- [Kubernetes probes docs](https://kubernetes.io/docs/tasks/configure-pod-container/configure-liveness-readiness-startup-probes/) -- startup/readiness probe patterns

### Secondary (MEDIUM confidence)
- [ort pykeio GitHub](https://github.com/pykeio/ort) -- Session API, execution provider patterns
- [tonic](https://crates.io/crates/tonic), [axum](https://crates.io/crates/axum), [hf-hub](https://crates.io/crates/hf-hub) -- crates.io version verification
- [microsoft/onnxruntime GitHub issues](https://github.com/microsoft/onnxruntime/issues) -- GPU memory leak documentation (#26831, #24376, #25996)
- [NVIDIA Triton docs](https://docs.nvidia.com/deeplearning/triton-inference-server/) -- feature comparison baseline
- [KServe V2 / Open Inference Protocol](https://kserve.github.io/website/latest/modelserving/data_plane/v2_protocol/) -- API structure reference

### Tertiary (LOW confidence)
- Community blog posts on Rust + ONNX inference patterns -- architectural validation
- Dynamic batching guides (Redis, Baseten) -- batching tradeoff analysis

---
*Research completed: 2026-08-22*
*Ready for roadmap: yes*
