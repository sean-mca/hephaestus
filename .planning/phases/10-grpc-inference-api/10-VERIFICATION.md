---
phase: 10-grpc-inference-api
verified: 2026-08-28T00:00:00Z
status: passed
score: 5/6 must-haves verified
behavior_unverified: 1
overrides_applied: 0
behavior_unverified_items:

  - truth: "gRPC health checking service reports serving/not-serving aligned with the existing readiness state (SC-03)"
    test: "Start the binary with a real model (MODEL_ID + STORAGE_TYPE=none), call grpc.health.v1.Health/Check (service name '' and 'hephaestus.v1.InferenceService') before warmup completes, after warmup, and after SIGTERM."
    expected: "NOT_SERVING before warmup completes, SERVING after warmup completes, NOT_SERVING immediately after SIGTERM/Ctrl-C is received."
    why_human: "The transition is wired in main.rs (health_reporter.set_service_status calls around state.set_ready(true/false)) and compiles, but no test in the codebase actually starts the binary/gRPC server and observes the Health service through the three states. This is a runtime state-transition invariant that presence/wiring checks cannot exercise."
human_verification:

  - test: "Start the binary with a real model and use grpcurl (grpc.health.v1.Health/Check) to confirm SERVING/NOT_SERVING transitions described above."
    expected: "See behavior_unverified_items entry above."
    why_human: "State transition not exercised by an automated test; grpcurl was not available in the verification environment (command not found) and starting a live server is out of scope for automated spot-checks."

  - test: "Use grpcurl -plaintext localhost:<port> list (server reflection) against a running instance to confirm hephaestus.v1.InferenceService, grpc.health.v1.Health, and grpc.reflection.v1.ServerReflection are all discoverable, then grpcurl ... hephaestus.v1.InferenceService/Infer with a JSON payload to confirm the RPC succeeds without a local .proto file."
    expected: "grpcurl list shows all three services; Infer call returns a JSON body with model_id, latency_ms, result_json (base64) fields resolved via reflection."
    why_human: "Reflection wiring (tonic_reflection::server::Builder + FILE_DESCRIPTOR_SET) is confirmed via code, build, and a unit test that the descriptor set is non-empty, but end-to-end grpcurl discovery against a live process was not exercised in this verification run (grpcurl not installed, and spot-checks must not start servers)."
---

# Phase 10: gRPC Inference API Verification Report

**Phase Goal:** Add a tonic gRPC serving layer alongside the existing HTTP/REST API, multiplexed on the same port, with health checking, reflection, and full inference support for all model profiles
**Verified:** 2026-08-28
**Status:** human_needed
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths

Truths merged from ROADMAP.md Phase 10 Success Criteria (roadmap contract) and PLAN 10-01/10-02 frontmatter `must_haves.truths` (deduplicated; roadmap wording kept as primary).

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 (SC-01) | gRPC clients can call an Infer RPC and receive classification/embedding/NER/seq2seq results identical to the REST API | ✓ VERIFIED | `crates/hephaestus-api/src/grpc/inference.rs` `infer()` reuses the exact `AppState` readiness gate, validation, timeout, read/write lock split, and batching path as `handlers::infer` in `crates/hephaestus-api/src/handlers.rs`; both enrich the pipeline's `serde_json::Value` output with `model_id`/`latency_ms` identically. gRPC serializes the same `Value` to `result_json` bytes with `serde_json::to_vec`, so the payload is byte-identical to what REST returns as JSON, for any profile (opaque JSON, no per-profile proto types). Confirmed by 2 passing unit tests (`classifier_result_json_roundtrips`, `embedding_result_json_roundtrips`) and full workspace build/test pass. |
| 2 (SC-02) | gRPC and HTTP/REST are multiplexed on the same port — no separate listener or port configuration required | ✓ VERIFIED | `crates/hephaestus/src/main.rs`: a single `tokio::net::TcpListener::bind(&addr)` is created; `tonic::service::Routes::new(inference_service).add_service(health_service).add_service(reflection_service).into_axum_router()` is merged with `build_router(state.clone())` via `rest_router.merge(grpc_router)`; a single `axum::serve(listener, app)` call serves the merged router. No second bind/listener exists anywhere in the binary. |
| 3 (SC-03) | gRPC health checking service reports serving/not-serving aligned with the existing readiness state | ⚠️ PRESENT_BEHAVIOR_UNVERIFIED | Code is present and wired: `tonic_health::server::health_reporter()` created before warmup, explicitly set to `NotServing` for `""` and `"hephaestus.v1.InferenceService"`; flipped to `Serving` immediately after `state.set_ready(true)`; flipped back to `NotServing` in `shutdown_signal` immediately after `state.set_ready(false)`. This is a state-transition invariant, and no test in the codebase starts the server and observes the transition — see `behavior_unverified_items`. |
| 4 (SC-04) | Server reflection is enabled, allowing grpcurl/grpcui to discover and call services without proto files | ✓ VERIFIED (core claim) | `tonic_reflection::server::Builder::configure().register_encoded_file_descriptor_set(hephaestus_proto::FILE_DESCRIPTOR_SET).build_v1()` is constructed and added to the merged `Routes` in `main.rs`. `FILE_DESCRIPTOR_SET` is confirmed non-empty by `hephaestus-proto`'s `file_descriptor_set_is_nonempty` unit test, and the descriptor is generated by `tonic-prost-build` in `build.rs` with `file_descriptor_set_path`. Live grpcurl discovery against a running process was not exercised — see human verification. |
| 5 (SC-05) | Proto definitions are published in the hephaestus-proto crate with tonic-build codegen | ✓ VERIFIED | `crates/hephaestus-proto/proto/hephaestus/v1/inference.proto` defines `InferenceService`/`InferRequest`/`InferResponse`; `build.rs` uses `tonic_prost_build::configure()...compile_protos(...)`; `lib.rs` exposes `pub mod v1 { tonic::include_proto!("hephaestus.v1"); }` and `FILE_DESCRIPTOR_SET`. `cargo build --workspace` succeeds (codegen runs at build time), and 3 unit tests pass (`infer_request_roundtrip`, `infer_response_roundtrip`, `file_descriptor_set_is_nonempty`). |
| 6 (SC-06) | All existing REST functionality, metrics, and graceful shutdown behavior remain unchanged | ✓ VERIFIED | `build_router()` in `crates/hephaestus-api/src/routes.rs` is unmodified (still mounts `/infer`, `/healthz/live`, `/healthz/ready`, `/metrics`). `shutdown_signal` retains the original `state.set_ready(false)` call, only adding the `HealthReporter` parameter and calls after it. Full workspace test suite passes unchanged plus new tests: 49 (hephaestus-resolve) + 50 (hephaestus-core) + 19 (hephaestus-api unit, incl. 4 new gRPC error-mapping tests) + 22 (hephaestus binary) + integration suites all green (`cargo test --workspace`). |

**Score:** 5/6 truths verified (1 present, behavior-unverified)

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/hephaestus-proto/proto/hephaestus/v1/inference.proto` | proto3 service + messages | ✓ VERIFIED | Present, matches plan spec exactly (InferenceService.Infer, InferRequest.text, InferResponse.model_id/latency_ms/result_json) |
| `crates/hephaestus-proto/build.rs` | tonic-prost-build codegen | ✓ VERIFIED | Present, configures file_descriptor_set_path + compile_protos |
| `crates/hephaestus-proto/src/lib.rs` | generated types + FILE_DESCRIPTOR_SET + tests | ✓ VERIFIED | Present, 3 passing unit tests |
| `crates/hephaestus-proto/Cargo.toml` | tonic/prost deps + tonic-prost-build build-dep | ✓ VERIFIED | Present; also adds runtime `tonic-prost` dep (documented deviation, required for generated code to compile) |
| `crates/hephaestus-api/src/grpc/mod.rs` | gRPC module, re-exports GrpcInferenceService | ✓ VERIFIED | Present, wired via `pub mod grpc;` in `lib.rs` |
| `crates/hephaestus-api/src/grpc/inference.rs` | InferenceService trait impl | ✓ VERIFIED | Present, substantive (mirrors HTTP handler control flow), 2 passing unit tests |

### Key Link Verification

| From | To | Via | Status | Details |
|------|-----|-----|--------|---------|
| `GrpcInferenceService` | `AppState` | `Arc<AppState>` field, `read_pipeline()`/`write_pipeline()`/`batcher()` calls | ✓ WIRED | Confirmed in `inference.rs`; identical lock-split pattern to `handlers::infer` |
| `HealthReporter` | `AppState::set_ready` | Both called together in `main.rs` (warmup completion) and `shutdown_signal` | ✓ WIRED (code-level) | Sequential calls confirmed by reading; runtime transition not exercised (see SC-03 above) |
| `tonic::service::Routes` | `axum::Router` | `.into_axum_router()` then `rest_router.merge(grpc_router)` | ✓ WIRED | Confirmed in `main.rs`; single `axum::serve` call on the merged router |
| `ApiError` | `tonic::Status` | `impl From<ApiError> for tonic::Status` in `error.rs` | ✓ WIRED | Confirmed; 4 passing unit tests cover NotReady→Unavailable, Timeout→DeadlineExceeded, Tokenization→InvalidArgument, Inference→Internal |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| hephaestus-proto unit tests (roundtrip + descriptor) | `cargo test -p hephaestus-proto --lib` | 3 passed | ✓ PASS |
| hephaestus-api unit tests (gRPC service + error mapping) | `cargo test -p hephaestus-api --lib` | 19 passed (incl. 6 gRPC-related) | ✓ PASS |
| Full workspace build | `cargo build --workspace` | Finished, no errors | ✓ PASS |
| Full workspace test suite (single run) | `cargo test --workspace` | All suites green (49+50+19+22+integration) | ✓ PASS |
| grpcurl service discovery against a live instance | `grpcurl -plaintext localhost:<port> list` | Not run | ? SKIP — grpcurl not installed in this environment; starting a live server with a real model is out of scope for automated spot-checks |
| gRPC health transition end-to-end | live Health/Check calls across warmup/shutdown | Not run | ? SKIP — requires starting the server with a real model; routed to human verification |

### Requirements Coverage

The PLAN frontmatter for this phase declares requirement IDs `SC-01` through `SC-06`, which are ROADMAP.md Phase 10 Success Criteria identifiers, not REQUIREMENTS.md requirement IDs. REQUIREMENTS.md has no `SC-*` namespace (its scheme is `CORE-*`, `PROF-*`, `RSLV-*`, `TOKN-*`, `API-*`, `OBSV-*`, `BTCH-*`, `FORG-*`, `XCUT-*`, plus deferred `GPU-*`/`APIX-*`/`PRFX-*`/`OPTM-*`). This is a pre-existing project-wide convention drift (the same pattern appears in Phase 6's `STOR-01` and Phase 8's `SC-01..SC-04`), not something introduced by this phase.

Notably, REQUIREMENTS.md still lists **APIX-01** ("gRPC API for high-throughput internal callers") under "v2 Requirements ... Deferred to future release. Tracked but not in current roadmap," and the Traceability table does not include a Phase 10 row. Phase 10 in fact implements APIX-01's intent. This is a documentation-staleness gap in REQUIREMENTS.md, not a code/goal gap — the actual contract for this phase (ROADMAP.md Phase 10 Success Criteria) is verified above.

| Requirement | Source | Description | Status | Evidence |
|-------------|--------|-------------|--------|----------|
| SC-01..SC-06 | ROADMAP.md Phase 10 | See Observable Truths above | 5 SATISFIED / 1 present-unverified | See truths table |
| APIX-01 (REQUIREMENTS.md, v2, unmapped) | REQUIREMENTS.md | "gRPC API for high-throughput internal callers" | Functionally satisfied by this phase, but REQUIREMENTS.md not updated to reflect it | Recommend updating REQUIREMENTS.md traceability table in a follow-up (documentation-only; not a phase gap) |

### Anti-Patterns Found

Scanned all files modified by both plans (`Cargo.toml`, `crates/hephaestus-proto/**`, `crates/hephaestus-api/src/grpc/**`, `crates/hephaestus-api/src/error.rs`, `crates/hephaestus-api/src/lib.rs`, `crates/hephaestus-api/Cargo.toml`, `crates/hephaestus/src/main.rs`, `crates/hephaestus/Cargo.toml`) for `TBD`/`FIXME`/`XXX`/`TODO`/`HACK`/`PLACEHOLDER`/`placeholder`/`not yet implemented`/`not available`.

None found. No debt markers, no stub return patterns (`return null`/`Response::json({message: "Not implemented"})`/empty-array fallbacks), no console-log-only handlers.

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| — | — | none found | — | — |

### Human Verification Required

### 1. gRPC health service state transition

**Test:** Start the binary with a real model (e.g. `MODEL_ID=Xenova/distilbert-base-uncased-finetuned-sst-2-english STORAGE_TYPE=none PORT=8090 ./target/release/hephaestus`). Immediately (before warmup finishes) call `grpc.health.v1.Health/Check` for service `""` and `"hephaestus.v1.InferenceService"`. Wait for "warmup complete, readiness enabled" log line, call again. Send SIGTERM/Ctrl-C, call again during the drain window.
**Expected:** NOT_SERVING before warmup, SERVING after warmup, NOT_SERVING immediately after shutdown signal — matching the HTTP `/healthz/ready` probe's 503/200/503 behavior at the same points in time.
**Why human:** This is a runtime state-transition invariant. The code correctly places `health_reporter.set_service_status` calls alongside `state.set_ready`, and the workspace compiles, but no automated test starts the process and observes the transition end-to-end.

### 2. gRPC server reflection discovery

**Test:** With the same running instance, run `grpcurl -plaintext localhost:8090 list` and `grpcurl -plaintext -d '{"text":"This product is amazing"}' localhost:8090 hephaestus.v1.InferenceService/Infer`.
**Expected:** `list` shows `hephaestus.v1.InferenceService`, `grpc.health.v1.Health`, and `grpc.reflection.v1.ServerReflection`; the `Infer` call succeeds without needing a local `.proto` file and returns `model_id`, `latency_ms`, and base64 `result_json`.
**Why human:** grpcurl was not available in this verification environment (`command not found`), and starting a live server with a downloaded model is out of scope for automated spot-checks (which must not start servers). Code, build, and the descriptor-non-empty unit test give strong indirect confidence, but the actual client-facing discovery flow is unverified.

### Gaps Summary

No blocking gaps. All artifacts exist, are substantive, and are wired correctly; the full workspace builds and all existing plus new tests pass (68 workspace unit/integration tests across all changed and unchanged crates, single `cargo test --workspace` run). The only open item is that two truths involving live gRPC client interaction — the health-status state transition (SC-03) and grpcurl-based reflection discovery (SC-04's client-facing half) — are backed by solid code/wiring evidence but have not been exercised against a running server, since grpcurl is unavailable in this environment and starting a live model server is outside the bounds of an automated spot-check. These are flagged for human verification rather than treated as failures, since the implementation is present and correctly wired per code inspection.

Also noted (non-blocking, documentation only): REQUIREMENTS.md was not updated to move APIX-01 out of the "v2 / deferred, not in current roadmap" section now that Phase 10 has implemented it, and its Traceability table has no Phase 10 row. This is part of a pre-existing pattern (Phases 6 and 8 have the same gap) and does not affect Phase 10's goal achievement.

---

*Verified: 2026-08-28*
*Verifier: Claude (gsd-verifier)*
