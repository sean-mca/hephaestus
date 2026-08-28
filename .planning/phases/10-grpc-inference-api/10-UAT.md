---
status: testing
phase: 10-grpc-inference-api
source: [10-VERIFICATION.md]
started: 2026-08-28
updated: 2026-08-28
---

## Current Test

number: 1
name: gRPC health check state transitions
expected: |
  NOT_SERVING before warmup completes, SERVING after warmup, NOT_SERVING after SIGTERM
awaiting: user response

## Tests

### 1. gRPC health check state transitions
expected: Start binary with MODEL_ID + STORAGE_TYPE=none. Use grpcurl grpc.health.v1.Health/Check (service name '' and 'hephaestus.v1.InferenceService'). Observe NOT_SERVING before warmup, SERVING after warmup, NOT_SERVING after SIGTERM.
result: [pending]

### 2. gRPC reflection and Infer RPC end-to-end
expected: grpcurl -plaintext localhost:<port> list shows hephaestus.v1.InferenceService, grpc.health.v1.Health, grpc.reflection.v1.ServerReflection. grpcurl Infer call returns model_id, latency_ms, result_json fields.
result: [pending]

## Summary

total: 2
passed: 0
issues: 0
pending: 2
skipped: 0
blocked: 0

## Gaps
