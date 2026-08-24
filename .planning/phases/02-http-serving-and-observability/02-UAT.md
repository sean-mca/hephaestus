---
status: testing
phase: 02-http-serving-and-observability
source: [02-VERIFICATION.md]
started: 2026-08-24T23:45:00Z
updated: 2026-08-24T23:45:00Z
---

## Current Test

number: 1
name: Full /infer round trip against a real model (API-01)
expected: |
  JSON classification result with label, score, model_id, latency_ms populated from a real inference pass
awaiting: user response

## Tests

### 1. Full /infer round trip against a real model (API-01)
expected: POST {"text": "..."} to /infer with MODEL_PATH pointing at real .onnx + tokenizer.json returns 200 with label/score/model_id/latency_ms
result: [pending]

### 2. Readiness 503->200 state transition (API-03)
expected: GET /healthz/ready returns 503 before warmup completes, 200 after the ready flag flips
result: [pending]

### 3. SIGTERM drain behavior (API-04)
expected: Send SIGTERM while a request is in flight; readiness flips to 503 immediately, in-flight request completes, process exits within SHUTDOWN_TIMEOUT_SECS
result: [pending]

### 4. Request timeout returns 504 (CORE-04)
expected: POST /infer against a slow pipeline exceeding REQUEST_TIMEOUT_SECS returns 504 with error.code == INFERENCE_TIMEOUT
result: [pending]

### 5. OTLP export to live collector (OBSV-03)
expected: With OTEL_EXPORTER_OTLP_ENDPOINT set to a running collector, spans arrive at the collector after making a request
result: [pending]

## Summary

total: 5
passed: 0
issues: 0
pending: 5
skipped: 0
blocked: 0

## Gaps
