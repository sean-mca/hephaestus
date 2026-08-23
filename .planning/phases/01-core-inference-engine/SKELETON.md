# Walking Skeleton -- Hephaestus

**Phase:** 1
**Generated:** 2026-08-22

## Capability Proven End-to-End

A developer can set MODEL_ID and MODEL_PATH environment variables, run the hephaestus binary, and it loads an ONNX classifier model from a local directory, validates the tokenizer against the model's input spec, runs a warmup inference pass, and reports ready. An integration test classifies text through the full pipeline and asserts on the output label and confidence score.

## Architectural Decisions

| Decision | Choice | Rationale |
|---|---|---|
| Language & edition | Rust 2024 edition, workspace resolver 3 | Project constraint. Single binary replaces Python runtimes; Rust delivers performance and single-binary deployment. |
| ONNX inference | ort 2.0.0-rc.13 (ONNX Runtime 1.28) | Only serious Rust binding for ONNX Runtime. Pre-release but actively maintained, 478K weekly downloads. |
| Tokenization | tokenizers 0.23.1 (HuggingFace Rust-native) | Reference implementation written in Rust by HuggingFace. Exact fidelity with training tokenizer is critical. |
| Configuration | envy 0.4.2 (serde env var deserialization) | K8s-only service -- all config from env vars. envy is minimal and purpose-built. No CLI parser needed (user feedback: no Clap for k8s services). |
| Error handling | thiserror 2.0 (library) + anyhow 1.0 (application) | Standard Rust error pattern. thiserror for typed errors at crate boundaries, anyhow for context-rich propagation in the binary. Per rules err-thiserror-lib and err-anyhow-app. |
| Directory layout | crates/ workspace with 4 member crates | hephaestus (binary), hephaestus-core (pipeline/inference), hephaestus-resolve (Phase 3 stub), hephaestus-proto (Phase 2+ stub). Central dependency pinning via workspace.dependencies. |
| Pipeline abstraction | Two-step Pipeline trait: prepare() + execute() | Ousterhout deep module pattern (2 methods hide tokenization, inference, post-processing). Two-step enables future batching in Phase 4. |
| Session ownership | Owned Session in Pipeline (not Arc) | Session::run() takes &mut self. Phase 1 has no concurrent access. Phase 2 wraps in Arc<Mutex<Session>> for HTTP handlers. |

## Stack Touched in Phase 1

- [x] Project scaffold (4-crate Rust workspace, Cargo.toml hierarchy, clippy, cargo test)
- [x] Model loading -- ONNX Session from local file path via ort
- [x] Tokenization -- load tokenizer.json via tokenizers crate, validate against model inputs
- [x] Inference -- real classification: text -> tokenize -> ONNX inference -> softmax -> label + score
- [x] Configuration -- env vars deserialized to typed Config struct via envy
- [x] Warmup -- inference pass at startup before reporting ready
- [ ] Deployment -- no HTTP server or k8s manifest in Phase 1 (Phase 2 adds serving)

## Out of Scope (Deferred to Later Slices)

- HTTP/gRPC serving, health probes, graceful shutdown (Phase 2)
- Model resolution from S3 or HuggingFace at runtime (Phase 3)
- Embeddings, seq2seq, token classifier profiles (Phase 4)
- Dynamic batching (Phase 4)
- Forge conversion service (Phase 5)
- GPU execution providers, CUDA/TensorRT (v2)
- Multi-model per pod, public API, multi-tenant auth (out of scope entirely)

## Subsequent Slice Plan

Each later phase adds one vertical slice on top of this skeleton without altering its architectural decisions:

- Phase 2: HTTP serving -- axum REST endpoints, health probes, Prometheus metrics, structured logging, OpenTelemetry tracing
- Phase 3: Model resolution -- S3 cache -> HuggingFace -> Forge fallback chain behind a single resolve() call
- Phase 4: Additional profiles (embeddings, seq2seq, NER) + configurable dynamic batching
- Phase 5: Forge Python service for converting non-ONNX models to ONNX format
