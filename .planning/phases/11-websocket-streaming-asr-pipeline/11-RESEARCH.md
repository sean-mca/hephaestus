# Phase 11: WebSocket Streaming & ASR Pipeline - Research

**Researched:** 2026-08-29
**Domain:** Real-time audio inference via WebSocket, ASR model profiles (Whisper, wav2vec2), mel spectrogram computation, CTC decoding
**Confidence:** HIGH

## Summary

Phase 11 adds two major capabilities to Hephaestus: (1) a WebSocket streaming endpoint for real-time audio transcription, and (2) an ASR model profile that handles audio preprocessing (mel spectrograms or raw waveform pass-through) and ONNX inference for speech-to-text models.

The WebSocket endpoint uses axum's built-in `ws` feature (already bundled with axum 0.8, just needs the feature flag enabled). Clients stream mono 16kHz PCM audio frames (f32 or i16 encoding) over WebSocket binary messages, and receive JSON transcript fragments back. Session configuration is passed via query parameters at connect time.

The ASR pipeline introduces a fundamentally different input type (audio samples vs. text tokens). Per D-01, the existing `PipelineKind` will be generalized with an `InferenceInput` enum. Two distinct preprocessing paths are needed: mel spectrogram computation for Whisper-family encoder-decoder models, and raw waveform pass-through for CTC models like wav2vec2. The `mel_spec` crate provides Whisper-compatible mel spectrogram computation in pure Rust, validated against whisper.cpp and librosa reference implementations. CTC greedy decoding is straightforward to implement (20-30 lines of Rust) and does not warrant an external dependency.

**Primary recommendation:** Use `mel_spec` for Whisper-compatible mel spectrograms, axum's built-in WebSocket support with the `ws` feature flag, and hand-roll CTC greedy decoding. The ASR pipeline should be a new `AsrPipeline` struct implementing the `Pipeline` trait with `Input = Vec<f32>` (audio samples), `Prepared` as a new `PreparedAudio` type, and `Output = String` (decoded text).

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- **D-01:** Generalize `PipelineKind` with an `InferenceInput` enum (`Text(String)` / `Audio(Vec<f32>)`). All existing profiles match on `Text`, ASR matches on `Audio`. Mismatched input/profile returns `CoreError::InvalidInput`.
- **D-02:** Introduce a typed `PipelineOutput` enum (e.g., `ClassifierOutput`, `EmbeddingsOutput`, `AsrOutput`) instead of returning raw `serde_json::Value` from `execute_batch()`.
- **D-03:** Each WebSocket connection runs inference independently -- no cross-connection batching. One `session.run()` per chunk per connection.
- **D-04:** Hephaestus only handles mono audio. Stereo is handled by the client opening two separate WebSocket connections with different channel labels.
- **D-05:** Session config via query params at connect: `/ws/transcribe?sample_rate=16000&channel=agent&encoding=f32`. Fixed for session duration. Invalid params rejected with 400.
- **D-06:** Supported audio encodings: `f32` (32-bit float PCM) and `i16` (16-bit integer PCM). Client declares encoding in query params. Hephaestus converts i16 to f32 internally.
- **D-07:** Transcript output is model's decoded text wrapped with minimal connection context (channel label, chunk index).
- **D-08:** Require 16kHz sample rate from client. Reject other rates at connection time. No resampling in Hephaestus.
- **D-09:** Feature extraction controlled via `FEATURE_EXTRACTOR` env var: `mel` (compute mel spectrograms in Rust for Whisper-style models) or `none` (pass raw waveform for wav2vec2/Parakeet-style models). Default: `none`.
- **D-10:** Fixed sliding window with configurable overlap (~1s default). Window size configurable (30s default for Whisper). Overlap prevents word-splitting artifacts at chunk boundaries.
- **D-11:** Chunking strategy configurable via `CHUNKING_STRATEGY` env var: `windowed` (fixed window with overlap, for encoder-decoder models like Whisper) or `streaming` (pass-through, for native streaming models like wav2vec2/Parakeet). Default: `windowed`.

### Claude's Discretion
- FFT crate selection for mel spectrogram computation
- WebSocket frame size and backpressure handling
- Overlap deduplication strategy
- `PreparedInput` adaptation for audio features
- Exact env var naming and validation

### Deferred Ideas (OUT OF SCOPE)
- Voice Activity Detection (VAD) for intelligent segmentation
- Speaker diarization on mono audio
- Word-level timestamps within transcript fragments
- Streaming partial/interim results within a window
- Cross-connection batching for GPU throughput optimization
- Resampling support (accepting non-16kHz audio)
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| PRFX-01 | ASR profile (audio in, text out) | Whisper ONNX model structure, wav2vec2 input format, CTC decoding, mel spectrogram computation all researched |
| APIX-02 | Streaming inference (WebSocket) for models | axum WebSocket API, binary message handling, query parameter extraction, connection lifecycle all documented |
</phase_requirements>

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| WebSocket connection management | API / Serving (hephaestus-api) | -- | axum handles upgrade, connection lifecycle, message routing |
| Audio chunk buffering & windowing | API / Serving (hephaestus-api) | -- | Per-connection state, not model-specific logic |
| i16-to-f32 PCM conversion | API / Serving (hephaestus-api) | -- | Input normalization before pipeline |
| Mel spectrogram computation | Core (hephaestus-core) | -- | Model-specific preprocessing, part of the pipeline |
| ONNX inference (encoder/decoder) | Core (hephaestus-core) | -- | Existing pipeline pattern |
| CTC greedy decoding | Core (hephaestus-core) | -- | Model-specific post-processing |
| ASR model profile detection | Core (hephaestus-core) | -- | Extends existing profile.rs |
| Transcript JSON serialization | API / Serving (hephaestus-api) | -- | Response formatting for WebSocket |
| Configuration (new env vars) | Binary (hephaestus) | -- | Extends existing config.rs |

## Standard Stack

### Core (New Dependencies)

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `mel_spec` | 0.4.0 | Whisper-compatible mel spectrogram computation | Purpose-built for Whisper/ASR. Validated against whisper.cpp, PyTorch, and librosa reference outputs. Pure Rust. 33K weekly downloads. [ASSUMED] |
| `rustfft` | 6.4.1 | FFT computation (transitive via mel_spec) | De facto Rust FFT library. 563K weekly downloads. Used by mel_spec internally. [VERIFIED: crates.io registry, package-legitimacy OK] |
| `futures-util` | 0.3.34 | StreamExt::split() for WebSocket send/receive | Standard async utilities. Required for splitting WebSocket into sender/receiver for concurrent bidirectional communication. [VERIFIED: crates.io registry] |

### Existing Dependencies (Feature Additions)

| Library | Current Config | Required Change | Purpose |
|---------|---------------|-----------------|---------|
| `axum` | `{ version = "0.8", features = ["http2"] }` | Add `"ws"` feature | Enables `axum::extract::ws::WebSocketUpgrade` and `WebSocket` types for WebSocket handling |
| `tokio` | `features = ["rt-multi-thread", "macros", "signal", "sync", "time"]` | No change needed | Already has all required features for async WebSocket handling |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `mel_spec` | Hand-rolled FFT + mel filterbank | mel_spec is validated against reference implementations; hand-rolling risks subtle numerical differences that degrade ASR quality |
| `mel_spec` | `ruststft` (0.4.0) | ruststft has only 16 weekly downloads (flagged SUS), less ASR-specific; mel_spec is purpose-built for Whisper |
| Built-in CTC decode | External CTC crate | Greedy CTC decoding is ~25 lines of Rust (argmax + blank collapse + repeat removal). No crate needed. |
| `futures-util` | `tokio_stream` | futures-util is more widely used with axum WebSocket patterns; tokio_stream is an alternative but less documented for this use case |

**Installation (workspace Cargo.toml additions):**
```toml
mel_spec = "0.4"
futures-util = "0.3"
# Also: add "ws" to axum features
axum = { version = "0.8", features = ["http2", "ws"] }
```

## Package Legitimacy Audit

| Package | Registry | Age | Downloads | Source Repo | Verdict | Disposition |
|---------|----------|-----|-----------|-------------|---------|-------------|
| rustfft | crates.io | 11 yrs | 563K/wk | github.com/ejmahler/RustFFT | OK | Approved |
| realfft | crates.io | 6 yrs | 341K/wk | github.com/HEnquist/realfft | OK | Approved (transitive, not directly needed) |
| mel_spec | crates.io | 3 yrs | 33K/wk | github.com/wavey-ai/mel-spec | OK | Approved |
| ruststft | crates.io | 3 yrs | 16/wk | github.com/sunsided/stft | SUS | Not recommended (low downloads) |
| futures-util | crates.io | 8+ yrs | N/A (core ecosystem) | github.com/rust-lang/futures-rs | OK | Approved |

**Packages removed due to [SLOP] verdict:** none
**Packages flagged as suspicious [SUS]:** `ruststft` -- not recommended, use `mel_spec` instead

## Architecture Patterns

### System Architecture Diagram

```
Client (WebSocket)
    |
    | Binary frames (PCM audio: f32 or i16)
    | Query params: ?sample_rate=16000&channel=agent&encoding=f32
    v
+-------------------------------------------+
| axum WebSocket Handler (hephaestus-api)   |
|                                           |
|  1. Validate query params at connect      |
|  2. Accept upgrade -> WebSocket           |
|  3. Split into sender/receiver            |
|                                           |
|  recv loop:                               |
|    Binary msg -> decode PCM bytes         |
|    -> i16-to-f32 conversion (if i16)      |
|    -> append to audio buffer              |
|                                           |
|    When buffer >= window_size:            |
|      Extract window (with overlap)        |
|      |                                    |
|      v                                    |
| +---------------------------------------+|
| | ASR Pipeline (hephaestus-core)        ||
| |                                       ||
| | prepare(audio_samples: Vec<f32>):     ||
| |   if mel: compute mel spectrogram     ||
| |   if none: pass raw waveform          ||
| |   -> PreparedAudio                    ||
| |                                       ||
| | execute(prepared: PreparedAudio):     ||
| |   session.run(tensor) -> outputs      ||
| |   if Whisper: decode tokens via       ||
| |     tokenizer                         ||
| |   if CTC: greedy decode (argmax +     ||
| |     blank collapse)                   ||
| |   -> String (transcript text)         ||
| +---------------------------------------+|
|      |                                    |
|      v                                    |
|    Wrap in JSON: { "channel": "agent",    |
|      "chunk_index": N, "text": "..." }    |
|    -> send Text msg to client             |
+-------------------------------------------+
```

### Recommended Project Structure

```
crates/hephaestus-core/src/
  pipeline.rs          # Add AsrPipeline, InferenceInput, PipelineOutput, PreparedAudio
  profile.rs           # Add ModelProfile::Asr + detection logic
  mel.rs               # NEW: Mel spectrogram computation (wraps mel_spec)
  ctc.rs               # NEW: CTC greedy decoder
  error.rs             # Add audio-specific error variants

crates/hephaestus-api/src/
  ws.rs                # NEW: WebSocket handler, audio buffer, chunking logic
  ws/mod.rs            # Alternative: ws/ directory with handler.rs, buffer.rs, protocol.rs
  routes.rs            # Add WebSocket route
  error.rs             # Add WebSocket-specific ApiError variants
  handlers.rs          # Existing REST handlers (unchanged)

crates/hephaestus/src/
  config.rs            # Add FEATURE_EXTRACTOR, CHUNKING_STRATEGY, window config env vars
  main.rs              # Add AsrPipeline construction branch
```

### Pattern 1: axum WebSocket Handler with Query Parameters

**What:** Accept WebSocket upgrade with session configuration from query params
**When to use:** WebSocket endpoint that needs per-connection configuration at connect time

```rust
// Source: docs.rs/axum/latest/axum/extract/ws (verified) + training knowledge
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::response::IntoResponse;
use serde::Deserialize;
use std::sync::Arc;

#[derive(Debug, Deserialize)]
pub struct TranscribeParams {
    sample_rate: u32,
    channel: String,
    encoding: String, // "f32" or "i16"
}

pub async fn ws_transcribe(
    ws: WebSocketUpgrade,
    Query(params): Query<TranscribeParams>,
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, ApiError> {
    // Validate params before upgrade
    if params.sample_rate != 16000 {
        return Err(ApiError::BadRequest(
            "only 16kHz sample rate is supported".into(),
        ));
    }
    if !matches!(params.encoding.as_str(), "f32" | "i16") {
        return Err(ApiError::BadRequest(
            "encoding must be 'f32' or 'i16'".into(),
        ));
    }

    Ok(ws.on_upgrade(move |socket| {
        handle_transcribe_socket(socket, params, state)
    }))
}
```

### Pattern 2: WebSocket Audio Buffer and Windowing

**What:** Buffer incoming audio frames and extract fixed-size windows with overlap
**When to use:** Streaming ASR where inference runs on fixed-size audio chunks

```rust
// Source: training knowledge [ASSUMED]
pub struct AudioBuffer {
    samples: Vec<f32>,
    window_samples: usize,   // e.g., 30s * 16000 = 480_000
    overlap_samples: usize,  // e.g., 1s * 16000 = 16_000
    chunk_index: u64,
}

impl AudioBuffer {
    pub fn new(window_secs: f32, overlap_secs: f32, sample_rate: u32) -> Self {
        Self {
            samples: Vec::new(),
            window_samples: (window_secs * sample_rate as f32) as usize,
            overlap_samples: (overlap_secs * sample_rate as f32) as usize,
            chunk_index: 0,
        }
    }

    /// Append samples. Returns windows ready for inference.
    pub fn push(&mut self, new_samples: &[f32]) -> Vec<(Vec<f32>, u64)> {
        self.samples.extend_from_slice(new_samples);
        let mut windows = Vec::new();

        while self.samples.len() >= self.window_samples {
            let window: Vec<f32> = self.samples[..self.window_samples].to_vec();
            let idx = self.chunk_index;
            self.chunk_index += 1;

            // Advance by (window - overlap) to keep overlap for next window
            let advance = self.window_samples - self.overlap_samples;
            self.samples.drain(..advance);

            windows.push((window, idx));
        }
        windows
    }

    /// Flush remaining samples as a final (possibly short) window.
    pub fn flush(&mut self) -> Option<(Vec<f32>, u64)> {
        if self.samples.is_empty() {
            return None;
        }
        let window = std::mem::take(&mut self.samples);
        let idx = self.chunk_index;
        self.chunk_index += 1;
        Some((window, idx))
    }
}
```

### Pattern 3: CTC Greedy Decoding

**What:** Decode CTC model output logits into text
**When to use:** wav2vec2, HuBERT, and other CTC-based ASR models

```rust
// Source: training knowledge [ASSUMED] - standard CTC algorithm
/// Greedy CTC decode: argmax per timestep, collapse repeats, remove blanks.
pub fn ctc_greedy_decode(
    logits: &[f32],
    num_timesteps: usize,
    vocab_size: usize,
    vocab: &[String],
    blank_id: usize,
) -> String {
    let mut prev_token = blank_id;
    let mut decoded = Vec::new();

    for t in 0..num_timesteps {
        let start = t * vocab_size;
        let end = start + vocab_size;
        let timestep_logits = &logits[start..end];

        // Argmax
        let (best_id, _) = timestep_logits
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
            .unwrap();

        // Skip blanks and repeated tokens
        if best_id != blank_id && best_id != prev_token {
            if let Some(token) = vocab.get(best_id) {
                decoded.push(token.as_str());
            }
        }
        prev_token = best_id;
    }

    decoded.join("")
}
```

### Pattern 4: i16-to-f32 PCM Conversion

**What:** Convert 16-bit integer PCM to 32-bit float PCM
**When to use:** Client sends i16-encoded audio (per D-06)

```rust
// Source: standard PCM conversion [ASSUMED]
/// Convert i16 PCM bytes (little-endian) to f32 samples in [-1.0, 1.0].
pub fn i16_bytes_to_f32(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(2)
        .map(|chunk| {
            let sample = i16::from_le_bytes([chunk[0], chunk[1]]);
            sample as f32 / 32768.0
        })
        .collect()
}

/// Convert f32 PCM bytes (little-endian) to f32 samples.
pub fn f32_bytes_to_samples(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
}
```

### Anti-Patterns to Avoid

- **Holding RwLock across WebSocket recv/send:** The WebSocket handler must NOT hold the pipeline RwLock while waiting for the next audio frame. Acquire read lock for prepare, drop it, acquire write lock for execute, drop it, then send the result. This is the same pattern used by the REST and gRPC handlers.
- **Unbounded audio buffer:** Without a size limit, a slow consumer or fast sender can exhaust memory. Cap the audio buffer at a reasonable maximum (e.g., 2x window size). Drop connection if exceeded.
- **Blocking the async runtime with FFT:** Mel spectrogram computation is CPU-intensive. For large windows (30s at 16kHz = 480K samples), consider `tokio::task::spawn_blocking` to avoid blocking the async runtime. However, for typical ASR windows the computation is fast enough (<5ms) that inline execution is acceptable.
- **Assuming little-endian without documentation:** PCM byte order must be documented in the WebSocket protocol. Hephaestus assumes little-endian (matching x86/ARM native order), but the protocol should make this explicit to clients.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Mel spectrogram computation | Custom FFT + mel filterbank | `mel_spec` crate | Whisper requires exact numerical parity with the training preprocessing. mel_spec is validated against whisper.cpp, PyTorch, and librosa reference outputs. Subtle FFT windowing or mel bin calculation errors cause silent quality degradation. |
| WebSocket protocol handling | Raw TCP + HTTP upgrade | `axum::extract::ws` (built-in) | WebSocket framing, masking, ping/pong, close handshake, and upgrade negotiation are complex and security-sensitive. axum's implementation is battle-tested. |
| FFT computation | Custom DFT implementation | `rustfft` (via mel_spec) | FFT is a well-understood algorithm but correct implementation requires careful numerical handling. rustfft is optimized with SIMD and has 11 years of production use. |

**Key insight:** ASR preprocessing requires exact numerical fidelity with the model's training pipeline. Any deviation -- even small rounding differences in mel bin boundaries or window functions -- causes silent quality degradation that is extremely hard to debug. Use validated implementations.

## Common Pitfalls

### Pitfall 1: Whisper Requires Encoder-Decoder Loop, Not Single Forward Pass
**What goes wrong:** Treating Whisper like a single-pass model (as existing pipelines do). Whisper's ONNX export produces separate encoder and decoder models. The decoder must be called in an autoregressive loop -- feed encoder output + previously generated tokens, get next token, repeat until EOS.
**Why it happens:** The existing `Pipeline` trait assumes single `session.run()` per inference. Whisper needs a loop.
**How to avoid:** The `AsrPipeline::execute()` for Whisper must: (1) run encoder once on mel features, (2) loop decoder with accumulated tokens until EOS or max length. This is fundamentally different from the single-pass pattern. The `execute()` method hides this complexity behind the trait interface (Ousterhout deep module principle).
**Warning signs:** Getting random or empty text output from Whisper, or only the first token.

### Pitfall 2: Mel Spectrogram Numerical Precision
**What goes wrong:** Whisper produces garbage output or high WER because mel features don't match training preprocessing.
**Why it happens:** Subtle differences in FFT windowing (periodic vs symmetric Hann), mel filterbank construction (HTK vs Slaney), or log scaling can shift mel features enough to degrade output.
**How to avoid:** Use `mel_spec` which is validated against whisper.cpp reference output. Write a test that computes mel features for a known audio sample and compares against reference values.
**Warning signs:** Unusually high word error rate, repetitive output, or hallucinated text.

### Pitfall 3: WebSocket Body Size Limit
**What goes wrong:** The existing `RequestBodyLimitLayer::new(1MB)` in `routes.rs` may interfere with WebSocket upgrade or limit binary message size.
**Why it happens:** axum applies middleware layers to all routes including WebSocket. WebSocket messages carrying 30s of f32 audio at 16kHz = 480K * 4 bytes = ~1.92MB, which exceeds the 1MB limit.
**How to avoid:** Either (1) exempt the WebSocket route from the body limit layer, or (2) ensure clients send audio in smaller frames (e.g., 100ms chunks = 6.4KB per frame). Option 2 is preferred because it aligns with the streaming use case and keeps memory usage low.
**Warning signs:** WebSocket connection drops after sending large frames, or 413 Payload Too Large errors.

### Pitfall 4: RwLock Starvation Under WebSocket Load
**What goes wrong:** Many concurrent WebSocket connections requesting write locks for `execute()` starve REST/gRPC handlers.
**Why it happens:** D-03 says each WebSocket connection runs inference independently. Many connections = many write lock acquisitions, blocking text inference requests.
**How to avoid:** The existing architecture limits this -- write lock is held only for the duration of `session.run()`, which is typically <100ms for ASR. But document that heavy WebSocket load impacts REST/gRPC latency. Future optimization (deferred): per-connection pipeline cloning or connection-level queuing.
**Warning signs:** Rising p99 latency on REST `/infer` endpoint when WebSocket connections are active.

### Pitfall 5: Whisper ONNX Model Variants
**What goes wrong:** Code assumes one ONNX model structure but Whisper models exported by different tools (Optimum, sherpa-onnx, custom) have different input/output names.
**Why it happens:** Optimum exports with input name `input_features`, sherpa-onnx uses `mel`. Decoder inputs also differ (`decoder_input_ids` vs `tokens`).
**How to avoid:** Support the Optimum export format (most common on HuggingFace): `encoder_model.onnx` with input `input_features` shape `[batch, 80, 3000]`, and `decoder_model_merged.onnx` with input `decoder_input_ids`. Document supported export format. Consider detecting format by inspecting ONNX input names at load time.
**Warning signs:** Model loads but inference returns "input name not found" errors.

### Pitfall 6: Overlap Deduplication Complexity
**What goes wrong:** Adjacent windows with overlap produce duplicate text at chunk boundaries.
**Why it happens:** The overlap region is processed by both the current and next window, producing redundant transcript segments.
**How to avoid:** For Whisper (encoder-decoder): timestamp tokens in the decoder output indicate time positions -- discard tokens from the overlap region of the previous window. For CTC models: simpler approach -- discard the last N frames of CTC output from each window (where N = overlap_frames / compression_ratio). Start with the simpler "discard overlap edges" strategy and refine later.
**Warning signs:** Repeated words or phrases at chunk boundaries in transcripts.

## Code Examples

### Whisper ONNX Model Structure (Optimum Export)

```
Model directory:
  config.json                    # architectures: ["WhisperForConditionalGeneration"]
  preprocessor_config.json       # feature_size: 80, n_fft: 400, hop_length: 160, sampling_rate: 16000
  tokenizer.json                 # Whisper tokenizer (50K+ vocab including special tokens)
  onnx/
    encoder_model.onnx           # Input: input_features [batch, 80, 3000], Output: last_hidden_state
    decoder_model_merged.onnx    # Input: decoder_input_ids + encoder outputs, Output: logits
```

**Encoder ONNX inputs/outputs (Optimum format):** [CITED: huggingface.co/docs/optimum-onnx]
- Input: `input_features` -- shape `[batch, 80, 3000]` (80 mel bins, 3000 time steps for 30s audio), dtype float32
- Output: `last_hidden_state` -- shape `[batch, 1500, d_model]` (1500 = 3000/2 due to conv stride)

**Encoder ONNX inputs/outputs (sherpa-onnx format):** [CITED: github.com/k2-fsa/sherpa-onnx]
- Input: `mel` -- shape `[batch, 80, 3000]`
- Output: `n_layer_cross_k`, `n_layer_cross_v` -- cross-attention KV cache

**Decoder ONNX inputs/outputs (Optimum merged format):** [CITED: huggingface.co/docs/optimum-onnx]
- Input: `decoder_input_ids` -- shape `[batch, seq]` dtype int64
- Input: `encoder_hidden_states` -- from encoder output
- Input: past_key_values (KV cache, managed internally in merged model)
- Output: `logits` -- shape `[batch, seq, vocab_size]`

**Whisper preprocessor_config.json key fields:** [CITED: huggingface.co/openai/whisper-large-v3]
```json
{
  "feature_extractor_type": "WhisperFeatureExtractor",
  "feature_size": 128,       // 128 for v3, 80 for v1/v2/base/tiny
  "sampling_rate": 16000,
  "n_fft": 400,              // FFT window size (25ms at 16kHz)
  "hop_length": 160,         // hop between frames (10ms at 16kHz)
  "chunk_length": 30,        // seconds per chunk
  "n_samples": 480000,       // 30s * 16000 samples
  "nb_max_frames": 3000,     // max spectrogram frames
  "return_attention_mask": false
}
```

**Whisper config.json key fields:** [CITED: huggingface.co/openai/whisper-base]
```json
{
  "architectures": ["WhisperForConditionalGeneration"],
  "model_type": "whisper",
  "is_encoder_decoder": true,
  "num_mel_bins": 80,        // 80 for base/tiny, 128 for v3
  "max_source_positions": 1500,
  "max_target_positions": 448,
  "d_model": 512,
  "decoder_start_token_id": 50258,
  "eos_token_id": 50257,
  "vocab_size": 51865
}
```

### wav2vec2 ONNX Model Structure

```
Model directory:
  config.json                    # architectures: ["Wav2Vec2ForCTC"]
  vocab.json                     # CTC vocabulary mapping
  onnx/
    model.onnx                   # Single model file
```

**ONNX inputs/outputs:** [CITED: huggingface.co/docs/transformers/en/model_doc/wav2vec2]
- Input: `input_values` -- shape `[batch, samples]`, dtype float32 (raw 16kHz waveform, normalized)
- Output: `logits` -- shape `[batch, timesteps, vocab_size]`, dtype float32 (CTC logits)

**CTC decoding:** [ASSUMED]
- Output logits are per-timestep probability distributions over the vocabulary
- Greedy decode: argmax per timestep, collapse consecutive repeats, remove blank tokens
- Blank token ID is typically 0 (the first entry in vocab.json, mapped to `"<pad>"` or `"|"`)
- vocab.json maps character IDs to characters (e.g., `{"a": 1, "b": 2, ..., "|": 0}`)

### ASR Profile Detection

```rust
// Source: existing profile.rs pattern + training knowledge [ASSUMED]
// Add to detect_profile() in profile.rs:

// In the architectures matching block:
if name.ends_with("ForCTC") {
    return Ok(ModelProfile::Asr);
}
if name == "WhisperForConditionalGeneration" {
    return Ok(ModelProfile::Asr);
}

// In the pipeline_tag fallback:
"automatic-speech-recognition" => return Ok(ModelProfile::Asr),
```

### Mel Spectrogram with mel_spec

```rust
// Source: training knowledge + crate description [ASSUMED]
// Whisper-compatible mel spectrogram computation
use mel_spec::mel::MelSpectrogram;

fn compute_mel_features(
    audio_samples: &[f32],
    n_fft: usize,      // 400 for Whisper
    hop_length: usize,  // 160 for Whisper
    n_mels: usize,      // 80 for Whisper base/tiny, 128 for v3
    sample_rate: u32,    // 16000
) -> Vec<f32> {
    // mel_spec provides Whisper-compatible mel spectrogram computation
    // The exact API should be verified against mel_spec docs at implementation time
    // Key: output must be log-mel spectrogram matching Whisper's preprocessing
    todo!("verify mel_spec API at implementation time")
}
```

### WebSocket Route Registration

```rust
// Source: axum docs + existing routes.rs pattern [ASSUMED]
// In routes.rs:
use crate::ws;

pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/infer", post(handlers::infer))
        .route("/ws/transcribe", get(ws::ws_transcribe))  // NEW
        .route("/healthz/live", get(handlers::liveness))
        .route("/healthz/ready", get(handlers::readiness))
        .route("/metrics", get(metrics::metrics_handler))
        .layer(RequestBodyLimitLayer::new(MAX_BODY_SIZE))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Whisper single ONNX file | Separate encoder + decoder ONNX files | Optimum ~2023 | Must load and orchestrate two ONNX sessions for Whisper |
| Whisper 80 mel bins | 128 mel bins (v3) | Whisper v3 (2023) | preprocessor_config.json `feature_size` field determines mel bins; not always 80 |
| Fixed mel bin count | Configurable via preprocessor_config.json | HuggingFace convention | Read `feature_size` from preprocessor_config.json rather than hardcoding |
| Manual ONNX export scripts | Optimum CLI export | Optimum ~2023 | Standard model format on HuggingFace; input/output names follow Optimum conventions |

**Deprecated/outdated:**
- Single-file Whisper ONNX exports are still functional but the Optimum split encoder/decoder format is the standard on HuggingFace
- whisper.cpp's custom ONNX format (with input name `mel` and output `n_layer_cross_k/v`) is sherpa-onnx specific, not the HuggingFace standard

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | mel_spec crate API provides direct Whisper-compatible mel spectrogram computation | Standard Stack, Code Examples | Would need to hand-roll mel computation using rustfft directly; higher effort but feasible |
| A2 | CTC greedy decoding (argmax + blank collapse + repeat removal) is sufficient for wav2vec2 output quality | Code Examples | Beam search CTC decoder would improve accuracy but adds complexity; greedy is standard for real-time |
| A3 | Optimum-exported Whisper models use `input_features` as encoder input name and `decoder_input_ids` as decoder input | Code Examples | If different, must adapt input tensor construction; can detect at model load time |
| A4 | wav2vec2 ONNX models use `input_values` as input name for raw waveform | Code Examples | If different, model load validation will catch it |
| A5 | futures-util 0.3.34 is the correct version for StreamExt::split() with axum WebSocket | Standard Stack | futures-util is stable; version mismatch unlikely to cause issues |
| A6 | mel_spec crate handles both 80-mel and 128-mel configurations (Whisper v1/v2 vs v3) | Code Examples | If not configurable, would need to read preprocessor_config.json and pass n_mels to mel_spec |
| A7 | Discard-overlap-edges is sufficient initial deduplication strategy | Common Pitfalls | May produce minor duplication at boundaries; refinement is a future enhancement |
| A8 | Little-endian byte order assumption for PCM over WebSocket is correct for target platforms | Code Examples | Network protocols sometimes use big-endian; documenting LE in the protocol spec prevents issues |

## Open Questions

1. **mel_spec API surface**
   - What we know: mel_spec is Whisper-compatible, uses rustfft internally, 33K weekly downloads
   - What's unclear: Exact API for configuring n_mels, n_fft, hop_length -- need to read mel_spec docs at implementation time
   - Recommendation: Verify API during implementation; if insufficient, fall back to rustfft + hand-rolled mel filterbank

2. **Whisper decoder loop complexity**
   - What we know: Whisper requires autoregressive decoding (encoder once, decoder in a loop). The merged decoder model handles KV caching internally.
   - What's unclear: Whether ort (the Rust ONNX Runtime bindings) supports running the merged decoder model with KV cache correctly, or if we need to manage past_key_values tensors explicitly
   - Recommendation: Test with a small Whisper model (whisper-tiny.en) during implementation to validate the decoder loop works with ort

3. **WebSocket route body limit interaction**
   - What we know: The existing `RequestBodyLimitLayer::new(1MB)` applies to all routes
   - What's unclear: Whether axum's body limit layer affects WebSocket messages after the initial upgrade handshake
   - Recommendation: Test empirically; if it does, move the body limit layer to a route-specific scope (only on `/infer`) or increase it

4. **Overlap deduplication precision for Whisper timestamps**
   - What we know: Whisper decoder output includes timestamp tokens that indicate time positions
   - What's unclear: Whether ort exposes these tokens in a way that allows time-based overlap trimming
   - Recommendation: Start with simple character-overlap trimming; refine with timestamp-based trimming if Whisper token IDs are accessible

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| Rust toolchain | All code | Yes | 1.97.1 | -- |
| Cargo | Build | Yes | 1.97.1 | -- |
| ONNX Runtime | Inference | Yes (via ort download-binaries) | 1.28 | -- |
| mel_spec crate | Mel spectrograms | Yes (crates.io) | 0.4.0 | Hand-roll with rustfft |
| Whisper ONNX model | ASR testing | No (must download) | -- | Download via hf-hub in tests |
| wav2vec2 ONNX model | ASR testing | No (must download) | -- | Download via hf-hub in tests |

**Missing dependencies with no fallback:** None -- all runtime dependencies are available or downloadable.

**Missing dependencies with fallback:**
- ASR test models must be downloaded during test setup (same pattern as existing classifier_e2e tests using hf-hub)

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in test + tokio::test |
| Config file | Cargo.toml (workspace) |
| Quick run command | `cargo test --workspace -q` |
| Full suite command | `cargo test --workspace` |

### Phase Requirements to Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| PRFX-01a | ASR pipeline loads Whisper ONNX model | integration | `cargo test -p hephaestus-core --features integration asr` | No -- Wave 0 |
| PRFX-01b | ASR pipeline produces transcript from audio | integration | `cargo test -p hephaestus-core --features integration asr` | No -- Wave 0 |
| PRFX-01c | CTC greedy decoder produces correct output | unit | `cargo test -p hephaestus-core ctc` | No -- Wave 0 |
| PRFX-01d | Mel spectrogram matches reference values | unit | `cargo test -p hephaestus-core mel` | No -- Wave 0 |
| PRFX-01e | ModelProfile::Asr detected from config.json | unit | `cargo test -p hephaestus-core profile` | No -- Wave 0 |
| APIX-02a | WebSocket upgrade succeeds with valid params | integration | `cargo test -p hephaestus-api ws` | No -- Wave 0 |
| APIX-02b | WebSocket rejects invalid sample_rate | unit | `cargo test -p hephaestus-api ws` | No -- Wave 0 |
| APIX-02c | i16-to-f32 PCM conversion correct | unit | `cargo test -p hephaestus-api pcm` | No -- Wave 0 |
| APIX-02d | Audio buffer windowing produces correct chunks | unit | `cargo test -p hephaestus-api buffer` | No -- Wave 0 |
| SC-11 | Existing REST/gRPC endpoints unchanged | integration | `cargo test -p hephaestus-api api` | Yes (existing) |

### Sampling Rate
- **Per task commit:** `cargo test --workspace -q`
- **Per wave merge:** `cargo test --workspace`
- **Phase gate:** Full suite green before `/gsd-verify-work`

### Wave 0 Gaps
- [ ] `crates/hephaestus-core/tests/asr_e2e.rs` -- ASR pipeline integration tests (model download + inference)
- [ ] `crates/hephaestus-core/src/ctc.rs` + unit tests -- CTC decoder module
- [ ] `crates/hephaestus-core/src/mel.rs` + unit tests -- Mel spectrogram wrapper
- [ ] `crates/hephaestus-api/tests/ws.rs` -- WebSocket handler tests
- [ ] `crates/hephaestus-api/src/ws.rs` + unit tests -- Audio buffer, PCM conversion

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | No | WebSocket endpoint is unauthenticated (same as existing REST/gRPC) |
| V3 Session Management | No | No HTTP sessions; WebSocket connections are stateless from auth perspective |
| V4 Access Control | No | No authorization model; all endpoints are open |
| V5 Input Validation | Yes | Validate query params at connect, validate PCM frame sizes, cap buffer size |
| V6 Cryptography | No | No crypto operations in this phase |

### Known Threat Patterns for WebSocket + Audio

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Oversized binary messages exhaust memory | Denial of Service | Cap individual WebSocket message size; cap total audio buffer per connection |
| Malformed PCM data crashes conversion | Tampering | Validate byte length is multiple of sample size (2 for i16, 4 for f32) |
| Connection flooding (many idle WebSocket connections) | Denial of Service | Idle timeout on WebSocket connections (e.g., close if no data received for 30s) |
| Path traversal via channel parameter | Information Disclosure | Channel label is a display string only, never used in file paths |
| Slowloris-style WebSocket attack | Denial of Service | Existing connection limits in axum; consider per-IP connection limits |

## Sources

### Primary (HIGH confidence)
- Codebase source files: pipeline.rs, profile.rs, routes.rs, handlers.rs, main.rs, state.rs, config.rs, error.rs -- direct reading of current implementation
- [axum WebSocket docs](https://docs.rs/axum/latest/axum/extract/ws/index.html) -- WebSocketUpgrade API, Message types, split()
- [axum WebSocket example](https://github.com/tokio-rs/axum/blob/main/examples/websockets/src/main.rs) -- Handler patterns, lifecycle management

### Secondary (MEDIUM confidence)
- [Whisper config.json](https://huggingface.co/openai/whisper-base/blob/main/config.json) -- Model config fields
- [Whisper preprocessor_config.json](https://huggingface.co/openai/whisper-large-v3/blob/main/preprocessor_config.json) -- Mel spectrogram parameters
- [Whisper ONNX export (sherpa-onnx)](https://github.com/k2-fsa/sherpa-onnx/blob/master/scripts/whisper/export-onnx.py) -- Encoder/decoder input/output names
- [Optimum ONNX export docs](https://huggingface.co/docs/optimum-onnx/en/onnx/usage_guides/export_a_model) -- Optimum export conventions
- [Whisper ONNX blog (cprohm.de)](https://cprohm.de/blog/whisper/) -- Practical ONNX inference details
- [wav2vec2 HuggingFace docs](https://huggingface.co/docs/transformers/en/model_doc/wav2vec2) -- Model architecture, CTC output
- [mel_spec GitHub](https://github.com/wavey-ai/mel-spec) -- Whisper-compatible mel spectrogram crate
- [CTC decoding distill.pub](https://distill.pub/2017/ctc/) -- CTC algorithm reference
- crates.io registry (rustfft 6.4.1, realfft 3.5.0, mel_spec 0.4.0, futures-util 0.3.34)

### Tertiary (LOW confidence)
- Training knowledge for CTC greedy decoding implementation details
- Training knowledge for audio buffer windowing pattern
- Training knowledge for WebSocket protocol best practices
- Training knowledge for mel_spec crate API surface

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- axum WebSocket is well-documented, mel_spec is validated against references, crate versions verified
- Architecture: HIGH -- extends existing patterns (PipelineKind, AppState, route registration) with clear integration points
- ASR model structure: MEDIUM -- Whisper ONNX format verified from multiple sources but decoder loop with ort needs runtime validation
- Pitfalls: HIGH -- grounded in codebase analysis (RwLock patterns, body limit layer) and documented ASR preprocessing challenges

**Research date:** 2026-08-29
**Valid until:** 2026-09-28 (30 days -- stable domain, mature libraries)

## Project Constraints (from CLAUDE.md)

- **Language:** Rust only, 2024 edition, workspace resolver 3
- **Rules compliance:** All files must adhere to `rules/` directory rules
- **No AI attribution:** No Co-Authored-By lines, AI-generated mentions, or Claude/AI references in any repo artifact
- **Deep module principle:** Traits expose minimal interface (1-3 methods) hiding significant complexity. The ASR pipeline `prepare()` and `execute()` methods must hide mel computation, encoder/decoder orchestration, and CTC decoding from callers.
