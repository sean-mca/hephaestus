---
status: testing
phase: 11-websocket-streaming-asr-pipeline
source: [11-VERIFICATION.md]
started: 2026-08-29T14:00:00Z
updated: 2026-08-29T14:00:00Z
---

## Current Test

number: 1
name: Real-time WebSocket ASR transcription with a live CTC model
expected: |
  Transcript fragments arrive as JSON TranscriptMessage objects with correct chunk_index and reasonably accurate text, delivered incrementally per completed window.
awaiting: user response

## Tests

### 1. Real-time WebSocket ASR transcription with a live CTC model
expected: Non-empty, accurate transcript fragments streamed back as JSON TranscriptMessage objects while audio is sent; connection stays open across multiple windows; flush on close returns the final short window.
result: [pending]

### 2. Whisper ONNX end-to-end inference
expected: Decoder session accepts the encoder_hidden_states input name and produces valid text; loop terminates at eos_token_id well before max_target_positions for normal utterances.
result: [pending]

### 3. Perceived real-time streaming latency
expected: Transcript fragments are pushed incrementally per completed window, not batched at connection close.
result: [pending]

## Summary

total: 3
passed: 0
issues: 0
pending: 3
skipped: 0
blocked: 0

## Gaps
