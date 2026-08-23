# Feature Research

**Domain:** ONNX model inference runtime / model serving platform
**Researched:** 2026-08-22
**Confidence:** MEDIUM (cross-referenced across Triton, TorchServe, BentoML, Seldon Core, KServe, Mosec; web sources are LOW individually but converge strongly)

## Feature Landscape

### Table Stakes (Users Expect These)

Features that any credible inference runtime must have. Missing these means Hephaestus cannot replace existing Python runtimes.

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| ONNX model loading and inference | Core purpose of the runtime; every serving platform loads and runs models | MEDIUM | Via `ort` crate. Must handle session creation, input/output tensor binding, dynamic shapes. Foundation of everything else. |
| Model resolution from remote storage (S3, HuggingFace) | Production models live in registries/stores, not baked into images. Triton uses model repositories, BentoML uses Bento stores. | MEDIUM | 3-tier resolution (S3 cache, HF ONNX exports, Forge conversion) is already in PROJECT.md. `hf-hub` crate for HF, `aws-sdk-s3` for S3. |
| Tokenization and pre/post-processing | Callers send raw text/data, not tensors. Every serving platform (Triton ensemble pipelines, TorchServe custom handlers, BentoML custom logic) handles this. Without it, callers must do their own tensor prep -- defeating the "drop-in replacement" goal. | HIGH | Via `tokenizers` crate. Model type profiles (classifier, embeddings, seq2seq, etc.) encapsulate the full pipeline. This is the hardest table-stakes feature. |
| HTTP/REST API | Universal access pattern. Triton, TorchServe, BentoML, KServe all expose HTTP. Required for quick integrations and debugging. | MEDIUM | Via `axum`. Follow KServe V2 / Open Inference Protocol for endpoint structure (health, metadata, infer). |
| gRPC API | High-throughput internal communication. Triton, TorchServe, BentoML, KServe all expose gRPC. Required for production inter-service calls. | MEDIUM | Via `tonic`. Define proto service matching KServe V2 inference protocol. |
| Health checks (liveness + readiness) | Kubernetes requires these for orchestration. Every production serving platform exposes them. Liveness = "process alive", readiness = "model loaded and accepting requests". | LOW | Liveness: simple HTTP 200. Readiness: model loaded, warmup complete. Startup probe support for slow model loads. |
| Prometheus metrics | Standard observability. Triton, TorchServe, BentoML, Mosec all export Prometheus metrics. Ops teams expect to scrape inference latency, request count, error rates. | MEDIUM | Via `metrics` + `metrics-exporter-prometheus` crates. Key metrics: request latency (p50/p95/p99), inference latency, requests/sec, error rate, batch size distribution, queue depth. |
| Structured logging | Production debugging requires structured (JSON) logs with request IDs, model names, latency. Every mature serving platform has this. | LOW | Via `tracing` + `tracing-subscriber` with JSON formatter. |
| Graceful shutdown | Must drain in-flight requests before terminating. Triton, TorchServe, Mosec all implement this. Without it, rolling updates cause request failures. | LOW | Tokio shutdown signal handling. Stop accepting new requests, drain queue, then exit. |
| Model warmup | First inference is slow (lazy initialization, JIT compilation, memory allocation). Triton has explicit warmup config. Without warmup, readiness probe passes but first N requests are slow. | LOW | Run one or more dummy inferences at startup before marking ready. |
| CPU and GPU execution provider support | Triton and ONNX Runtime support multiple execution providers. Must run on CPU (dev/test, some workloads) and GPU (production throughput). | MEDIUM | `ort` supports CPU, CUDA, TensorRT, CoreML execution providers via feature flags. Selection via env var at startup. |
| Environment-based configuration | Kubernetes-native: config via env vars and ConfigMaps. One model per pod, model ID as env var. Triton uses config.pbtxt, but for single-model pods env vars are simpler. | LOW | `MODEL_ID`, `EXECUTION_PROVIDER`, `BATCH_MAX_SIZE`, `BATCH_TIMEOUT_MS`, etc. |
| Request timeout | Prevent stuck inferences from holding resources forever. Standard in all production servers. | LOW | Configurable per-request timeout. Return 504 on timeout. |

### Differentiators (Competitive Advantage)

Features that set Hephaestus apart from existing options. These align with the core value proposition of a single Rust binary replacing scattered Python runtimes.

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| Self-building S3 cache layer | Models are resolved once, cached to S3, and all future pods pull from cache. No manual ONNX export step. No model registry to manage. Triton requires pre-built model repos; BentoML requires pre-built Bentos. Hephaestus bootstraps itself. | MEDIUM | The 3-tier resolution flow (S3 -> HF -> Forge) with automatic cache-back to S3 is the key differentiator. Cold start is slow; warm start is fast. |
| Single Rust binary for all model types | Triton is a massive C++ server. TorchServe/BentoML are Python. Hephaestus is one small, fast binary that handles classifiers, embeddings, ASR/TTS, LLMs -- all via ONNX. Operational simplicity: one container image, one deployment pattern. | HIGH | Model type profiles (4-5 profiles) encapsulate different pre/post-processing pipelines. Adding a new model type = adding a new profile, not a new service. |
| Forge conversion service | No other serving platform auto-converts non-ONNX models. You must pre-export. The Forge handles PyTorch/TF -> ONNX conversion as a service, so teams can specify a HuggingFace model ID and Hephaestus figures out the rest. | HIGH | Separate Python service using `optimum`. Infrequent calls. Must handle conversion failures gracefully. |
| Configurable dynamic batching | Default single-request for low latency (most Minerva workloads). Opt-in batching for throughput when needed. Triton and BentoML have batching, but it's always-on or complex to tune. Hephaestus defaults to simple and lets you opt in. | HIGH | Collect requests for up to `BATCH_TIMEOUT_MS`, batch up to `BATCH_MAX_SIZE`, run single inference, scatter results. Requires careful async design. |
| Sub-millisecond overhead from Rust | Python serving frameworks add 5-50ms of framework overhead per request. Rust eliminates GIL contention, interpreter overhead, and GC pauses. For high-frequency internal calls (sentiment on every utterance), this matters. | LOW | Inherent to Rust. Not a feature to build, but a property to preserve -- avoid unnecessary allocations and copies in the hot path. |
| OpenTelemetry distributed tracing | Full trace context propagation from caller through Hephaestus to downstream services. BentoML has basic tracing; Triton has limited tracing. Proper OTel integration means inference latency shows up in the caller's trace. | MEDIUM | Via `opentelemetry` + `tracing-opentelemetry` crates. Propagate trace context from gRPC/HTTP headers. |

### Anti-Features (Deliberately NOT Building)

Features that seem appealing but create problems for Hephaestus's scope.

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|-----------------|-------------|
| Multi-model per pod | Triton does this. Seems efficient -- share GPU memory across models. | Massively increases complexity: model scheduling, memory management, fairness, isolation. For an internal platform with k8s, just scale pods. One model per pod is the k8s-native pattern. | Scale horizontally with one-model-per-pod. Use k8s HPA for autoscaling. |
| Model versioning / A/B testing | Seldon Core and TorchServe support this. Useful for ML experimentation. | Conflates model serving with experiment orchestration. Hephaestus serves one model; experiment routing belongs in the service mesh or a higher-level orchestrator. | Use Kubernetes Deployments with different model versions. Route traffic via Istio/Envoy or application-level routing. |
| Model marketplace / registry UI | BentoML has BentoCloud. Looks polished. | Hephaestus is internal infrastructure, not a product. A UI adds maintenance burden without value for a team that manages models via git and CI/CD. | S3 bucket with clear naming conventions. HuggingFace Hub for model discovery. |
| Custom Python/C++ backends | Triton supports arbitrary Python and C++ backends. Maximum flexibility. | Defeats the purpose of a single Rust binary. Adding Python execution to the inference path reintroduces all the problems Hephaestus eliminates. | Model type profiles in Rust cover the standard pre/post-processing patterns. Truly custom models that need Python logic stay in their own Python service. |
| Ensemble pipelines (multi-model chaining) | Triton and Seldon support DAG-based multi-model inference. Useful for complex pipelines. | Requires an internal scheduler, memory management between stages, and error handling across models. Way beyond scope for v1, and most Minerva workloads are single-model. | Chain at the application level -- call model A, then call model B. Simpler, more debuggable. |
| Auto-scaling logic | BentoML and Seldon build scaling into the framework. | Kubernetes HPA/KEDA already handles this based on metrics. Duplicating scaling logic inside the inference server is redundant and less flexible. | Export Prometheus metrics (queue depth, latency). Let k8s HPA/KEDA scale based on those metrics. |
| Streaming inference (SSE/WebSocket) | LLM serving platforms (vLLM, TGI) do token-by-token streaming. | Hephaestus v1 targets classifiers and embeddings -- fixed-output models. Streaming adds significant complexity (connection management, partial results). Only relevant for LLM text generation, which is a later expansion. | Defer to v2+ if LLM serving becomes a requirement. |
| Multi-tenant authentication / authorization | Triton Enterprise has auth. Needed for public APIs. | Hephaestus is an internal Minerva service behind the service mesh. Auth is handled at the ingress/mesh layer. Adding auth to the inference server is redundant. | Rely on network-level isolation (k8s network policies, service mesh mTLS). |
| Request/response logging for audit | Some platforms log every request/response for debugging. | For high-throughput inference, logging every request creates massive storage costs and performance overhead. | Sample-based logging at configurable rates. Full request logging as opt-in for debugging. |

## Feature Dependencies

```
Model Loading (ort Session)
    |
    +--requires--> Model Resolution (S3 / HF / Forge)
    |                  |
    |                  +--requires--> S3 Client (aws-sdk-s3)
    |                  +--requires--> HF Client (hf-hub)
    |                  +--optional--> Forge Client (gRPC/HTTP to Forge service)
    |
    +--requires--> Execution Provider Selection (CPU/CUDA/TensorRT)
    |
    +--enables--> Inference Engine (run model on input tensors)
                      |
                      +--requires--> Pre/Post-Processing Pipelines
                      |                  |
                      |                  +--requires--> Tokenization (tokenizers crate)
                      |                  +--requires--> Model Type Profiles
                      |
                      +--enables--> HTTP API (axum)
                      +--enables--> gRPC API (tonic)
                      |
                      +--optional--> Dynamic Batching
                      |                  |
                      |                  +--requires--> Request Queue
                      |                  +--requires--> Batch Collector (async timer + size limit)
                      |
                      +--optional--> Model Warmup (runs after load, before ready)

Health Checks --requires--> Model Loading (readiness depends on model state)
Metrics --independent-- (can instrument from the start)
Logging --independent-- (can instrument from the start)
OTel Tracing --requires--> HTTP/gRPC APIs (propagates context from headers)
Graceful Shutdown --requires--> HTTP/gRPC APIs + Request Queue
```

### Dependency Notes

- **Inference Engine requires Pre/Post-Processing:** Without tokenization and output decoding, the engine can only accept raw tensors -- useless for Minerva callers who send text/audio.
- **Dynamic Batching requires Inference Engine:** Batching collects multiple requests, combines input tensors, runs one inference, then scatters outputs. Cannot work without the base inference path.
- **Model Warmup requires Model Loading:** Warmup runs dummy inferences after the model is loaded but before readiness is signaled.
- **Health Checks (readiness) require Model Loading:** Readiness can only be true after the model is loaded and warmed up.
- **Forge is optional in the resolution chain:** Most HuggingFace models already have ONNX exports. Forge is only called when no ONNX exists anywhere. Can be deferred to a later phase.

## MVP Definition

### Launch With (v1)

Minimum viable product -- enough to replace one existing Python classifier service.

- [ ] **ONNX model loading via ort** -- Core runtime. Load a model from a local path into an ort Session.
- [ ] **S3 model resolution** -- Download ONNX files from S3. Skip HF and Forge for v1 MVP.
- [ ] **Classifier pre/post-processing profile** -- Tokenize text input, run inference, decode class probabilities to labels. One profile is enough to prove the pattern.
- [ ] **HTTP REST API** -- `/v2/models/{model}/infer` endpoint following Open Inference Protocol structure. Health and readiness endpoints.
- [ ] **Prometheus metrics** -- Request count, latency histogram, error counter. Scrape endpoint at `/metrics`.
- [ ] **Structured JSON logging** -- Request-scoped logs with model name, latency, status.
- [ ] **Health checks** -- Liveness at `/health/live`, readiness at `/health/ready`.
- [ ] **Model warmup** -- Run N dummy inferences before marking ready.
- [ ] **Graceful shutdown** -- Drain in-flight requests on SIGTERM.
- [ ] **Env-based configuration** -- `MODEL_ID`, `MODEL_PATH`, `EXECUTION_PROVIDER`, `PORT`.

### Add After Validation (v1.x)

Features to add once the core loop (load model, serve inference, report metrics) is proven.

- [ ] **gRPC API** -- Add once HTTP API is stable and at least one caller needs gRPC throughput.
- [ ] **HuggingFace model resolution** -- Extend resolution chain to download ONNX from HF Hub when not in S3.
- [ ] **S3 cache-back** -- After resolving from HF, upload to S3 so future pods get cache hits.
- [ ] **Embeddings model type profile** -- Second profile: text in, vector out. Proves the profile abstraction works for multiple model types.
- [ ] **GPU execution provider** -- Enable CUDA/TensorRT execution providers. Requires GPU-enabled container image.
- [ ] **Configurable dynamic batching** -- Opt-in batching with `BATCH_MAX_SIZE` and `BATCH_TIMEOUT_MS`.
- [ ] **OpenTelemetry tracing** -- Trace context propagation from caller through inference.
- [ ] **Request timeout** -- Configurable per-request timeout with 504 response.

### Future Consideration (v2+)

Features to defer until Hephaestus is running multiple model types in production.

- [ ] **Forge integration** -- Call the Forge service for PyTorch/TF to ONNX conversion. Only needed for models without existing ONNX exports.
- [ ] **Seq2seq / ASR / TTS profiles** -- Complex pre/post-processing (audio decode, beam search, vocoder). Defer until embeddings and classifiers are stable.
- [ ] **LLM profile with streaming** -- Token-by-token generation with SSE. Major complexity increase. Defer until there is a concrete Minerva use case.
- [ ] **Model quantization at load time** -- Apply INT8/FP16 quantization during model loading for smaller/faster inference. Research needed on accuracy tradeoffs.
- [ ] **IO binding for GPU** -- Pin input/output tensors in GPU memory to avoid CPU-GPU copies. Optimization for GPU-heavy workloads.

## Feature Prioritization Matrix

| Feature | User Value | Implementation Cost | Priority |
|---------|------------|---------------------|----------|
| ONNX model loading (ort Session) | HIGH | MEDIUM | P1 |
| S3 model resolution | HIGH | MEDIUM | P1 |
| Classifier pre/post-processing | HIGH | HIGH | P1 |
| HTTP REST API | HIGH | MEDIUM | P1 |
| Health checks (live + ready) | HIGH | LOW | P1 |
| Prometheus metrics | HIGH | MEDIUM | P1 |
| Structured logging | MEDIUM | LOW | P1 |
| Model warmup | MEDIUM | LOW | P1 |
| Graceful shutdown | MEDIUM | LOW | P1 |
| Env-based configuration | HIGH | LOW | P1 |
| gRPC API | HIGH | MEDIUM | P2 |
| HF model resolution | MEDIUM | MEDIUM | P2 |
| S3 cache-back | MEDIUM | LOW | P2 |
| Embeddings profile | HIGH | MEDIUM | P2 |
| GPU execution providers | HIGH | MEDIUM | P2 |
| Dynamic batching | MEDIUM | HIGH | P2 |
| OpenTelemetry tracing | MEDIUM | MEDIUM | P2 |
| Request timeout | MEDIUM | LOW | P2 |
| Forge integration | LOW | HIGH | P3 |
| Seq2seq/ASR/TTS profiles | MEDIUM | HIGH | P3 |
| LLM streaming | LOW | HIGH | P3 |
| Model quantization at load | LOW | MEDIUM | P3 |
| IO binding (GPU) | LOW | MEDIUM | P3 |

**Priority key:**
- P1: Must have for launch -- proves the concept, replaces one Python service
- P2: Should have -- expands to more model types and production hardening
- P3: Future consideration -- complex features for specialized use cases

## Competitor Feature Analysis

| Feature | Triton | TorchServe | BentoML | Seldon Core | Hephaestus Approach |
|---------|--------|------------|---------|-------------|---------------------|
| Multi-framework | TensorRT, ONNX, PyTorch, TF, OpenVINO, Python, C++ | PyTorch only | sklearn, TF, PyTorch, XGBoost, ONNX, etc. | Via runtimes (MLServer, Triton) | ONNX only -- convert everything else via Forge |
| Language | C++ | Java/Python | Python | Python/Go | Rust |
| API | HTTP + gRPC | HTTP + gRPC | HTTP + gRPC (FastAPI) | HTTP + gRPC | HTTP (axum) + gRPC (tonic) |
| Batching | Dynamic batching, always available | Request batching | Adaptive batching | Via runtime | Configurable, default off |
| Multi-model per instance | Yes, core feature | Yes | Yes | Yes | No -- one model per pod (k8s-native scaling) |
| Model versioning | Integer versions in repo | Version management | Bento versioning | Via experiments | No -- use k8s Deployments |
| Pre/post-processing | Ensemble pipelines or Python backend | Custom Python handlers | Custom Python logic | Custom containers | Rust model type profiles |
| Model warmup | Yes, configurable | Partial | No explicit warmup | No | Yes, dummy inference at startup |
| Auto-conversion | No | No | No | No | Yes, via Forge service |
| Container size | Large (>5GB with all backends) | Medium (~2GB) | Medium (~1GB+) | Varies by runtime | Small (<100MB binary + ONNX RT libs) |
| Startup time | Slow (multi-model load) | Medium | Medium | Slow (k8s operator) | Fast (single model, Rust binary) |
| Memory overhead | High (multi-model) | Medium (JVM + Python) | Medium (Python) | High (k8s operator + sidecars) | Low (Rust, single model) |

## Sources

- [NVIDIA Triton Inference Server docs](https://docs.nvidia.com/deeplearning/triton-inference-server/user-guide/docs/index.html) -- features, model management, dynamic batching, ensemble pipelines
- [Triton model management](https://docs.nvidia.com/deeplearning/triton-inference-server/user-guide/docs/user_guide/model_management.html) -- versioning, dynamic load/unload, polling
- [TorchServe documentation](https://docs.pytorch.org/serve/large_model_inference.html) -- large model support, handlers, parallelism
- [BentoML documentation](https://docs.bentoml.com/) -- framework support, adaptive batching, Bento packaging
- [BentoML GitHub](https://github.com/bentoml/BentoML) -- feature overview, multi-model pipelines
- [Seldon Core v2 features](https://docs.seldon.ai/seldon-core-2/about/core-features) -- experiments, A/B testing, explainability
- [KServe V2 / Open Inference Protocol](https://kserve.github.io/website/latest/modelserving/data_plane/v2_protocol/) -- standardized inference API
- [ONNX Runtime execution providers](https://onnxruntime.ai/docs/execution-providers/) -- CPU, CUDA, TensorRT, OpenVINO, CoreML
- [Model serving comparison](https://reintech.io/blog/bentoml-vs-seldon-core-vs-kserve-model-serving-framework-comparison) -- BentoML vs Seldon vs KServe
- [Top model serving frameworks](https://www.devopsschool.com/blog/top-10-ai-model-serving-frameworks-tools-in-2025-features-pros-cons-comparison/) -- cross-platform comparison
- [Triton vs TorchServe](https://algoroq.io/compare-tech/triton-vs-torchserve-inference/) -- head-to-head feature comparison
- [Mosec inference framework](https://github.com/mosecorg/mosec) -- dynamic batching, warmup, graceful shutdown patterns

---
*Feature research for: ONNX model inference runtime (Hephaestus)*
*Researched: 2026-08-22*
