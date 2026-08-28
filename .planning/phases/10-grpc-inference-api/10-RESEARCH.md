# Phase 10: gRPC Inference API - Research

**Researched:** 2026-08-28
**Domain:** gRPC serving layer (tonic) multiplexed with axum HTTP/REST
**Confidence:** HIGH

## Summary

Phase 10 adds a tonic gRPC serving layer to Hephaestus alongside the existing axum HTTP/REST API, serving both protocols on a single TCP port. The core technical challenge is multiplexing HTTP/1.1 REST and HTTP/2 gRPC traffic through one listener. Tonic 0.14.6 (already a transitive dependency via opentelemetry-otlp) provides first-class integration with axum via `tonic::service::Routes::into_axum_router()`, which converts gRPC service routes into an `axum::Router` that can be merged with existing REST routes. This is the recommended approach -- no custom `MultiplexService` or `tower::steer::Steer` is needed.

The hephaestus-proto crate is currently an empty placeholder. It needs proto definitions, a `build.rs` for tonic-prost-build codegen, and the generated Rust types. The gRPC service reuses the same `Arc<AppState>` and `PipelineKind` dispatch that the HTTP handlers use, so implementation is primarily a new API surface over existing inference logic.

**Primary recommendation:** Use `tonic::service::Routes` to build gRPC services (inference, health, reflection), call `.into_axum_router()`, and merge the result with the existing axum `Router` via `.merge()`. This avoids custom routing logic and leverages tonic's native axum integration.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| gRPC Infer RPC | API / Serving | -- | New API surface over existing pipeline; mirrors HTTP handler |
| Proto definition + codegen | Build / Proto crate | -- | Compile-time code generation in hephaestus-proto |
| gRPC health checking | API / Serving | -- | Bridges existing `AtomicBool` readiness to gRPC health protocol |
| Server reflection | API / Serving | -- | Runtime service for tooling; no business logic |
| HTTP/gRPC multiplexing | API / Serving (main.rs) | -- | Server binding and router composition in binary crate |
| Inference pipeline | Core (unchanged) | -- | `PipelineKind` and `AppState` are shared, not duplicated |

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| tonic | 0.14.6 | gRPC server framework | The Rust gRPC framework. Already a transitive dep. Built on Hyper/Tower, native axum integration via Routes. [VERIFIED: crates.io registry] |
| tonic-health | 0.14.6 | gRPC health checking | Standard `grpc.health.v1.Health` service. `HealthReporter` handle updates service status. Version must match tonic. [VERIFIED: crates.io registry] |
| tonic-reflection | 0.14.6 | gRPC server reflection | Enables grpcurl/grpcui discovery without proto files. Version must match tonic. [VERIFIED: crates.io registry] |
| prost | 0.14.4 | Protobuf serialization | Protobuf Rust types. Transitive dep of tonic. Already in workspace tree. [VERIFIED: crates.io registry] |
| prost-types | 0.14.4 | Well-known protobuf types | Required by tonic-reflection for FileDescriptorSet. Must match prost version. [VERIFIED: crates.io registry] |

### Build Dependencies

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| tonic-prost-build | 0.14.6 | Proto codegen in build.rs | Generates Rust types + gRPC server/client stubs from .proto files. Replaces the older tonic-build + prost-build pattern. [VERIFIED: crates.io registry] |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| tonic::service::Routes + into_axum_router() | tower::steer::Steer content-type routing | Steer is more manual, requires custom service impl, harder to maintain. Routes is tonic's official axum integration. |
| tonic::service::Routes + into_axum_router() | axum_tonic crate | Extra dependency for what tonic already provides natively. axum_tonic (v0.4.1) adds NestTonic trait but is unnecessary when using Routes directly. |
| Single Infer RPC with oneof response | Separate RPCs per profile (Classify, Embed, etc.) | Separate RPCs are more gRPC-idiomatic (strong typing per profile) but duplicate request logic. Single RPC with oneof mirrors the REST API's unified `/infer` endpoint. |
| tonic-prost-build | tonic-build (legacy) | tonic-prost-build is the current recommended crate as of tonic 0.14. tonic-build is being deprecated in favor of the split crate. |

**Installation (workspace Cargo.toml additions):**
```toml
# gRPC serving
tonic = "0.14"
tonic-health = "0.14"
tonic-reflection = "0.14"
prost = "0.14"
prost-types = "0.14"

# Build deps (in hephaestus-proto/Cargo.toml [build-dependencies])
tonic-prost-build = "0.14"
```

## Package Legitimacy Audit

| Package | Registry | Age | Downloads | Source Repo | Verdict | Disposition |
|---------|----------|-----|-----------|-------------|---------|-------------|
| tonic | crates.io | 8 yrs | 6.5M/wk | github.com/hyperium/tonic | OK | Approved |
| tonic-health | crates.io | 6 yrs | 794K/wk | github.com/hyperium/tonic | OK | Approved |
| tonic-reflection | crates.io | 5 yrs | 784K/wk | github.com/hyperium/tonic | OK | Approved |
| tonic-prost-build | crates.io | 1 yr | 1.8M/wk | github.com/hyperium/tonic | OK | Approved |
| prost | crates.io | 9 yrs | 9.8M/wk | github.com/tokio-rs/prost | OK | Approved |
| prost-types | crates.io | 9 yrs | 7.1M/wk | github.com/tokio-rs/prost | OK | Approved |

**Packages removed due to [SLOP] verdict:** none
**Packages flagged as suspicious [SUS]:** none

## Architecture Patterns

### System Architecture Diagram

```
                    TCP Port (e.g., 8080)
                          |
                    [TcpListener]
                          |
                   [axum::serve]
                          |
              +-----------+-----------+
              |                       |
        content-type:            all other
       application/grpc          requests
              |                       |
     [tonic gRPC routes]      [axum REST routes]
      (merged via Router)      (existing routes)
              |                       |
    +---------+---------+    +--------+--------+
    |         |         |    |        |        |
  Infer    Health   Reflect  POST   GET      GET
   RPC     Check    ion      /infer /healthz /metrics
    |       svc     svc
    |
    +---> Arc<AppState>
              |
         PipelineKind
         (shared with REST)
```

### How Multiplexing Works

Tonic 0.14 provides `tonic::service::Routes`, which implements `Service<Request<Body>>` and also converts to an `axum::Router` via `into_axum_router()`. The pattern is:

1. Build gRPC services (InferenceServer, HealthServer, ReflectionServer)
2. Add them to `tonic::service::Routes`
3. Call `.prepare().into_axum_router()` to get an `axum::Router`
4. Merge with the existing REST `axum::Router` via `.merge()`
5. Serve the combined router with `axum::serve()`

The key insight: both tonic and axum are built on hyper/tower. Since tonic 0.12+, tonic's internal router IS an axum Router. The `into_axum_router()` method simply unwraps it. Merged routers use axum's path-based routing -- gRPC services are routed by their protobuf service path (e.g., `/hephaestus.v1.InferenceService/Infer`), and REST endpoints by their HTTP paths (`/infer`, `/healthz/live`, etc.). No content-type sniffing is needed because the paths are disjoint.

**Important:** `axum::serve()` supports both HTTP/1.1 and HTTP/2. gRPC requires HTTP/2, but axum's built-in server handles HTTP/2 prior knowledge (h2c) and HTTP/1.1 upgrade transparently. No additional configuration is needed for plaintext gRPC (h2c). For TLS with ALPN negotiation, `axum-server` with rustls would be needed, but this project uses plaintext in-cluster communication. [CITED: docs.rs/tonic/0.14.6 and docs.rs/axum/0.8]

### Recommended Project Structure

```
crates/
  hephaestus-proto/
    proto/
      hephaestus/
        v1/
          inference.proto        # Service + message definitions
    build.rs                     # tonic-prost-build codegen
    src/
      lib.rs                     # include! generated code + FILE_DESCRIPTOR_SET
    Cargo.toml                   # prost, tonic, tonic-prost-build (build-dep)
  hephaestus-api/
    src/
      grpc/
        mod.rs                   # gRPC module
        inference.rs             # InferenceService impl
      lib.rs                     # add `pub mod grpc;`
      state.rs                   # unchanged -- shared with gRPC
  hephaestus/
    src/
      main.rs                    # Updated: build gRPC routes, merge, serve
```

### Pattern 1: Proto Definition with OneOf Response

**What:** A single `Infer` RPC that accepts text and returns a `oneof` result discriminated by model profile.
**When to use:** When the API mirrors the REST endpoint's unified `/infer` pattern.
**Example:**

```protobuf
// Source: designed for Hephaestus based on PipelineKind variants
syntax = "proto3";
package hephaestus.v1;

service InferenceService {
  rpc Infer(InferRequest) returns (InferResponse);
}

message InferRequest {
  string text = 1;
}

message InferResponse {
  string model_id = 1;
  uint64 latency_ms = 2;

  oneof result {
    ClassificationResult classification = 10;
    EmbeddingResult embedding = 11;
    TokenClassificationResult token_classification = 12;
    Seq2SeqResult seq2seq = 13;
  }
}

message ClassificationResult {
  string label = 1;
  float score = 2;
}

message EmbeddingResult {
  repeated float values = 1;
}

message TokenClassificationResult {
  repeated Entity entities = 1;
}

message Entity {
  string word = 1;
  string entity = 2;
  float score = 3;
  uint32 start = 4;
  uint32 end = 5;
}

message Seq2SeqResult {
  string generated_text = 1;
}
```

### Pattern 2: gRPC + axum Multiplexing in main.rs

**What:** Building the combined router in the binary crate.
**When to use:** Server startup, replacing the current axum-only `build_router` call.
**Example:**

```rust
// Source: docs.rs/tonic/0.14.6 -- tonic::service::Routes API
use tonic::service::Routes;

// Build gRPC services
let (health_reporter, health_service) = tonic_health::server::health_reporter();
// Set initial serving status after warmup
health_reporter
    .set_service_status("hephaestus.v1.InferenceService", tonic_health::ServingStatus::Serving)
    .await;

let reflection_service = tonic_reflection::server::Builder::configure()
    .register_encoded_file_descriptor_set(hephaestus_proto::FILE_DESCRIPTOR_SET)
    .build_v1()
    .expect("failed to build reflection service");

let inference_service = InferenceServiceServer::new(
    GrpcInferenceService::new(state.clone())
);

// Convert gRPC routes to axum Router
let grpc_router = Routes::new(inference_service)
    .add_service(health_service)
    .add_service(reflection_service)
    .prepare()
    .into_axum_router();

// Merge with existing REST router
let rest_router = build_router(state.clone());
let app = rest_router.merge(grpc_router);

// Serve (existing axum::serve call works unchanged)
axum::serve(listener, app)
    .with_graceful_shutdown(shutdown_signal(server_state))
```

### Pattern 3: gRPC Service Implementation

**What:** Implementing the tonic service trait using existing AppState.
**When to use:** The InferenceService impl in hephaestus-api.
**Example:**

```rust
// Source: pattern derived from existing handler logic in handlers.rs
use hephaestus_proto::v1::{
    inference_service_server::InferenceService,
    InferRequest, InferResponse,
    // result variants...
};
use tonic::{Request, Response, Status};

pub struct GrpcInferenceService {
    state: Arc<AppState>,
}

#[tonic::async_trait]
impl InferenceService for GrpcInferenceService {
    async fn infer(
        &self,
        request: Request<InferRequest>,
    ) -> Result<Response<InferResponse>, Status> {
        let req = request.into_inner();

        if !self.state.is_ready() {
            return Err(Status::unavailable("service not ready"));
        }
        if req.text.is_empty() {
            return Err(Status::invalid_argument("text must not be empty"));
        }

        let request_start = std::time::Instant::now();
        let timer = StageTimer::new(self.state.model_id().to_string());

        // Same prepare/execute logic as HTTP handler
        let prepared = {
            let pipeline = self.state.read_pipeline().await;
            timer.time("tokenization", || pipeline.prepare(req.text))
                .map_err(|e| Status::internal(e.to_string()))?
        };
        let output = {
            let mut pipeline = self.state.write_pipeline().await;
            timer.time("inference", || pipeline.execute(prepared))
                .map_err(|e| Status::internal(e.to_string()))?
        };

        timer.finish_request(request_start, true);
        let latency_ms = request_start.elapsed().as_millis() as u64;

        // Convert PipelineKind output to proto response
        let response = build_infer_response(output, self.state.model_id(), latency_ms);
        Ok(Response::new(response))
    }
}
```

### Pattern 4: tonic-health Bridge to AtomicBool Readiness

**What:** Keeping HealthReporter in sync with the existing `AppState::ready` AtomicBool.
**When to use:** Startup (after warmup) and shutdown signal handler.
**Example:**

```rust
// After warmup completes:
state.set_ready(true);
health_reporter
    .set_service_status("hephaestus.v1.InferenceService", ServingStatus::Serving)
    .await;
// Also set the empty service name (overall server health):
health_reporter
    .set_service_status("", ServingStatus::Serving)
    .await;

// In shutdown_signal():
state.set_ready(false);
health_reporter
    .set_service_status("hephaestus.v1.InferenceService", ServingStatus::NotServing)
    .await;
health_reporter
    .set_service_status("", ServingStatus::NotServing)
    .await;
```

### Pattern 5: build.rs for Proto Codegen

**What:** Generating Rust types and gRPC server stubs from .proto files.
**When to use:** In the hephaestus-proto crate's build.rs.
**Example:**

```rust
// Source: docs.rs/tonic-prost-build/0.14.6 + tonic examples build.rs
use std::{env, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    tonic_prost_build::configure()
        .file_descriptor_set_path(out_dir.join("hephaestus_descriptor.bin"))
        .compile_protos(
            &["proto/hephaestus/v1/inference.proto"],
            &["proto"],
        )?;

    Ok(())
}
```

And in `lib.rs`:

```rust
pub mod v1 {
    tonic::include_proto!("hephaestus.v1");
}

/// Encoded file descriptor set for gRPC reflection.
pub const FILE_DESCRIPTOR_SET: &[u8] =
    tonic::include_file_descriptor_set!("hephaestus_descriptor");
```

### Anti-Patterns to Avoid

- **Separate listener for gRPC:** Do NOT bind a second TCP listener. Use the Routes-to-axum-Router merge pattern. Two listeners means two ports, complicating k8s service definitions and health probes.
- **Custom MultiplexService:** The old pattern of writing a custom hyper service that sniffs content-type headers is obsolete since tonic 0.12. Use `into_axum_router()` instead.
- **Duplicating inference logic:** The gRPC handler should call the SAME `AppState::read_pipeline()` / `write_pipeline()` methods as the HTTP handler. Do not copy-paste pipeline interaction code.
- **Forgetting `.prepare()` before `.into_axum_router()`:** The `prepare()` call optimizes router internals. Omitting it works but leaves performance on the table.
- **Using `build_v1alpha()` for reflection:** Use `build_v1()`. The v1 reflection spec has been stable since 2023. v1alpha is deprecated in most tooling.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| gRPC health checking | Custom health RPC | tonic-health | Standard `grpc.health.v1.Health` protocol. k8s gRPC health probes expect this exact service. |
| Server reflection | Manual proto descriptor serving | tonic-reflection | Reflection protocol is complex (file descriptor graph traversal). Library handles it correctly. |
| Proto codegen | Manual prost compilation | tonic-prost-build | Generates both message types AND gRPC service traits with correct routing paths. |
| gRPC error mapping | Custom error serialization | `tonic::Status` | Standard gRPC status codes map directly to error semantics. |

**Key insight:** The entire gRPC serving surface is already solved by the tonic ecosystem. The implementation work is wiring tonic's generated traits to the existing `AppState` and `PipelineKind` dispatch -- not building any new infrastructure.

## Common Pitfalls

### Pitfall 1: HTTP/2 Prior Knowledge (h2c) Not Working

**What goes wrong:** gRPC clients can't connect because the server only speaks HTTP/1.1.
**Why it happens:** `axum::serve()` defaults to HTTP/1.1 but also supports h2c (HTTP/2 without TLS). However, some configurations or reverse proxies strip HTTP/2 prior knowledge.
**How to avoid:** Use `axum::serve()` directly (it handles h2c out of the box). Test with `grpcurl --plaintext` which uses h2c. Do NOT place an HTTP/1.1-only reverse proxy in front during testing.
**Warning signs:** "connection error" or "protocol error" from gRPC clients, while REST endpoints work fine.

### Pitfall 2: HealthReporter Out of Sync with AtomicBool

**What goes wrong:** gRPC health check returns SERVING while REST readiness returns 503 (or vice versa).
**Why it happens:** The `HealthReporter` is a separate state mechanism from `AppState::ready`. If one is updated without the other, clients see inconsistent health.
**How to avoid:** Update both in the same code path. Pass `HealthReporter` (cloneable) to `shutdown_signal()` alongside `Arc<AppState>`. Update both atomically (HealthReporter update is async but completes immediately in practice).
**Warning signs:** k8s routes traffic to a pod that reports SERVING via gRPC but 503 via HTTP, or vice versa.

### Pitfall 3: Missing FILE_DESCRIPTOR_SET for Reflection

**What goes wrong:** `grpcurl --plaintext localhost:8080 list` returns "failed to list services" or empty results.
**Why it happens:** The `file_descriptor_set_path()` was not set in build.rs, or the `include_file_descriptor_set!` macro points to the wrong filename.
**How to avoid:** Verify the filename in `file_descriptor_set_path()` matches the macro argument exactly. Add a test that decodes the descriptor set bytes to confirm they're valid.
**Warning signs:** Reflection service responds but lists no services. Check that the .bin file is generated in OUT_DIR during build.

### Pitfall 4: Blocking the Tokio Runtime in gRPC Handler

**What goes wrong:** gRPC handler calls `pipeline.execute()` under a tokio RwLock write guard, but the ONNX session runs synchronous CPU-bound work, blocking the runtime.
**Why it happens:** Same issue as the HTTP handler -- ONNX inference is CPU-bound.
**How to avoid:** This is already mitigated in the HTTP handler's design (write lock serialization via `RwLock<PipelineKind>`). The gRPC handler must follow the exact same read-lock-for-prepare, write-lock-for-execute pattern. Do NOT hold a lock across await points per `rules/anti-lock-across-await.md`.
**Warning signs:** Latency spikes under concurrent gRPC + HTTP load.

### Pitfall 5: Proto Package vs Crate Path Mismatch

**What goes wrong:** `tonic::include_proto!("hephaestus.v1")` fails to find the generated file.
**Why it happens:** The proto package name (`package hephaestus.v1;`) determines the generated filename. If the include path doesn't match, compilation fails.
**How to avoid:** The generated file will be at `{OUT_DIR}/hephaestus.v1.rs`. The `include_proto!` argument must be the package name with dots, not slashes.
**Warning signs:** `file not found` errors during compilation pointing to OUT_DIR.

### Pitfall 6: Version Mismatch Between tonic and prost

**What goes wrong:** Build errors with trait incompatibilities or mismatched types.
**Why it happens:** tonic 0.14 requires prost 0.14. Mixing versions (e.g., prost 0.13 from another dependency) causes silent type mismatches.
**How to avoid:** Use workspace dependencies for ALL tonic/prost crates. Verify with `cargo tree -d` that only one version of prost exists in the tree. The existing opentelemetry-otlp dep already pulls tonic 0.14.6 and prost 0.14.4 -- align with those exact versions.
**Warning signs:** "expected struct `prost::Message` found struct `prost::Message`" (same name, different versions).

## Code Examples

### gRPC Error Mapping (ApiError to tonic::Status)

```rust
// Source: pattern derived from existing error.rs + tonic Status codes
impl From<ApiError> for tonic::Status {
    fn from(err: ApiError) -> Self {
        match err {
            ApiError::NotReady => Status::unavailable("service not ready"),
            ApiError::BadRequest(msg) => Status::invalid_argument(msg),
            ApiError::Tokenization(msg) => Status::invalid_argument(format!("tokenization failed: {msg}")),
            ApiError::Timeout => Status::deadline_exceeded("inference timeout"),
            ApiError::Inference(_) | ApiError::Model(_) | ApiError::Internal(_) => {
                // Log detailed error server-side, return generic message
                Status::internal("internal server error")
            }
        }
    }
}
```

### Batching-Aware gRPC Handler

```rust
// Source: pattern from existing handlers.rs batching path
async fn infer_with_batching(
    state: &AppState,
    text: String,
    timer: &StageTimer,
) -> Result<serde_json::Value, ApiError> {
    if state.is_batching_enabled() {
        let prepared = {
            let pipeline = state.read_pipeline().await;
            timer.time("tokenization", || pipeline.prepare(text))?
        }; // Read lock dropped before submit
        state
            .batcher()
            .ok_or(ApiError::Internal("batcher not available".to_string()))?
            .submit(prepared)
            .await
            .map_err(ApiError::from)
    } else {
        let prepared = {
            let pipeline = state.read_pipeline().await;
            timer.time("tokenization", || pipeline.prepare(text))?
        };
        let output = {
            let mut pipeline = state.write_pipeline().await;
            timer.time("inference", || pipeline.execute(prepared))?
        };
        Ok(output)
    }
}
```

### serde_json::Value to Proto Response Conversion

```rust
// Source: pattern derived from PipelineKind::execute return types
fn build_infer_response(
    output: serde_json::Value,
    model_id: &str,
    latency_ms: u64,
) -> InferResponse {
    let result = if let Some(label) = output.get("label") {
        // Classifier output
        Some(infer_response::Result::Classification(ClassificationResult {
            label: label.as_str().unwrap_or_default().to_string(),
            score: output["score"].as_f64().unwrap_or_default() as f32,
        }))
    } else if let Some(embedding) = output.get("embedding") {
        // Embeddings output
        let values: Vec<f32> = embedding
            .as_array()
            .map(|arr| arr.iter().filter_map(|v| v.as_f64().map(|f| f as f32)).collect())
            .unwrap_or_default();
        Some(infer_response::Result::Embedding(EmbeddingResult { values }))
    } else if let Some(entities) = output.get("entities") {
        // Token classifier output -- convert JSON array to proto entities
        let proto_entities = convert_entities(entities);
        Some(infer_response::Result::TokenClassification(
            TokenClassificationResult { entities: proto_entities },
        ))
    } else if let Some(text) = output.get("generated_text") {
        // Seq2seq output
        Some(infer_response::Result::Seq2seq(Seq2SeqResult {
            generated_text: text.as_str().unwrap_or_default().to_string(),
        }))
    } else {
        None
    };

    InferResponse {
        model_id: model_id.to_string(),
        latency_ms,
        result,
    }
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Custom `MultiplexService` with content-type sniffing | `tonic::service::Routes::into_axum_router()` + merge | tonic 0.12 (2024) | No custom code needed for multiplexing |
| `tonic-build` + `prost-build` as separate deps | `tonic-prost-build` unified crate | tonic 0.14 (2025) | Single build dependency instead of two |
| `build_v1alpha()` for reflection | `build_v1()` | tonic-reflection 0.12+ | v1 spec is stable, v1alpha deprecated |
| Separate `tonic::transport::Server::builder().serve()` | Merge into axum router | tonic 0.12+ | Single server process, single port |

**Deprecated/outdated:**
- `tonic::transport::Server` for serving: Still works but unnecessary when multiplexing with axum. Use only if you need a standalone gRPC-only server.
- `tonic-build` as direct dependency: Replaced by `tonic-prost-build` which bundles the prost integration.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | `axum::serve()` handles h2c (HTTP/2 without TLS) transparently without extra configuration | Architecture Patterns | If h2c requires explicit opt-in, gRPC clients will fail to connect. Mitigation: test with grpcurl early. |
| A2 | `tonic-prost-build` is the recommended replacement for `tonic-build` in 0.14 | Standard Stack | If wrong, use `tonic-build` directly instead. Both produce identical output. |
| A3 | The `oneof` response pattern is more appropriate than separate RPCs for this use case | Architecture Patterns | If separate RPCs are preferred, proto definition changes but service implementation is similar. Low risk. |

## Open Questions

1. **Timeout handling for gRPC**
   - What we know: The HTTP handler wraps inference in `tokio::time::timeout()` with the `request_timeout` duration. gRPC has its own deadline mechanism.
   - What's unclear: Should the gRPC handler respect client-supplied deadlines via `request.metadata().get("grpc-timeout")`, or enforce the server-side timeout like the HTTP handler?
   - Recommendation: Enforce server-side timeout (same as HTTP), but also respect client deadline if shorter. Use `tokio::time::timeout(min(server_timeout, client_deadline), ...)`.

2. **Batcher interaction with gRPC**
   - What we know: The batcher accepts `PreparedInput` and returns `serde_json::Value` via oneshot channel.
   - What's unclear: The gRPC handler needs proto types, not JSON. Converting JSON to proto after batching works but adds overhead.
   - Recommendation: Share inference execution code between HTTP and gRPC handlers. Both call the same `prepare/execute` path, then convert the `serde_json::Value` output to their respective response format (JSON for HTTP, proto for gRPC). This is simple and the JSON-to-proto conversion cost is negligible compared to inference.

3. **Proto versioning scheme**
   - What we know: The proto package is `hephaestus.v1`.
   - What's unclear: Whether to version the proto in the package name (v1, v2) or use a separate versioning mechanism.
   - Recommendation: Use `hephaestus.v1` package naming. This is the standard protobuf versioning convention and allows future breaking changes in v2 without affecting v1 clients.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| protoc | Proto compilation in build.rs | Yes | 33.2 | tonic-prost-build may bundle protoc; verify |
| Rust toolchain | Compilation | Yes | (workspace) | -- |
| grpcurl | Testing gRPC endpoints | Check at test time | -- | Use `cargo test` with tonic test client |

**Missing dependencies with no fallback:** none
**Missing dependencies with fallback:** none

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | cargo test (built-in) + tokio::test |
| Config file | Cargo.toml [dev-dependencies] |
| Quick run command | `cargo test -p hephaestus-proto --lib && cargo test -p hephaestus-api --lib` |
| Full suite command | `cargo test --workspace` |

### Phase Requirements to Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| SC-01 | gRPC Infer RPC returns correct results for all profiles | integration | `cargo test -p hephaestus-api grpc` | No -- Wave 0 |
| SC-02 | gRPC + HTTP multiplexed on same port | integration | `cargo test -p hephaestus-api multiplex` | No -- Wave 0 |
| SC-03 | gRPC health check aligns with readiness | unit | `cargo test -p hephaestus-api grpc_health` | No -- Wave 0 |
| SC-04 | Server reflection discovers services | integration | `cargo test -p hephaestus-api reflection` | No -- Wave 0 |
| SC-05 | Proto definitions compile and generate types | unit | `cargo test -p hephaestus-proto` | No -- Wave 0 |
| SC-06 | Existing REST unchanged | integration | `cargo test -p hephaestus-api --test api` | Yes |

### Sampling Rate
- **Per task commit:** `cargo test -p hephaestus-proto && cargo test -p hephaestus-api`
- **Per wave merge:** `cargo test --workspace`
- **Phase gate:** Full suite green before `/gsd-verify-work`

### Wave 0 Gaps
- [ ] Proto definitions and build.rs for hephaestus-proto
- [ ] Unit tests for proto message construction and serialization
- [ ] Integration tests for gRPC inference handler (requires mock pipeline or test model)
- [ ] Integration test for multiplexing (HTTP + gRPC on same port)

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | No | gRPC auth is out of scope for this phase (internal cluster traffic) |
| V3 Session Management | No | Stateless RPC, no sessions |
| V4 Access Control | No | No authorization layer in this phase |
| V5 Input Validation | Yes | Validate `text` field is non-empty; truncation at 512 tokens (existing tokenizer config) |
| V6 Cryptography | No | Plaintext in-cluster communication; TLS is infrastructure concern |

### Known Threat Patterns for gRPC

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Oversized request payload | Denial of Service | tonic has default message size limits (4MB). Configure `max_decoding_message_size()` on the server. |
| Unbounded streaming | Denial of Service | Not applicable -- no streaming RPCs in this phase (unary only). |
| Reflection information disclosure | Information Disclosure | Server reflection exposes service and message definitions. Acceptable for internal services. For public-facing, gate behind feature flag. |
| gRPC status code leaking internals | Information Disclosure | Map all internal errors to `Status::internal("internal server error")` -- same pattern as HTTP handler. |

## Sources

### Primary (HIGH confidence)
- [crates.io: tonic 0.14.6](https://crates.io/crates/tonic) -- version verified via `cargo search`
- [crates.io: tonic-health 0.14.6](https://crates.io/crates/tonic-health) -- version verified
- [crates.io: tonic-reflection 0.14.6](https://crates.io/crates/tonic-reflection) -- version verified
- [crates.io: tonic-prost-build 0.14.6](https://crates.io/crates/tonic-prost-build) -- version verified
- [crates.io: prost 0.14.4](https://crates.io/crates/prost) -- version verified
- [docs.rs/tonic/0.14.6: service::Routes](https://docs.rs/tonic/0.14.6/tonic/service/struct.Routes.html) -- `into_axum_router()`, `add_service()`, `prepare()` API
- [docs.rs/tonic-health/0.14.6: HealthReporter](https://docs.rs/tonic-health/0.14.6/tonic_health/server/struct.HealthReporter.html) -- `set_service_status()`, `set_serving()`, `set_not_serving()` API
- [docs.rs/tonic-reflection/0.14.6: Builder](https://docs.rs/tonic-reflection/0.14.6/tonic_reflection/server/struct.Builder.html) -- `register_encoded_file_descriptor_set()`, `build_v1()` API
- Codebase: `crates/hephaestus-api/src/` -- handlers.rs, state.rs, routes.rs, error.rs, batcher.rs (read directly)
- Codebase: `crates/hephaestus-core/src/pipeline.rs` -- Pipeline trait, PipelineKind enum (read directly)
- Codebase: `crates/hephaestus/src/main.rs` -- server startup, shutdown signal (read directly)
- Codebase: `Cargo.toml` workspace -- existing dependency versions (read directly)
- `cargo tree` -- confirmed tonic 0.14.6 and prost 0.14.4 already in dependency tree via opentelemetry-otlp

### Secondary (MEDIUM confidence)
- [github.com/sunsided/http-grpc-cohosting](https://github.com/sunsided/http-grpc-cohosting) -- axum + tonic multiplexing patterns
- [github.com/tokio-rs/axum PR #2825](https://github.com/tokio-rs/axum/pull/2825) -- rest-grpc-multiplex example updates
- [github.com/hyperium/tonic examples/build.rs](https://github.com/hyperium/tonic/blob/master/examples/build.rs) -- file_descriptor_set_path pattern

### Tertiary (LOW confidence)
- [ASSUMED] `axum::serve()` h2c support without explicit configuration -- needs validation during implementation

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- all crates verified on crates.io, versions confirmed in dependency tree, docs.rs API verified
- Architecture: HIGH -- multiplexing pattern confirmed via tonic docs (Routes::into_axum_router), existing codebase analyzed thoroughly
- Pitfalls: MEDIUM -- pitfalls derived from training knowledge and community discussions, not all personally encountered

**Research date:** 2026-08-28
**Valid until:** 2026-09-28 (stable ecosystem, tonic 0.14 is mature)

## Project Constraints (from CLAUDE.md)

- **Language:** Rust only, 2024 edition, workspace resolver 3
- **Code Convention:** Traits must follow Ousterhout deep module principle (1-3 methods). The gRPC InferenceService trait is generated by tonic and has 1 method (Infer) -- compliant.
- **No AI attribution:** No Co-Authored-By lines, AI-generated mentions, or Claude references in any artifacts.
- **Rules compliance:** All files must adhere to rules in `rules/`. Key rules for this phase:
  - `anti-lock-across-await.md` -- do not hold RwLock across await points in gRPC handler
  - `err-thiserror-lib.md` / `err-anyhow-app.md` -- use thiserror in library crate, anyhow in binary
  - `doc-all-public.md` -- document all public items
  - `async-bounded-channel.md` -- if extending batcher for gRPC, use bounded channels
