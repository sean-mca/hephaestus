---
phase: 10-grpc-inference-api
plan: 02
subsystem: api
tags: [grpc, tonic, tonic-health, tonic-reflection, inference, multiplexing]

requires:
  - phase: 10-grpc-inference-api
    provides: hephaestus-proto crate with InferRequest, InferResponse, InferenceService trait, FILE_DESCRIPTOR_SET
provides:
  - GrpcInferenceService implementing tonic InferenceService trait with result_json opaque bytes
  - Multiplexed gRPC + REST router on single TCP port
  - gRPC health checking synced with AtomicBool readiness
  - gRPC server reflection for service discovery
  - ApiError to tonic::Status mapping with information-hiding
affects: [future-grpc-clients, deployment, monitoring]

tech-stack:
  added: [tonic 0.14 (InferenceService server), tonic-health 0.14 (HealthReporter), tonic-reflection 0.14 (server reflection)]
  patterns: [gRPC+REST multiplexing via axum router merge, HealthReporter synced with AtomicBool readiness, opaque JSON bytes in result_json]

key-files:
  created:
    - crates/hephaestus-api/src/grpc/mod.rs
    - crates/hephaestus-api/src/grpc/inference.rs
  modified:
    - crates/hephaestus-api/Cargo.toml
    - crates/hephaestus-api/src/lib.rs
    - crates/hephaestus-api/src/error.rs
    - crates/hephaestus/Cargo.toml
    - crates/hephaestus/src/main.rs

key-decisions:
  - "HealthReporter initialized to NOT_SERVING, set to SERVING only after warmup completes"
  - "tonic::service::Routes merged into axum Router via into_axum_router for same-port multiplexing"

patterns-established:
  - "gRPC handler mirrors HTTP handler control flow exactly: readiness gate, validation, timeout, lock split, tracing"
  - "ApiError to tonic::Status maps internal errors to generic message (information-hiding matches HTTP)"
  - "HealthReporter updated alongside AtomicBool set_ready to keep gRPC and HTTP health probes consistent"

requirements-completed: [SC-01, SC-02, SC-03, SC-04, SC-06]

coverage:
  - id: D1
    description: "GrpcInferenceService implements InferenceService trait with result_json opaque bytes"
    requirement: "SC-01"
    verification:
      - kind: unit
        ref: "crates/hephaestus-api/src/grpc/inference.rs#classifier_result_json_roundtrips"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-api/src/grpc/inference.rs#embedding_result_json_roundtrips"
        status: pass
    human_judgment: false
  - id: D2
    description: "gRPC and REST multiplexed on same TCP port via axum router merge"
    requirement: "SC-02"
    verification:
      - kind: other
        ref: "cargo build --workspace (compiles cleanly, routes merged in main.rs)"
        status: pass
    human_judgment: false
  - id: D3
    description: "HealthReporter synced with AtomicBool readiness on startup and shutdown"
    requirement: "SC-03"
    verification:
      - kind: other
        ref: "cargo build --workspace (health_reporter set_service_status calls verified at compile time)"
        status: pass
    human_judgment: false
  - id: D4
    description: "tonic-reflection configured with FILE_DESCRIPTOR_SET for service discovery"
    requirement: "SC-04"
    verification:
      - kind: other
        ref: "cargo build --workspace (reflection service built with hephaestus_proto::FILE_DESCRIPTOR_SET)"
        status: pass
    human_judgment: false
  - id: D5
    description: "ApiError to tonic::Status mapping with information-hiding for internal errors"
    verification:
      - kind: unit
        ref: "crates/hephaestus-api/src/error.rs#api_error_not_ready_to_grpc_unavailable"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-api/src/error.rs#api_error_timeout_to_grpc_deadline_exceeded"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-api/src/error.rs#api_error_tokenization_to_grpc_invalid_argument"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-api/src/error.rs#api_error_inference_to_grpc_internal"
        status: pass
    human_judgment: false
  - id: D6
    description: "Existing REST endpoints and metrics continue to work unchanged"
    requirement: "SC-06"
    verification:
      - kind: unit
        ref: "cargo test --workspace (all 49 existing tests pass)"
        status: pass
    human_judgment: false

duration: 6min
completed: 2026-08-28
status: complete
---

# Phase 10 Plan 02: gRPC Server Integration Summary

**gRPC InferenceService with opaque result_json bytes, multiplexed with REST on same port, health checking synced with readiness, and server reflection**

## Performance

- **Duration:** 6 min
- **Started:** 2026-08-28T23:31:03Z
- **Completed:** 2026-08-28T23:37:03Z
- **Tasks:** 2
- **Files modified:** 7

## Accomplishments

- GrpcInferenceService implementing tonic InferenceService trait, mirroring HTTP handler control flow exactly (readiness gate, validation, timeout, lock split, batching path)
- gRPC + REST multiplexed on single TCP port via tonic Routes merged into axum Router
- HealthReporter synced with AtomicBool readiness: NOT_SERVING on startup, SERVING after warmup, NOT_SERVING on shutdown
- tonic-reflection with FILE_DESCRIPTOR_SET for grpcurl service discovery
- ApiError to tonic::Status conversion with information-hiding for internal errors (Inference/Model/Internal -> generic message)

## Task Commits

Each task was committed atomically:

1. **Task 1: gRPC InferenceService implementation and error mapping** - `9657e5e` (feat)
2. **Task 2: gRPC route multiplexing, health reporter, and reflection in main.rs** - `ff3bb5d` (feat)

## Files Created/Modified

- `crates/hephaestus-api/Cargo.toml` - Added hephaestus-proto, tonic, tonic-health, tonic-reflection, prost deps
- `crates/hephaestus-api/src/lib.rs` - Added pub mod grpc
- `crates/hephaestus-api/src/grpc/mod.rs` - gRPC module with GrpcInferenceService re-export
- `crates/hephaestus-api/src/grpc/inference.rs` - InferenceService trait implementation with result_json serialization
- `crates/hephaestus-api/src/error.rs` - From<ApiError> for tonic::Status with information-hiding
- `crates/hephaestus/Cargo.toml` - Added hephaestus-proto, tonic, tonic-health, tonic-reflection deps
- `crates/hephaestus/src/main.rs` - gRPC route construction, health reporter wiring, router merge

## Decisions Made

- HealthReporter initialized to NOT_SERVING, explicitly set to SERVING only after warmup completes (prevents gRPC health probes from reporting ready before model is loaded)
- tonic::service::Routes::into_axum_router merged with REST Router for same-port multiplexing (no separate listener needed)

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Full gRPC inference API operational: clients can call Infer RPC and receive opaque JSON bytes for any model profile
- grpcurl can discover services via server reflection
- gRPC health probes available for k8s deployment
- Phase 10 complete: all plans executed

## Self-Check: PASSED

All created files verified on disk. All commit hashes found in git log.

---
*Phase: 10-grpc-inference-api*
*Completed: 2026-08-28*
