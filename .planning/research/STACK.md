# Technology Stack

**Project:** Hephaestus (ONNX Model Inference Runtime)
**Researched:** 2026-08-22
**Overall Confidence:** MEDIUM — versions verified via crates.io API; architectural patterns cross-referenced across multiple production implementations

## Recommended Stack

### Async Runtime & Core

| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| tokio | 1.53 | Async runtime | The Rust async runtime. Everything else in this stack depends on it. Use features `full` during development, narrow to `rt-multi-thread,macros,signal,time,sync,fs,io-util` for production builds. |
| serde | 1.0 | Serialization | De facto standard. Needed for config, API payloads, model metadata. |
| serde_json | 1.0 | JSON | REST API request/response bodies, model config files. |
| thiserror | 2.0 | Library errors | Derive-based error types for internal crate boundaries. Use in library code, not application-level. |
| anyhow | 1.0 | Application errors | Context-rich error propagation at the binary level. Use in `main()` and CLI, not in library traits. |
| clap | 4.6 | CLI / config | Parse CLI args and env vars. Use `derive` feature for struct-based config. Env var fallback via `env` attribute handles the "one model per pod via env" requirement. |

### ONNX Inference

| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| ort | 2.0.0-rc.13 | ONNX Runtime Rust bindings | The only serious Rust binding for ONNX Runtime. Wraps ONNX Runtime 1.28. Pre-release but actively developed by pykeio, used in production by multiple teams. No stable alternative exists — this is the path. |
| ort-sys | 2.0.0-rc.13 | ONNX Runtime C bindings | Transitive dependency of `ort`. Version must match `ort` exactly. `download-binaries` feature (default) pulls prebuilt ONNX Runtime libraries. |
| ndarray | 0.16 | Tensor data | Create input tensors for ort. `Session::run()` accepts `TensorRef` from ndarray views via `ort::inputs!` macro. |

**ort feature flags to enable:**
- `download-binaries` (default) — auto-downloads ONNX Runtime shared libraries
- `cuda` — CUDA execution provider (NVIDIA GPU, requires CUDA 11.6+)
- `tensorrt` — TensorRT execution provider (NVIDIA optimized inference)
- `coreml` — CoreML execution provider (Apple Silicon, useful for local dev on macOS)
- `half` — f16 tensor support (needed for quantized models)
- `tracing` — integrates with `tracing` crate for internal diagnostics

**Critical ort design constraint:** `Session::run()` takes `&mut self` because ONNX Runtime internals are not thread-safe. For concurrent inference, you need one of:
1. `Arc<Mutex<Session>>` — simple but serializes all inference (fine for single-model-per-pod)
2. Pool of Sessions — create N sessions sharing the same model, dispatch round-robin
3. Dedicated inference thread with `tokio::sync::mpsc` channel — recommended for batching

### Model Acquisition

| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| hf-hub | 1.0.0 | HuggingFace model downloads | Official Rust client from HuggingFace. Reached 1.0 stable. Api/ApiBuilder pattern, `.model(id).get(filename)` returns local cached path. Shares cache directory with Python `huggingface_hub` — content-addressed blob storage with ETag-based deduplication. |
| aws-sdk-s3 | 1.143 | S3 model cache | Official AWS SDK. ByteStream for streaming uploads/downloads. Use `get_object().send().await?.body.collect().await?.into_bytes()` for full downloads, or stream via `impl Stream<Result<Bytes>>`. |
| aws-config | 1.11 | AWS credential resolution | Loads credentials from env, IMDS, IRSA (k8s). Use `behavior-version-latest` feature. |

### Tokenization

| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| tokenizers | 0.23 | HuggingFace tokenizers | Rust-native implementation (HuggingFace wrote it in Rust first, Python bindings are wrappers). Loads `tokenizer.json` directly. Pipeline: Normalizer -> PreTokenizer -> Model -> PostProcessor. Extremely fast — tokenizes 1GB text in <20s on CPU. Exact fidelity with model training tokenizer is critical for inference quality. |

### Serving (gRPC + HTTP)

| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| tonic | 0.14 | gRPC server | The Rust gRPC framework. Built on Hyper/Tower. High-throughput internal callers use gRPC. Health service via `tonic-health`. |
| tonic-health | 0.14 | gRPC health checks | Standard `grpc.health.v1.Health` service. `HealthReporter` handle updates service status. Use for k8s gRPC health probes. |
| tonic-reflection | 0.14 | gRPC reflection | Server reflection for debugging with grpcurl/grpcui. Enable in non-production or behind a feature flag. |
| tonic-build | 0.14 | Protobuf codegen | Build script generates Rust types from .proto files. |
| prost | 0.14 | Protobuf types | Protobuf serialization. Transitive dep of tonic. Version must align with tonic. |
| axum | 0.8 | HTTP/REST server | Macro-free, Tower-native, maintained by the Tokio team. REST endpoints for quick integrations, health checks, metrics scraping. |
| tower | 0.5 | Middleware | Shared middleware layer between axum and tonic. Rate limiting, timeout, compression. |

**Multiplexing gRPC + HTTP on a single port:** Route by `Content-Type: application/grpc` header. Both axum and tonic speak HTTP/2 via Hyper. Use `tower::steer::Steer` or content-type inspection in a Hyper service to route traffic. This avoids exposing two ports and simplifies k8s service configuration.

### Observability

| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| tracing | 0.1 | Instrumentation | De facto Rust instrumentation. Structured spans and events. Use `#[instrument]` on async functions. |
| tracing-subscriber | 0.3 | Log output | Formats tracing events as structured JSON logs (for k8s log aggregation) or human-readable (for dev). Use `fmt::layer()` with `json()` format in production. |
| tracing-opentelemetry | 0.33 | OTel bridge | Exports tracing spans as OpenTelemetry traces. Bridges the Rust tracing ecosystem to OTel Collector/Jaeger/Tempo. |
| opentelemetry | 0.32 | OTel API | Core OpenTelemetry types. Required by tracing-opentelemetry. |
| opentelemetry-otlp | 0.32 | OTLP exporter | Sends traces (and optionally metrics) to OTel Collector via gRPC or HTTP. Use gRPC transport in k8s (lower overhead). |
| opentelemetry_sdk | 0.32 | OTel SDK | Runtime implementation. Batch span processor, resource detection. |
| metrics | 0.24 | Metrics API | Lightweight metrics facade. `counter!`, `histogram!`, `gauge!` macros. Simpler than OTel metrics for Prometheus-scrape setups. |
| metrics-exporter-prometheus | 0.18 | Prometheus exporter | Renders `/metrics` endpoint in Prometheus exposition format. Axum handler pulls from global metrics registry. Simpler and more reliable than the deprecated `opentelemetry-prometheus` crate. |

**Observability architecture:**
- **Logs**: `tracing` + `tracing-subscriber` (JSON format) -> stdout -> k8s log aggregation (Loki/CloudWatch)
- **Traces**: `tracing` -> `tracing-opentelemetry` -> `opentelemetry-otlp` -> OTel Collector -> Tempo/Jaeger
- **Metrics**: `metrics` + `metrics-exporter-prometheus` -> `/metrics` endpoint -> Prometheus scrape

This split is deliberate. The `opentelemetry-prometheus` crate is deprecated. Using `metrics` for Prometheus is simpler, more stable, and avoids pulling in the full OTel metrics SDK. Traces go through OTel because distributed tracing needs collector infrastructure anyway.

### Testing & Dev

| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| tokio (test features) | 1.53 | Async test runtime | `#[tokio::test]` for async tests. |
| criterion | 0.5 | Benchmarks | Statistically rigorous benchmarks for inference latency. |
| mockall | 0.13 | Mocking | Mock trait implementations for unit testing service layers. |
| tempfile | 3 | Temp directories | Test fixtures for model file caching. |

## Alternatives Considered

| Category | Recommended | Alternative | Why Not |
|----------|-------------|-------------|---------|
| ONNX bindings | ort (pykeio) | onnxruntime (raw C bindings) | ort provides safe Rust API, execution provider management, and active maintenance. Raw bindings are unsafe and unmaintained. |
| HTTP framework | axum | actix-web | Axum is Tower-native (shares middleware with tonic), maintained by Tokio team, no macro magic. Actix uses its own runtime, doesn't compose with tonic. |
| gRPC | tonic | grpc-rs (C wrapper) | tonic is pure Rust, async-native, well-maintained. grpc-rs wraps C gRPC core — unnecessary complexity. |
| Metrics | metrics + prometheus exporter | opentelemetry metrics SDK | OTel metrics SDK is complex, the Prometheus exporter is deprecated, and the OTLP-push approach adds infrastructure. `metrics` crate is simpler for scrape-based Prometheus. |
| S3 client | aws-sdk-s3 | rust-s3 | aws-sdk-s3 is the official AWS SDK, handles IRSA/IMDS credentials natively. rust-s3 is a community crate with less maintenance. |
| HF downloads | hf-hub | Manual HTTP | hf-hub is the official client, handles caching, ETags, auth tokens, and cache sharing with Python tooling. |
| Tokenizers | tokenizers (HF) | Manual regex | tokenizers is the reference implementation. Any deviation from the training tokenizer causes silent quality degradation. |
| Error handling | thiserror + anyhow | eyre | thiserror + anyhow is the established pattern. eyre adds custom report handlers which are unnecessary here. |
| Serialization | serde | manual impl | No reason to avoid serde. It is the ecosystem standard. |

## Version Pinning Strategy

Use exact versions for the `ort` + `ort-sys` pair (they must match). Use caret ranges for everything else.

```toml
[workspace.dependencies]
# Inference — pin exactly, versions must match
ort = { version = "=2.0.0-rc.13", features = ["download-binaries", "half"] }

# Serving
tonic = { version = "0.14", features = ["transport"] }
tonic-health = "0.14"
tonic-reflection = "0.14"
tonic-build = "0.14"
prost = "0.14"
axum = { version = "0.8", features = ["json", "tokio"] }
tower = { version = "0.5", features = ["steer", "timeout"] }

# Model acquisition
hf-hub = { version = "1.0", features = ["tokio"] }
tokenizers = "0.23"
aws-sdk-s3 = "1.143"
aws-config = { version = "1.11", features = ["behavior-version-latest"] }

# Async
tokio = { version = "1.53", features = ["rt-multi-thread", "macros", "signal", "time", "sync", "fs", "io-util"] }

# Data
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
ndarray = "0.16"

# Observability
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["json", "env-filter"] }
tracing-opentelemetry = "0.33"
opentelemetry = "0.32"
opentelemetry-otlp = { version = "0.32", features = ["grpc-tonic"] }
opentelemetry_sdk = { version = "0.32", features = ["rt-tokio"] }
metrics = "0.24"
metrics-exporter-prometheus = "0.18"

# Errors
thiserror = "2.0"
anyhow = "1.0"

# Config
clap = { version = "4.6", features = ["derive", "env"] }

# Dev
[workspace.dev-dependencies]
tokio = { version = "1.53", features = ["test-util"] }
criterion = { version = "0.5", features = ["async_tokio"] }
mockall = "0.13"
tempfile = "3"
```

## Execution Provider Configuration

### CPU (default, always available)
No feature flag needed. Automatic fallback when no GPU EP is available.

```rust
ort::init()
    .with_execution_providers([ort::execution_providers::CPUExecutionProvider::default()])
    .commit()?;
```

### CUDA (NVIDIA GPU)
Requires: `cuda` feature flag, CUDA 11.6+, cuDNN installed on host.

```rust
ort::init()
    .with_execution_providers([
        ort::execution_providers::CUDAExecutionProvider::default()
            .with_device_id(0),
        ort::execution_providers::CPUExecutionProvider::default(),
    ])
    .commit()?;
```

EPs are tried in order. If CUDA is unavailable, falls back to CPU silently. Check availability: `CUDAExecutionProvider::default().is_available()`.

### TensorRT (NVIDIA optimized)
Requires: `tensorrt` feature flag, TensorRT installed on host.
Best for: production GPU inference with maximum throughput. Longer startup (engine compilation) but faster steady-state.

Key env vars:
- `ORT_TENSORRT_MAX_WORKSPACE_SIZE` (default 1GB)
- `ORT_TENSORRT_FP16_ENABLE=1` for FP16 mode
- `ORT_TENSORRT_ENGINE_CACHE_ENABLE=1` to cache compiled engines

### CoreML (Apple Silicon)
Requires: `coreml` feature flag, macOS/iOS.
Useful for: local development on Apple Silicon Macs. Not relevant for production k8s.

## Container Build Strategy

Two Dockerfile targets:
1. **CPU**: Use `download-binaries` default (pulls ONNX Runtime CPU libs)
2. **GPU**: Base on `nvidia/cuda:12.x-cudnn-runtime-ubuntu22.04`, compile with `cuda` and/or `tensorrt` features, include ONNX Runtime GPU libs

Use multi-stage builds: build stage with full toolchain, runtime stage with only the binary + ONNX Runtime shared libs.

## Sources

- [ort crate on crates.io](https://crates.io/crates/ort) — version 2.0.0-rc.13 (verified via API)
- [ort documentation](https://docs.rs/ort/latest/ort/) — ONNX Runtime 1.28 binding
- [ort pykeio GitHub](https://github.com/pykeio/ort) — Session API, execution providers
- [ort DeepWiki](https://deepwiki.com/pykeio/ort) — Architecture, EP feature flags
- [tonic on crates.io](https://crates.io/crates/tonic) — version 0.14.6 (verified via API)
- [tonic-health docs](https://docs.rs/tonic-health) — gRPC health checking service
- [axum on crates.io](https://crates.io/crates/axum) — version 0.8.9 (verified via API)
- [axum docs.rs](https://docs.rs/axum/latest/axum/) — router, handlers, state
- [hf-hub on crates.io](https://crates.io/crates/hf-hub) — version 1.0.0 (verified via API)
- [hf-hub DeepWiki](https://deepwiki.com/huggingface/hf-hub) — API, caching, auth
- [tokenizers on crates.io](https://crates.io/crates/tokenizers) — version 0.23.1 (verified via API)
- [aws-sdk-s3 on crates.io](https://crates.io/crates/aws-sdk-s3) — version 1.143.0 (verified via API)
- [opentelemetry-rust GitHub](https://github.com/open-telemetry/opentelemetry-rust) — OTel Rust implementation
- [metrics crate](https://crates.io/crates/metrics) — version 0.24.6 (verified via API)
- [loftllc Rust ONNX architecture](https://loftllc.dev/en/docs/tech/architecture/building-embedding-rerank-api-on-rust-onnx/) — production patterns
- [axum + tonic multiplexing](https://github.com/sunsided/http-grpc-cohosting) — same-port serving
- [NVIDIA TensorRT EP docs](https://onnxruntime.ai/docs/execution-providers/TensorRT-ExecutionProvider.html) — EP configuration
