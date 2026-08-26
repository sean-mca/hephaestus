---
status: testing
phase: 05-forge-conversion-service
source: [05-VERIFICATION.md]
started: 2026-08-26T19:50:00Z
updated: 2026-08-26T19:50:00Z
---

## Current Test

number: 1
name: ConversionQueue concurrency and deduplication behavior
expected: |
  Only one conversion runs per model_id at a time. Concurrent callers for the same model_id block and receive the same result. The asyncio.Semaphore(1) enforces sequential conversion, and per-model asyncio.Lock prevents duplicate work.
awaiting: user response

## Tests

### 1. ConversionQueue concurrency and deduplication behavior
expected: Only one conversion runs per model_id at a time. Concurrent callers for the same model_id block and receive the same result. The asyncio.Semaphore(1) enforces sequential conversion, and per-model asyncio.Lock prevents duplicate work.
result: [pending]

### 2. Docker image build
expected: `docker build -t forge:test forge/` completes successfully. The multi-stage build installs dependencies via uv, copies source, and the resulting image starts with uvicorn serving the FastAPI app on port 8080.
result: [pending]

## Summary

total: 2
passed: 0
issues: 0
pending: 2
skipped: 0
blocked: 0

## Gaps
