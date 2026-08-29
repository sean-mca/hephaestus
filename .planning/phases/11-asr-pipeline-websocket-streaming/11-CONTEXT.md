# Phase 11: ASR Pipeline & WebSocket Streaming - Context

**Gathered:** 2026-08-29
**Status:** Ready for planning

<domain>
## Phase Boundary

Add an ASR (automatic speech recognition) model profile to Hephaestus with a WebSocket streaming endpoint for real-time transcription. Supports mono and stereo audio with configurable channel labels for call-center use cases. The ASR pipeline handles audio preprocessing (format normalization, resampling, feature extraction) and ONNX model inference, producing transcript text. The WebSocket endpoint accepts audio frames and streams JSON transcript fragments back to the client.

</domain>

<decisions>
## Implementation Decisions

### Audio Input Contract
- **D-01:** Supported audio encodings: linear16, linear8, alaw, mulaw (all PCM-family, no codec dependencies)
- **D-02:** Client declares format via query params at WebSocket connect: `/ws/transcribe?encoding=mulaw&sample_rate=8000&channels=1`. Format is fixed for the session duration.
- **D-03:** Server normalizes all input to f32 PCM at the model's expected sample rate (16kHz for Whisper). A-law and mu-law decoding are lookup-table conversions. Sample rate conversion via a Rust resampling crate.

### Target ASR Model
- **D-04:** Generic ASR pipeline design — abstract the feature extraction step so both mel-spectrogram models (Whisper) and raw-waveform models (wav2vec2/HuBERT) work.
- **D-05:** Feature extractor is a trait or enum dispatched at model load time based on config.json detection (same pattern as existing `detect_profile`).
- **D-06:** First validation model: Whisper ONNX (hardest preprocessing — mel spectrogram extraction; if it works, simpler models follow).
- **D-07:** Extend `ModelProfile` enum with `Asr` variant. Extend `detect_profile` and `parse_profile_string` to recognize ASR model configs.

### Streaming Strategy
- **D-08:** Fixed sliding window chunking — configurable window size (30s default for Whisper), slight overlap to avoid splitting mid-word. Emit transcript after each window completes.
- **D-09:** Transcript fragments are JSON: `{ "text": "...", "is_final": true, "start_ms": 0, "end_ms": 30000, "channel": 0 }`. Matches industry conventions (Deepgram/AWS Transcribe style).
- **D-10:** Predictable latency — client receives a transcript every window-length interval.

### Stereo/Channel Protocol
- **D-11:** Configurable channel labels via `channel_labels=agent,caller` query param. Defaults to `channel_0`, `channel_1` if not specified.
- **D-12:** Stereo execution uses two independent pipeline calls — demux stereo to two mono streams, buffer and infer independently, tag transcript fragments with the channel label.
- **D-13:** Same pipeline code path for both channels. No special stereo logic inside the ASR pipeline — stereo handling is purely in the WebSocket handler's routing layer.

### Claude's Discretion
- Audio resampling crate selection (rubato, dasp, or similar) — researcher should evaluate
- Mel spectrogram implementation approach (pure Rust FFT vs existing crate)
- WebSocket frame size / backpressure handling
- Overlap strategy details (how many ms of overlap between windows)

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Pipeline Architecture
- `crates/hephaestus-core/src/pipeline.rs` — Pipeline trait (Input/Prepared/Output associated types), PipelineKind enum dispatch
- `crates/hephaestus-core/src/profile.rs` — ModelProfile enum, detect_profile(), parse_profile_string()

### Serving Layer
- `crates/hephaestus/src/main.rs` — Server setup, router construction, gRPC+REST multiplexing (WebSocket endpoint merges here)
- `crates/hephaestus-api/src/routes.rs` — REST route registration (add WebSocket route here)
- `crates/hephaestus-api/src/handlers.rs` — HTTP handler patterns (control flow template for WebSocket handler)

### Prior Phase Context
- `crates/hephaestus-api/src/grpc/inference.rs` — gRPC handler mirrors HTTP handler; WebSocket handler follows same AppState/lock patterns

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `Pipeline` trait with `prepare()`/`execute()` pattern — ASR pipeline follows this exactly, with audio bytes as Input instead of String
- `AppState` with `read_pipeline()`/`write_pipeline()` RwLock split — WebSocket handler reuses for concurrent tokenization / exclusive inference
- `StageTimer` for per-request metrics instrumentation
- `ApiError` enum for error handling (extend with audio-specific variants)

### Established Patterns
- `PipelineKind` enum dispatch — add `Asr(AsrPipeline)` variant, all dispatch sites get a new arm
- Profile detection from model `config.json` — extend the existing matching logic
- `PipelineKind::prepare()` currently hardcodes `String` input — needs adaptation for audio (the ASR variant takes `Vec<f32>` or audio bytes, not text)

### Integration Points
- `PipelineKind` enum in `pipeline.rs` — new `Asr` variant
- `ModelProfile` enum in `profile.rs` — new `Asr` variant + detection logic
- `main.rs` — new pipeline construction branch in the `match profile {}` block
- `routes.rs` or `main.rs` — WebSocket route registration (axum::extract::ws)
- `Cargo.toml` workspace deps — audio processing crates (resampler, FFT)

</code_context>

<specifics>
## Specific Ideas

- Call-center audio is the primary use case — A-law/mu-law support is essential (G.711 telephony codec)
- Channel labels should be meaningful for call-center workflows (agent/caller, not just left/right)
- The WebSocket API should feel like Deepgram/AWS Transcribe — query-param config, JSON transcript fragments

</specifics>

<deferred>
## Deferred Ideas

- Voice Activity Detection (VAD) for intelligent segmentation — future enhancement on top of fixed windowing
- Speaker diarization on mono audio (identifying speakers without channel separation)
- Word-level timestamps within transcript fragments
- Streaming partial/interim results within a window (before window completes)

</deferred>

---

*Phase: 11-asr-pipeline-websocket-streaming*
*Context gathered: 2026-08-29*
