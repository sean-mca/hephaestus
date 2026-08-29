---
phase: 11-websocket-streaming-asr-pipeline
plan: 02
subsystem: api
tags: [websocket, axum, pcm, audio, streaming, futures-util]

requires:
  - phase: 10-grpc-inference-api
    provides: axum router with multiplexed gRPC/HTTP serving
provides:
  - WebSocket endpoint at /ws/transcribe with query param validation
  - AudioBuffer with windowed chunking and overlap for streaming audio
  - PCM encoding conversion (i16 and f32 to normalized f32 samples)
  - TranscriptMessage JSON shape for client responses
affects: [11-03-PLAN, asr-pipeline-integration]

tech-stack:
  added: [futures-util, axum ws feature]
  patterns: [WebSocket upgrade with pre-validation, windowed audio buffering with overlap]

key-files:
  created: [crates/hephaestus-api/src/ws.rs]
  modified: [Cargo.toml, crates/hephaestus-api/Cargo.toml, crates/hephaestus-api/src/lib.rs, crates/hephaestus-api/src/routes.rs]

key-decisions:
  - "AudioBuffer caps at 2x window_samples to prevent memory exhaustion from fast senders (T-11-04)"
  - "30-second idle timeout on WebSocket recv to prevent connection slot exhaustion (T-11-05)"
  - "TranscriptMessage.text is empty string until Plan 11-03 wires ASR inference"
  - "futures-util used for StreamExt/SinkExt on WebSocket split sender/receiver"

patterns-established:
  - "WebSocket handler pattern: validate params at upgrade time, reject before allocating resources"
  - "Audio buffering: windowed chunking with configurable overlap for streaming processing"
  - "PCM conversion: chunks_exact ignores trailing bytes for malformed-input safety"

requirements-completed: [APIX-02]

coverage:
  - id: D1
    description: "WebSocket endpoint at /ws/transcribe accepts connections with query param validation"
    requirement: "APIX-02"
    verification:
      - kind: unit
        ref: "crates/hephaestus-api/src/ws.rs#audio_encoding_from_str_valid"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-api/src/ws.rs#audio_encoding_from_str_invalid"
        status: pass
    human_judgment: false
  - id: D2
    description: "AudioBuffer accumulates PCM samples and emits windows at configured size with overlap"
    verification:
      - kind: unit
        ref: "crates/hephaestus-api/src/ws.rs#audio_buffer_computes_correct_window_and_overlap"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-api/src/ws.rs#audio_buffer_push_returns_one_window_at_exact_size"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-api/src/ws.rs#audio_buffer_push_returns_multiple_windows"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-api/src/ws.rs#audio_buffer_push_drains_correctly_with_overlap"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-api/src/ws.rs#audio_buffer_caps_at_max_size"
        status: pass
    human_judgment: false
  - id: D3
    description: "i16 and f32 PCM bytes correctly converted to normalized f32 samples"
    verification:
      - kind: unit
        ref: "crates/hephaestus-api/src/ws.rs#i16_bytes_to_f32_known_value"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-api/src/ws.rs#i16_bytes_to_f32_negative_value"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-api/src/ws.rs#i16_bytes_to_f32_zero"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-api/src/ws.rs#i16_bytes_to_f32_empty_input"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-api/src/ws.rs#i16_bytes_to_f32_odd_length_ignores_trailing"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-api/src/ws.rs#f32_bytes_to_samples_roundtrip"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-api/src/ws.rs#f32_bytes_to_samples_empty"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-api/src/ws.rs#f32_bytes_to_samples_trailing_bytes_ignored"
        status: pass
    human_judgment: false
  - id: D4
    description: "AudioBuffer flush returns remaining samples or None on empty buffer"
    verification:
      - kind: unit
        ref: "crates/hephaestus-api/src/ws.rs#audio_buffer_flush_returns_none_on_empty"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-api/src/ws.rs#audio_buffer_flush_returns_remaining_samples"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-api/src/ws.rs#audio_buffer_flush_increments_chunk_index"
        status: pass
    human_judgment: false
  - id: D5
    description: "Existing REST and gRPC routes unchanged after WebSocket route registration"
    verification:
      - kind: integration
        ref: "cargo test --workspace -q (all 173 tests pass)"
        status: pass
    human_judgment: false

duration: 3min
completed: 2026-08-29
status: complete
---

# Phase 11 Plan 02: WebSocket Streaming Infrastructure Summary

**WebSocket transport layer with PCM audio buffering, windowed chunking, encoding conversion, and param validation at /ws/transcribe**

## Performance

- **Duration:** 3 min
- **Started:** 2026-08-29T12:13:00Z
- **Completed:** 2026-08-29T12:16:23Z
- **Tasks:** 2
- **Files modified:** 5

## Accomplishments

- WebSocket endpoint at /ws/transcribe with pre-upgrade query param validation (sample_rate, encoding, channel)
- AudioBuffer with configurable window/overlap durations, 2x max-size memory cap, and flush for connection close
- PCM conversion functions for i16 (normalized to [-1.0, 1.0]) and f32 little-endian byte formats
- 19 unit tests covering buffer windowing, overlap draining, PCM conversion edge cases, and encoding parsing
- Route registered in build_router alongside existing /infer, /healthz/*, /metrics endpoints

## Task Commits

Each task was committed atomically:

1. **Task 1: WebSocket handler module with audio buffer, PCM conversion, and param validation** - `8e44944` (feat)
2. **Task 2: Register WebSocket route and add workspace dependencies** - `c27ae1e` (feat)

## Files Created/Modified

- `crates/hephaestus-api/src/ws.rs` - WebSocket handler module: TranscribeParams, AudioEncoding, AudioBuffer, TranscriptMessage, PCM conversion, ws_transcribe handler, handle_transcribe_socket
- `crates/hephaestus-api/src/routes.rs` - Added /ws/transcribe route in build_router
- `crates/hephaestus-api/src/lib.rs` - Added pub mod ws declaration
- `crates/hephaestus-api/Cargo.toml` - Added futures-util workspace dependency
- `Cargo.toml` - Added axum ws feature and futures-util workspace dependency

## Decisions Made

- AudioBuffer caps at 2x window_samples to prevent memory exhaustion from fast senders (T-11-04 mitigation)
- 30-second idle timeout via tokio::time::timeout on each recv to close idle connections (T-11-05 mitigation)
- TranscriptMessage.text set to empty string (placeholder) -- Plan 11-03 wires actual ASR inference
- futures-util for StreamExt/SinkExt traits on WebSocket split, consistent with Rust async ecosystem
- chunks_exact used for PCM conversion to silently ignore trailing incomplete bytes (T-11-02 mitigation)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed unused mut warning on socket parameter**
- **Found during:** Task 2 (workspace compilation)
- **Issue:** Compiler warning: `mut socket: WebSocket` unnecessary since socket is consumed by `.split()`
- **Fix:** Removed `mut` qualifier from the `socket` parameter in `handle_transcribe_socket`
- **Files modified:** crates/hephaestus-api/src/ws.rs
- **Verification:** Clean compilation with no warnings
- **Committed in:** c27ae1e (Task 2 commit)

---

**Total deviations:** 1 auto-fixed (1 bug)
**Impact on plan:** Trivial warning fix. No scope creep.

## Issues Encountered

None

## Known Stubs

| File | Line | Description | Resolution |
|------|------|-------------|------------|
| crates/hephaestus-api/src/ws.rs | TranscriptMessage.text | Empty string placeholder for transcript text | Plan 11-03 wires ASR inference to populate this field |

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- WebSocket transport layer complete and registered at /ws/transcribe
- Plan 11-03 wires the ASR pipeline: handle_transcribe_socket calls into PipelineKind for actual transcription
- AudioBuffer API is stable: push() returns windows, flush() returns remainder
- TranscriptMessage JSON shape established for client consumption

---
*Phase: 11-websocket-streaming-asr-pipeline*
*Completed: 2026-08-29*

## Self-Check: PASSED

- All 5 files verified present on disk
- Commit 8e44944 verified in git log
- Commit c27ae1e verified in git log
