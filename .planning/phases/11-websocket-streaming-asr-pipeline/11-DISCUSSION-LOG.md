# Phase 11: WebSocket Streaming & ASR Pipeline - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-08-29
**Phase:** 11-websocket-streaming-asr-pipeline
**Areas discussed:** Pipeline input contract, WebSocket protocol design, Audio preprocessing scope, Streaming chunking strategy

---

## Pipeline Input Contract

| Option | Description | Selected |
|--------|-------------|----------|
| Separate ASR path | ASR pipeline lives outside PipelineKind, WebSocket handler calls it directly | |
| Generalize PipelineKind input | Make prepare() accept InferenceInput enum (Text/Audio) | ✓ |
| You decide | Let researcher/planner figure out cleanest approach | |

**User's choice:** Generalize PipelineKind input
**Notes:** User wanted ASR to be a first-class citizen in the existing enum dispatch, not a separate path.

### Output type

| Option | Description | Selected |
|--------|-------------|----------|
| Keep serde_json::Value | All profiles return raw JSON | |
| Typed output enum | PipelineOutput enum (ClassifierOutput, AsrOutput, etc.) | ✓ |

**User's choice:** Typed output enum
**Notes:** User confirmed this is the cleaner option. Also clarified that execute_batch() is not the right mechanism for streaming — ASR uses sequential single-inference per chunk, not batching.

### Execution path

| Option | Description | Selected |
|--------|-------------|----------|
| Independent per connection | Each WebSocket connection calls transcribe() independently | ✓ |
| Batch across connections | Funnel chunks from multiple connections through batching system | |

**User's choice:** Independent per connection

### Stereo handling

| Option | Description | Selected |
|--------|-------------|----------|
| Mono only | Single-channel only, stereo deferred | |
| Mono + stereo | Support both, stereo demuxed to two mono streams | ✓ |

**User's choice:** Mono + stereo
**Notes:** User clarified that the upstream audio service will send two separate mono streams, not interleaved stereo. So Hephaestus only handles mono — stereo is the client's responsibility (two connections with channel labels).

---

## WebSocket Protocol Design

### Session config

| Option | Description | Selected |
|--------|-------------|----------|
| Query params at connect | Config via URL params, fixed for session | ✓ |
| JSON config message first | Client sends config JSON after connecting, server ACKs | |

**User's choice:** Query params at connect

### Audio encoding

| Option | Description | Selected |
|--------|-------------|----------|
| Raw PCM f32 only | 32-bit float samples only | |
| Raw PCM i16 only | 16-bit integer samples only | |
| Support both (f32 and i16) | Client declares encoding in query params | ✓ |

**User's choice:** Support both

### Transcript output

**User's choice:** Minimal context wrapper
**Notes:** User pushed back on designing an opinionated transcript schema. Hephaestus is a model inference runtime — it passes model output through. The output wraps model text with channel label and chunk index, nothing more.

---

## Audio Preprocessing Scope

### Resampling

| Option | Description | Selected |
|--------|-------------|----------|
| Hephaestus resamples | Accept any rate, resample to 16kHz internally | |
| Require 16kHz from client | Reject non-16kHz at connection time | ✓ |

**User's choice:** Require 16kHz from client

### Feature extraction

| Option | Description | Selected |
|--------|-------------|----------|
| Rust-side feature extraction | Hephaestus computes mel spectrograms | |
| Model graph handles it | Expect ONNX graph to include preprocessing | |
| Configurable per model via env var | FEATURE_EXTRACTOR=mel or none | ✓ |

**User's choice:** Configurable via env var
**Notes:** User asked for explanation of feature extraction differences between Whisper (needs mel spectrograms) and wav2vec2 (takes raw waveform). Chose env var approach over preprocessor_config.json parsing — same pattern as MODEL_PROFILE.

---

## Streaming Chunking Strategy

**User's choice:** Fixed window with configurable overlap
**Notes:** User asked which approach is best practice. Research confirmed overlap is required for production quality — no-overlap causes systematic artifacts at boundaries. User then asked whether windowed chunking would break native streaming models. Solution: CHUNKING_STRATEGY env var (windowed/streaming) so both model types work.

---

## Claude's Discretion

- FFT crate selection for mel spectrogram computation
- WebSocket frame size and backpressure handling
- Overlap deduplication strategy
- PreparedInput adaptation for audio features
- Exact env var naming and validation

## Deferred Ideas

- Voice Activity Detection (VAD) for intelligent segmentation
- Speaker diarization on mono audio
- Word-level timestamps within transcript fragments
- Streaming partial/interim results within a window
- Cross-connection batching for GPU throughput
- Resampling support (accepting non-16kHz audio)
