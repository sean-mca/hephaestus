---
status: testing
phase: 03-model-resolution
source: [03-VERIFICATION.md]
started: 2026-08-26T00:00:00Z
updated: 2026-08-26T00:00:00Z
---

## Current Test

number: 1
name: S3 cache hit resolves without HuggingFace contact
expected: |
  'model resolved from S3 cache' logged (tier=s3); no HuggingFace request occurs; pod becomes ready.
awaiting: user response

## Tests

### 1. S3 cache hit resolves without HuggingFace contact
expected: Set S3_BUCKET to a real bucket pre-populated with model files under {prefix}/{model_id}/. Start pod with matching MODEL_ID, no MODEL_PATH. Watch logs — 'model resolved from S3 cache' logged, no HF request, pod becomes ready.
result: [pending]

### 2. Background cache-back uploads to S3 after HF download
expected: Start pod with MODEL_ID pointing at an HF model with ONNX export, S3_BUCKET configured, cold cache. After readiness, model.onnx/tokenizer.json/config.json appear in S3. Pod did not wait on upload to become ready.
result: [pending]

### 3. MODEL_ID-only deploy downloads from HuggingFace and serves inference
expected: Start pod with MODEL_ID=Xenova/distilbert-base-uncased-finetuned-sst-2-english, no MODEL_PATH, no S3_BUCKET. Model downloads from HF, pipeline builds, warmup passes, classification request returns valid label + score.
result: [pending]

## Summary

total: 3
passed: 0
issues: 0
pending: 3
skipped: 0
blocked: 0

## Gaps
