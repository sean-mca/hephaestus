# Phase 11: WebSocket Streaming & ASR Pipeline - Context

**Gathered:** 2026-08-29
**Status:** Ready for planning

<domain>
## Phase Boundary

Add a WebSocket streaming endpoint for real-time audio inference and an ASR model profile to Hephaestus. Clients open a WebSocket connection per audio channel, stream mono PCM audio frames, and receive transcript fragments back in real-time. The ASR pipeline handles optional feature extraction (mel spectrograms) and ONNX model inference. Existing REST and gRPC endpoints remain unchanged.

</domain>

<decisions>
## Implementation Decisions

### Pipeline Input Contract
- **D-01:** Generalize `PipelineKind` with an `InferenceInput` enum (`Text(String)` / `Audio(Vec<f32>)`). All existing profiles match on `Text`, ASR matches on `Audio`. Mismatched input/profile returns `CoreError::InvalidInput`.
- **D-02:** Introduce a typed `PipelineOutput` enum (e.g., `ClassifierOutput`, `EmbeddingsOutput`, `AsrOutput`) instead of returning raw `serde_json::Value` from `execute_batch()`.
- **D-03:** Each WebSocket connection runs inference independently — no cross-connection batching. One `session.run()` per chunk per connection.
- **D-04:** Hephaestus only handles mono audio. Stereo is handled by the client opening two separate WebSocket connections with different channel labels (e.g., `?channel=agent` and `?channel=caller`). No stereo demuxing in Hephaestus.

### WebSocket Protocol
- **D-05:** Session config via query params at connect: `/ws/transcribe?sample_rate=16000&channel=agent&encoding=f32`. Fixed for session duration. Invalid params rejected with 400.
- **D-06:** Supported audio encodings: `f32` (32-bit float PCM) and `i16` (16-bit integer PCM). Client declares encoding in query params. Hephaestus converts i16 to f32 internally.
- **D-07:** Transcript output is model's decoded text wrapped with minimal connection context (channel label, chunk index). Hephaestus does not impose an opinionated transcript schema — the model's output drives the content.

### Audio Preprocessing
- **D-08:** Require 16kHz sample rate from client. Reject other rates at connection time. No resampling in Hephaestus.
- **D-09:** Feature extraction controlled via `FEATURE_EXTRACTOR` env var: `mel` (compute mel spectrograms in Rust for Whisper-style models) or `none` (pass raw waveform for wav2vec2/Parakeet-style models). Default: `none`.

### Streaming Chunking Strategy
- **D-10:** Fixed sliding window with configurable overlap (~1s default). Window size configurable (30s default for Whisper). Overlap prevents word-splitting artifacts at chunk boundaries — decode both chunks, discard overlap edges, keep stable middle.
- **D-11:** Chunking strategy configurable via `CHUNKING_STRATEGY` env var: `windowed` (fixed window with overlap, for encoder-decoder models like Whisper) or `streaming` (pass-through, for native streaming models like wav2vec2/Parakeet). Default: `windowed`.

### Claude's Discretion
- FFT crate selection for mel spectrogram computation (rustfft, realfft, or similar)
- WebSocket frame size and backpressure handling
- Overlap deduplication strategy (how to reconcile overlapping transcript segments)
- `PreparedInput` adaptation for audio features (mel frames vs token IDs)
- Exact env var naming and validation

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Pipeline Architecture
- `crates/hephaestus-core/src/pipeline.rs` — Pipeline trait, PipelineKind enum dispatch, prepare()/execute_batch() signatures (must be generalized for InferenceInput/PipelineOutput)
- `crates/hephaestus-core/src/profile.rs` — ModelProfile enum, detect_profile(), parse_profile_string() (extend with Asr variant)

### Serving Layer
- `crates/hephaestus/src/main.rs` — Server setup, router construction, gRPC+REST multiplexing (WebSocket endpoint merges here)
- `crates/hephaestus-api/src/routes.rs` — REST route registration (add WebSocket route here)
- `crates/hephaestus-api/src/handlers.rs` — HTTP handler patterns (control flow template for WebSocket handler)

### Prior Phase Patterns
- `crates/hephaestus-api/src/grpc/inference.rs` — gRPC handler uses same AppState/lock patterns the WebSocket handler will follow

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `Pipeline` trait with `prepare()`/`execute()` — ASR pipeline follows this pattern with audio bytes as Input
- `AppState` with `read_pipeline()`/`write_pipeline()` RwLock split — WebSocket handler reuses for concurrent access
- `StageTimer` for per-request metrics instrumentation
- `ApiError` enum for error handling (extend with audio-specific variants)
- `build_router()` in routes.rs — add WebSocket route alongside existing REST routes

### Established Patterns
- `PipelineKind` enum dispatch — add `Asr(AsrPipeline)` variant
- `ModelProfile` enum with `detect_profile()` from config.json architectures — add ASR detection
- `MODEL_PROFILE` env var override pattern — same pattern for `FEATURE_EXTRACTOR` and `CHUNKING_STRATEGY`
- axum handler patterns with `Arc<AppState>` extraction

### Integration Points
- `PipelineKind` enum in `pipeline.rs` — new `Asr` variant, generalized `InferenceInput`/`PipelineOutput`
- `ModelProfile` enum in `profile.rs` — new `Asr` variant + detection logic
- `main.rs` — new pipeline construction branch in the `match profile {}` block
- `routes.rs` — WebSocket route registration (`axum::extract::ws`)
- `Cargo.toml` workspace deps — FFT crate (for mel spectrograms), axum WebSocket feature

</code_context>

<specifics>
## Specific Ideas

- Hephaestus is a model inference runtime — it passes model output through, not an opinionated transcript API
- Env var configuration follows established MODEL_PROFILE pattern — explicit operator control
- 16kHz is universal for ASR models (Whisper, NeMo, wav2vec2, HuBERT) — safe to require from clients

</specifics>

<deferred>
## Deferred Ideas

- Voice Activity Detection (VAD) for intelligent segmentation — future enhancement on top of fixed windowing
- Speaker diarization on mono audio
- Word-level timestamps within transcript fragments
- Streaming partial/interim results within a window (before window completes)
- Cross-connection batching for GPU throughput optimization
- Resampling support (accepting non-16kHz audio)

</deferred>

---

*Phase: 11-websocket-streaming-asr-pipeline*
*Context gathered: 2026-08-29*
