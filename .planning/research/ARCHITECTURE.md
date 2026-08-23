# Architecture Research

**Domain:** ONNX model inference runtime (Rust binary + Python conversion service)
**Researched:** 2026-08-22
**Confidence:** MEDIUM (cross-verified across docs.rs API docs, community examples, and NVIDIA reference architectures)

## Standard Architecture

### System Overview

```
┌──────────────────────────────────────────────────────────────────────────┐
│                          API Layer                                       │
│  ┌──────────────────┐  ┌──────────────────┐  ┌───────────────────────┐  │
│  │   gRPC Server    │  │   HTTP/REST API  │  │  Health / Metrics     │  │
│  │   (tonic)        │  │   (axum)         │  │  /healthz /readyz     │  │
│  │                  │  │                  │  │  /metrics (prom)      │  │
│  └────────┬─────────┘  └────────┬─────────┘  └───────────┬───────────┘  │
│           │                     │                        │              │
│           └─────────┬───────────┘                        │              │
│                     ↓                                    │              │
├──────────────────────────────────────────────────────────────────────────┤
│                       Inference Pipeline                                 │
│  ┌─────────────┐  ┌──────────────────┐  ┌──────────────────┐           │
│  │ Pre-Process  │→│  ONNX Session    │→│  Post-Process     │           │
│  │ (tokenize,   │  │  (ort crate)     │  │  (softmax, argmax│           │
│  │  normalize)  │  │  CPU/CUDA/TRT    │  │   label map)     │           │
│  └─────────────┘  └──────────────────┘  └──────────────────┘           │
│         ↑                   ↑                                           │
│         │                   │                                           │
│  ┌──────┴──────┐   ┌───────┴────────┐                                  │
│  │ Tokenizer   │   │ Model Profile  │                                  │
│  │ (tokenizers │   │ (classifier,   │                                  │
│  │  crate)     │   │  embeddings..) │                                  │
│  └─────────────┘   └────────────────┘                                  │
├──────────────────────────────────────────────────────────────────────────┤
│                       Model Resolution                                   │
│  ┌───────────┐  ┌────────────────┐  ┌──────────────────┐               │
│  │ S3 Cache  │→│  HuggingFace   │→│  Forge Service    │               │
│  │ (aws-sdk) │  │  (hf-hub)      │  │  (gRPC client)   │               │
│  └───────────┘  └────────────────┘  └──────────────────┘               │
│                                              ↕                          │
│                                     ┌────────────────┐                  │
│                                     │ Forge (Python)  │                  │
│                                     │ optimum export  │                  │
│                                     │ → S3 upload     │                  │
│                                     └────────────────┘                  │
└──────────────────────────────────────────────────────────────────────────┘
```

### Component Responsibilities

| Component | Responsibility | Implementation |
|-----------|----------------|----------------|
| **gRPC Server** | High-throughput internal inference API; streaming support for batch callers | `tonic` with generated service from `.proto` definitions |
| **HTTP/REST API** | Quick-integration REST endpoint; health/readiness probes; metrics | `axum` router with shared state, Prometheus metrics endpoint |
| **Inference Pipeline** | Orchestrate pre-process -> model -> post-process for each request | Trait-based pipeline with `ModelProfile` implementations |
| **Pre-Processor** | Tokenize text, normalize inputs, reshape tensors to model's expected format | `tokenizers` crate (from_file), ndarray for tensor shaping |
| **ONNX Session** | Load ONNX model, execute inference with configured execution provider | `ort` crate Session with CPU/CUDA/TensorRT EPs |
| **Post-Processor** | Convert raw model output to typed response (labels, scores, embeddings) | Per-profile logic: softmax for classifiers, normalize for embeddings |
| **Model Profile** | Encapsulate pre/post-processing logic for a model category (classifier, embeddings, etc.) | Enum or trait object dispatch; ~4-5 profiles total |
| **Model Resolver** | 3-tier resolution: S3 cache -> HuggingFace ONNX -> Forge conversion | Async chain with fallback; downloads to local disk then loads |
| **S3 Cache** | Store and retrieve ONNX model files and tokenizer configs | `aws-sdk-s3` with content-addressed paths |
| **HuggingFace Client** | Download ONNX models and tokenizer.json from HF Hub | `hf-hub` crate with env-based auth |
| **Forge Client** | Request ONNX conversion from the Python Forge service | gRPC client (tonic) calling Forge's convert RPC |
| **Forge Service** | Convert PyTorch/TF models to ONNX format, upload to S3 | Separate Python service using `optimum` library |
| **Health/Metrics** | Kubernetes probes, Prometheus metrics, OpenTelemetry tracing | `/healthz`, `/readyz` endpoints; `metrics` crate + `opentelemetry` |

## Recommended Project Structure

```
hephaestus/                      # Workspace root
├── Cargo.toml                   # [workspace] members, shared deps, lints
├── Cargo.lock
├── proto/                       # Protobuf definitions (shared)
│   ├── inference.proto          # Inference gRPC service definition
│   └── forge.proto              # Forge conversion service definition
│
├── crates/
│   ├── hephaestus/              # Main binary crate
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs          # Entry point: config, init, start servers
│   │       ├── config.rs        # Env-based configuration (MODEL_ID, EP, etc.)
│   │       └── server.rs        # Server lifecycle, graceful shutdown
│   │
│   ├── hephaestus-core/         # Core inference library
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs           # Public API re-exports
│   │       ├── engine.rs        # InferenceEngine: owns Session, runs inference
│   │       ├── pipeline.rs      # Pipeline trait + per-profile implementations
│   │       ├── profile/         # Model type profiles
│   │       │   ├── mod.rs
│   │       │   ├── classifier.rs
│   │       │   ├── embeddings.rs
│   │       │   ├── seq2seq.rs
│   │       │   └── token_classifier.rs
│   │       ├── tensor.rs        # Tensor construction/conversion helpers
│   │       └── error.rs         # Domain error types
│   │
│   ├── hephaestus-resolve/      # Model resolution library
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── resolver.rs      # 3-tier resolution chain
│   │       ├── s3.rs            # S3 cache operations
│   │       ├── huggingface.rs   # HuggingFace download
│   │       ├── forge.rs         # Forge gRPC client
│   │       └── manifest.rs      # Model manifest (what files, what profile)
│   │
│   ├── hephaestus-api/          # API layer (gRPC + HTTP)
│   │   ├── Cargo.toml
│   │   ├── build.rs             # tonic-build for proto compilation
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── grpc.rs          # tonic service implementation
│   │       ├── http.rs          # axum router + handlers
│   │       ├── health.rs        # Health/readiness probes
│   │       └── metrics.rs       # Prometheus metrics
│   │
│   └── hephaestus-proto/        # Generated protobuf code (build artifact)
│       ├── Cargo.toml
│       ├── build.rs
│       └── src/
│           └── lib.rs
│
├── forge/                       # Python Forge service (separate deployment)
│   ├── pyproject.toml
│   ├── Dockerfile
│   └── src/
│       ├── server.py            # gRPC server
│       ├── converter.py         # optimum ONNX export logic
│       └── uploader.py          # S3 upload after conversion
│
├── rules/                       # Rust code rules (already present)
├── .planning/                   # GSD planning artifacts
└── docker/
    ├── Dockerfile.hephaestus    # Multi-stage Rust build
    └── Dockerfile.forge         # Python Forge image
```

### Structure Rationale

- **`crates/hephaestus/` (binary):** Thin binary that wires together config, engine, and servers. Follows `proj-lib-main-split` rule — all logic lives in library crates, binary only orchestrates.
- **`crates/hephaestus-core/` (library):** The inference engine and pipeline logic. This is the heart — it owns the ort Session and model profiles. Separated so it can be tested without API concerns.
- **`crates/hephaestus-resolve/` (library):** Model resolution is a distinct concern with its own dependencies (aws-sdk-s3, hf-hub, tonic client). Isolating it keeps the core crate lean and testable with mock resolvers.
- **`crates/hephaestus-api/` (library):** API layer owns gRPC and HTTP implementations. Depends on core for the inference engine trait. Separating API from core means you can swap or extend protocols without touching inference logic.
- **`crates/hephaestus-proto/` (library):** Generated protobuf code in its own crate avoids recompilation cascades when proto files change. Both API and resolve crates depend on it.
- **`forge/` (Python):** Completely separate service. Not part of the Rust workspace. Has its own Dockerfile and deployment.
- **`proto/` (shared):** Proto files at workspace root, referenced by build scripts in both hephaestus-proto and forge.

## Architectural Patterns

### Pattern 1: Trait-Based Model Profiles

**What:** Define a `Pipeline` trait that each model type (classifier, embeddings, seq2seq) implements. The trait owns pre-processing, post-processing, and output type mapping. The inference engine dispatches to the right profile based on configuration.

**When to use:** Always. This is the core abstraction that makes one binary serve multiple model types without per-model code.

**Trade-offs:** Adds one level of indirection. Worth it because adding a new model type means implementing one trait, not forking the binary.

**Example:**

```rust
/// A model processing pipeline. Implementations define how raw input
/// becomes model-ready tensors and how raw output becomes typed responses.
pub trait Pipeline: Send + Sync {
    /// The input type this pipeline accepts from API callers.
    type Input;
    /// The output type this pipeline returns to API callers.
    type Output;

    /// Convert caller input into ONNX-ready tensors.
    fn pre_process(&self, input: &Self::Input) -> Result<Vec<Value>>;

    /// Convert raw ONNX output tensors into a typed response.
    fn post_process(&self, outputs: SessionOutputs<'_>) -> Result<Self::Output>;
}
```

### Pattern 2: Arc-Wrapped Engine with Mutex-Protected Session

**What:** The ort `Session::run()` requires `&mut self` because ONNX Runtime internals are not thread-safe. Wrap the Session in a `Mutex` inside an `Arc`-shared `InferenceEngine`. Both gRPC and HTTP handlers receive a clone of the Arc.

**When to use:** Always for the single-model-per-pod design. The Mutex serializes inference calls, which is correct — GPU inference is inherently serial on one model instance.

**Trade-offs:** Serialized inference means one request at a time per session. For GPU workloads this is natural (GPU executes one batch at a time). For CPU, you could create a pool of sessions, but start simple.

**Example:**

```rust
pub struct InferenceEngine {
    session: Mutex<Session>,
    pipeline: Box<dyn Pipeline<Input = InferenceRequest, Output = InferenceResponse>>,
}

impl InferenceEngine {
    pub async fn infer(&self, input: InferenceRequest) -> Result<InferenceResponse> {
        let tensors = self.pipeline.pre_process(&input)?;
        let outputs = {
            let mut session = self.session.lock().await;
            session.run(ort::inputs![tensors])?
        };
        self.pipeline.post_process(outputs)
    }
}
```

### Pattern 3: Tiered Model Resolution with Fallback Chain

**What:** Resolve a model ID to local ONNX files through a 3-tier chain: S3 cache (fast, authoritative) -> HuggingFace ONNX export (if exists) -> Forge conversion (slow, last resort). Each tier either returns a local path or falls through.

**When to use:** At pod startup, before the inference engine initializes. Resolution is a one-time cost.

**Trade-offs:** Forge conversion can take minutes. This is acceptable because it happens once per unique model, and the result is cached to S3 for all future pods. The startup latency is bounded to "S3 download time" for all subsequent deployments.

**Example:**

```rust
pub async fn resolve(model_id: &ModelId, config: &ResolveConfig) -> Result<ModelArtifacts> {
    // Tier 1: S3 cache
    if let Some(artifacts) = s3::try_fetch(model_id, config).await? {
        return Ok(artifacts);
    }

    // Tier 2: HuggingFace ONNX export
    if let Some(artifacts) = huggingface::try_fetch(model_id, config).await? {
        s3::upload(&artifacts, config).await?; // Cache for next time
        return Ok(artifacts);
    }

    // Tier 3: Forge conversion
    let artifacts = forge::convert(model_id, config).await?;
    // Forge uploads to S3 directly
    Ok(artifacts)
}
```

### Pattern 4: Content-Type-Based Protocol Routing

**What:** Host gRPC and HTTP on the same port by inspecting the `content-type` header. Requests with `application/grpc` route to tonic; everything else routes to axum.

**When to use:** When you want a single port per pod (simpler k8s Service definitions). The `axum_tonic` crate or manual hyper service switching supports this.

**Trade-offs:** Adds routing complexity. Alternatively, use two ports (simpler code, slightly more k8s config). Two ports is the safer starting choice — co-hosting can be added later as an optimization.

**Recommendation:** Start with two ports (e.g., 50051 for gRPC, 8080 for HTTP). Merge later if needed.

### Pattern 5: Configurable Dynamic Batching via Channel

**What:** For throughput-oriented workloads, accumulate incoming requests in a bounded channel with a short timer (e.g., 5ms). When the batch is full or the timer fires, run inference on the batch. Default to single-request mode (batch size 1, no timer) for lowest latency.

**When to use:** Opt-in via configuration. Default off. Enable when GPU utilization is low due to many small requests.

**Trade-offs:** Adds latency (up to timer duration) in exchange for throughput. Complexity in managing request-response correlation. Not needed for v1 classifiers — add when scaling embeddings or high-throughput workloads.

## Data Flow

### Startup Flow (Pod Initialization)

```
ENV: MODEL_ID=org/model-name
    ↓
[Config] → parse MODEL_ID, EP type, batch settings
    ↓
[Model Resolver] → S3 cache check
    ↓ (miss)
[Model Resolver] → HuggingFace ONNX check
    ↓ (miss)
[Model Resolver] → Forge gRPC: convert(model_id)
    ↓
[Forge Service] → optimum.export() → upload to S3
    ↓
[Model Resolver] → download from S3 → local disk
    ↓
[InferenceEngine] → load tokenizer.json → init Tokenizer
                  → load model.onnx → Session::builder()
                      .with_execution_providers([CUDA, CPU])
                      .commit_from_file()
                  → select Pipeline profile (classifier, etc.)
    ↓
[Servers] → start gRPC on :50051, HTTP on :8080
         → readiness probe → READY
```

### Inference Flow (Per Request)

```
[Client] → gRPC InferRequest { model_id, inputs: [text] }
    ↓
[gRPC Handler] → extract input, validate
    ↓
[InferenceEngine.infer()]
    ↓
[Pipeline.pre_process()]
    → Tokenizer.encode(text, add_special_tokens=true)
    → shape input_ids, attention_mask as tensors
    ↓
[Session.run(inputs![input_ids, attention_mask])]
    → ONNX Runtime executes graph on configured EP
    ↓
[Pipeline.post_process()]
    → extract logits tensor
    → softmax → argmax → label lookup
    ↓
[gRPC Handler] → InferResponse { label, confidence, latency_ms }
    ↓
[Client]
```

### Key Data Flows

1. **Model resolution** is a one-time startup cost. Once resolved, the ONNX files live on local disk for the pod's lifetime. The Session loads from local files, not network.
2. **Inference is synchronous within the Mutex.** One request executes at a time per Session. GPU naturally serializes anyway. The async layer (tokio) handles concurrent request arrival — requests queue at the Mutex.
3. **Tokenizer is stateless and thread-safe.** Unlike Session, the tokenizers crate's Tokenizer can be shared via Arc without a Mutex. Pre-processing can happen concurrently even while one inference is running.
4. **Forge is fire-and-forget from Hephaestus's perspective.** Hephaestus asks Forge to convert, Forge uploads to S3, Hephaestus downloads from S3. Hephaestus never receives model bytes over gRPC.

## Scaling Considerations

| Scale | Architecture Adjustments |
|-------|--------------------------|
| 1-10 pods per model | Default design. One model per pod, k8s HPA scales replicas. Mutex serialization is fine — each pod handles ~50-200 req/s for classifiers. |
| 10-50 pods per model | S3 cache ensures fast cold starts. Consider pre-warming: resolve models at image build time or init container. |
| High-throughput (>1K req/s) | Enable dynamic batching. Pool multiple Sessions on multi-GPU nodes. At this scale, consider dedicated embedding pods vs classifier pods. |
| Many models (>20 unique) | Forge conversion queue becomes a bottleneck if many new models deploy simultaneously. Add concurrency limits or a queue to Forge. S3 cache amortizes this over time. |

### Scaling Priorities

1. **First bottleneck: Mutex contention on Session.** If latency degrades under load, scale horizontally (more pods) rather than trying to parallelize within a pod. GPU inference is inherently serial per model instance.
2. **Second bottleneck: Model resolution cold start.** If pods cycle frequently, pre-cache models in the container image or use an init container that resolves before the main process starts.
3. **Third bottleneck: Tokenizer throughput.** Unlikely for classifiers (tokenization is fast), but for batch embedding workloads, Rayon parallelism in the tokenizers crate handles this. Configure `RAYON_RS_NUM_THREADS`.

## Anti-Patterns

### Anti-Pattern 1: Sharing a Single Session Across Threads Without Synchronization

**What people do:** Wrap Session in `Arc` without `Mutex`, hoping concurrent reads are safe.
**Why it's wrong:** `Session::run()` takes `&mut self`. ONNX Runtime's internal allocators and statistics trackers are not thread-safe. This causes undefined behavior or panics.
**Do this instead:** Use `Arc<Mutex<Session>>` (tokio Mutex for async). Accept serialized inference — it matches GPU execution semantics anyway.

### Anti-Pattern 2: Downloading Models on Every Inference Request

**What people do:** Resolve model files lazily per request, or re-download on cache miss during serving.
**Why it's wrong:** Model loading takes seconds to minutes. Doing it on the request path causes timeouts and unpredictable latency.
**Do this instead:** Resolve and load the model at startup. The pod is not ready until the model is loaded. Use k8s readiness probes to gate traffic.

### Anti-Pattern 3: Hardcoding Pre/Post-Processing Per Model

**What people do:** Write model-specific pre/post-processing in the request handler, creating a new code path for every model.
**Why it's wrong:** Unmaintainable. N models = N code paths with duplicated tokenization, tensor shaping, output parsing.
**Do this instead:** Use model profiles (Pipeline trait). ~4-5 profiles cover the common architectures. A classifier profile handles all BERT-like classifiers, not just one specific model.

### Anti-Pattern 4: Using `Tokenizer::from_pretrained()` in Production

**What people do:** Call `from_pretrained()` which downloads tokenizer files from HuggingFace at runtime.
**Why it's wrong:** Adds network dependency to the inference path. HuggingFace rate limits apply. Fails if HF is down.
**Do this instead:** Download tokenizer.json during model resolution (alongside the ONNX file), cache to S3, load from local disk via `Tokenizer::from_file()`.

### Anti-Pattern 5: Premature Dynamic Batching

**What people do:** Build complex batching infrastructure before validating that single-request latency meets requirements.
**Why it's wrong:** Batching adds latency (timer delay), complexity (request correlation, padding, error handling), and is only beneficial when GPU utilization is the bottleneck. For classifiers doing <10ms inference, batching adds overhead without benefit.
**Do this instead:** Start with batch_size=1 (default). Measure GPU utilization under load. Add batching only when GPU util is low and throughput is the constraint, not latency.

## Integration Points

### External Services

| Service | Integration Pattern | Notes |
|---------|---------------------|-------|
| **S3** | `aws-sdk-s3` async client for model file download/upload | Content-addressed paths: `s3://bucket/models/{org}/{name}/{revision}/model.onnx`. IAM auth via IRSA in k8s. |
| **HuggingFace Hub** | `hf-hub` crate with `HF_TOKEN` env var for auth | Content-addressed local cache. Handles rate limiting via error types. Only used during model resolution (startup). |
| **Forge Service** | gRPC client (tonic) calling Forge's `Convert` RPC | Forge is a separate deployment. Hephaestus sends model_id, Forge returns S3 path of converted model. Timeout should be generous (10+ minutes for large models). |
| **Prometheus** | `/metrics` endpoint via `prometheus` or `metrics` crate | Standard histograms: inference_duration_seconds, request_count, model_load_duration_seconds |
| **OpenTelemetry** | `tracing` + `opentelemetry` crate for distributed tracing | Trace spans for: model_resolution, pre_process, inference, post_process |
| **Kubernetes** | Liveness (`/healthz`) and readiness (`/readyz`) probes | Readiness flips to ready only after model is loaded and a warm-up inference succeeds |

### Internal Boundaries

| Boundary | Communication | Notes |
|----------|---------------|-------|
| **API -> Core** | Rust trait (`InferenceEngine` trait with `infer()` method) | API crate depends on core crate. Shared via `Arc<dyn InferenceEngine>`. |
| **Core -> Resolve** | Rust function call at startup only | Core calls `resolve::resolve()` once during init. After that, no interaction. |
| **Core -> ort** | `Session` struct owned by engine, `inputs!` macro for tensor construction | Direct dependency. Session lifetime = engine lifetime. |
| **Core -> tokenizers** | `Tokenizer` struct owned by pipeline, `encode()`/`decode()` calls | Direct dependency. Thread-safe, no Mutex needed. |
| **API gRPC <-> Forge gRPC** | gRPC over k8s internal DNS (`forge-service:50052`) | Only during model resolution. Forge is a separate pod. |
| **Hephaestus <-> S3** | HTTPS via aws-sdk-s3 | Model files are ~100MB-2GB. Download to `/tmp/models/` on local disk. |

## Build Order (Dependency Chain)

The components have clear dependency ordering that should guide phase structure:

```
1. hephaestus-proto      (no deps — just protobuf codegen)
2. hephaestus-core       (depends on: ort, tokenizers, ndarray)
3. hephaestus-resolve    (depends on: proto, aws-sdk-s3, hf-hub)
4. hephaestus-api        (depends on: proto, core)
5. hephaestus (binary)   (depends on: core, resolve, api)
6. forge (Python)         (independent — can be built in parallel with 1-4)
```

**Build order rationale:**
- Proto first because both API and resolve depend on generated types.
- Core next because it contains the Pipeline trait that everything else implements against.
- Resolve can be built after proto (uses Forge proto types) but is independent of core.
- API depends on core's InferenceEngine trait.
- Binary wires everything together — must be last.
- Forge is fully independent (Python, separate repo/deployment) and can proceed in parallel.

## Sources

- [ort crate API docs (docs.rs)](https://docs.rs/ort/latest/ort/) — Session, SessionBuilder, ExecutionProvider APIs (MEDIUM confidence — official docs.rs)
- [ort Session struct docs](https://docs.rs/ort/latest/ort/session/struct.Session.html) — run() signature, &mut self requirement, IoBinding (MEDIUM confidence)
- [ort getting started (ort.pyke.io)](https://ort.pyke.io/) — SessionBuilder example with execution providers (MEDIUM confidence — blocked by 403, verified via search results)
- [hf-hub crate API docs (docs.rs)](https://docs.rs/hf-hub/latest/hf_hub/) — HFClient, download_file, caching, error types (MEDIUM confidence)
- [tokenizers crate API docs (docs.rs)](https://docs.rs/tokenizers/latest/tokenizers/) — 4-stage pipeline, encode/decode, Rayon parallelism (MEDIUM confidence)
- [orp (ONNX Runtime Pipelines)](https://github.com/fbilhaut/orp) — composable pre/post-processing pipeline pattern (MEDIUM confidence)
- [Axum + Tonic co-hosting](https://github.com/sunsided/http-grpc-cohosting) — content-type routing pattern (MEDIUM confidence)
- [NVIDIA Triton gRPC service.proto](https://docs.nvidia.com/deeplearning/triton-inference-server/archives/triton_inference_server_1120/triton-inference-server-guide/docs/protobuf_api/grpc_service.proto.html) — reference gRPC inference API design (MEDIUM confidence)
- [NVIDIA TensorRT EP docs](https://onnxruntime.ai/docs/execution-providers/TensorRT-ExecutionProvider.html) — EP fallback chain, FP16/INT8 config (MEDIUM confidence)
- [Rust ONNX inference architecture (calmops.com)](https://calmops.com/programming/rust/real-time-ml-model-development-with-rust-and-onnx-runtime/) — Arc-wrapped session, axum State pattern (MEDIUM confidence)
- [Dynamic batching patterns (Redis blog)](https://redis.io/blog/dynamic-batching-guide/) — timer+max_batch_size approach (MEDIUM confidence)

---
*Architecture research for: Hephaestus ONNX inference runtime*
*Researched: 2026-08-22*
