---
phase: 11-websocket-streaming-asr-pipeline
reviewed: 2026-08-29T13:45:00Z
depth: standard
files_reviewed: 16
files_reviewed_list:
  - crates/hephaestus-core/src/pipeline.rs
  - crates/hephaestus-core/src/error.rs
  - crates/hephaestus-core/src/lib.rs
  - crates/hephaestus-core/src/ctc.rs
  - crates/hephaestus-core/src/mel.rs
  - crates/hephaestus-core/src/profile.rs
  - crates/hephaestus-core/Cargo.toml
  - crates/hephaestus-api/src/error.rs
  - crates/hephaestus-api/src/handlers.rs
  - crates/hephaestus-api/src/grpc/inference.rs
  - crates/hephaestus-api/src/batcher.rs
  - crates/hephaestus-api/src/ws.rs
  - crates/hephaestus-api/src/state.rs
  - crates/hephaestus-api/src/routes.rs
  - crates/hephaestus/src/config.rs
  - crates/hephaestus/src/main.rs
findings:
  critical: 3
  warning: 7
  info: 0
  total: 10
status: issues_found
---

# Phase 11: Code Review Report

**Reviewed:** 2026-08-29T13:45:00Z
**Depth:** standard
**Files Reviewed:** 16
**Status:** issues_found

## Summary

Phase 11 adds WebSocket streaming and ASR pipeline support across three plans: pipeline type generalization (InferenceInput/PreparedData/PipelineOutput), WebSocket transport infrastructure, and the CTC/Whisper ASR pipeline with mel spectrogram preprocessing.

The overall architecture is well-structured -- the InferenceInput/PipelineOutput type system is a clean generalization, AudioBuffer windowing logic is correct and well-tested, and CTC greedy decode is a faithful standard implementation. Profile detection correctly prioritizes Whisper before generic ForConditionalGeneration to avoid Seq2Seq misdetection.

However, the review identified 3 critical and 7 warning-level issues. The most serious are: the Whisper decoder's `encoder_hidden_states` input name is hardcoded without load-time validation (will crash on first inference if the model uses a different name), the WebSocket handler leaks internal error details to clients, and token ID casting in the Whisper decode path is unchecked -- contradicting the project's own `u32::try_from()` pattern used in the Seq2Seq pipeline. The release profile sets `panic = "abort"`, making any unchecked indexing or arithmetic underflow an instant process crash with no recovery.

## Narrative Findings (AI reviewer)

## Critical Issues

### CR-01: Whisper decoder `encoder_hidden_states` input name hardcoded without load-time validation

**File:** `crates/hephaestus-core/src/pipeline.rs:1261-1262`
**Issue:** The Whisper decoder is invoked with a hardcoded input name `"encoder_hidden_states"`:
```rust
ort::inputs![
    self.decoder_input_name.as_str() => token_tensor,
    "encoder_hidden_states" => enc_tensor,
]
```
The T-11-07 validation at load time (lines 1051-1065) checks for `input_ids`/`decoder_input_ids` but does NOT validate that the decoder session has an input named `encoder_hidden_states`. Some Whisper ONNX exports use different names (e.g., `encoder_output`, `encoder_last_hidden_state`). If the model uses a different name, it loads successfully but the first inference call fails with an ONNX Runtime error. Since `panic = "abort"` is set in the release profile, any uncaught panic from an unexpected ONNX error path terminates the process.

**Fix:** Add `encoder_hidden_states` to the decoder input validation in `new_whisper`, and store the detected name (similar to `decoder_input_name`):
```rust
// In new_whisper, after decoder_input_name detection:
let encoder_hidden_name = if decoder_inputs.iter().any(|n| n == "encoder_hidden_states") {
    "encoder_hidden_states".to_string()
} else if decoder_inputs.iter().any(|n| n == "encoder_output") {
    "encoder_output".to_string()
} else {
    return Err(CoreError::ModelValidation(format!(
        "Whisper decoder must have 'encoder_hidden_states' or 'encoder_output' input; got: {decoder_inputs:?}"
    )));
};

// Add field to AsrPipeline struct, use in execute_whisper
```

### CR-02: WebSocket handler leaks internal error details to clients

**File:** `crates/hephaestus-api/src/ws.rs:298-308`
**Issue:** When ASR inference fails, the full error message is sent to the WebSocket client:
```rust
let error_msg = serde_json::json!({
    "error": format!("inference failed: {e}"),
    "chunk_index": chunk_index,
});
```
`CoreError` variants can contain file system paths (`ModelLoad`), ONNX Runtime internal messages (`Inference`), and other system details. The HTTP handler (handlers.rs:107-112) and gRPC handler (grpc/inference.rs:72-83) both sanitize server errors to `"internal server error"` before sending to clients. The WebSocket handler does not follow this pattern, creating an information disclosure gap.

**Fix:** Sanitize the error message before sending to the client:
```rust
let client_message = "inference failed";
tracing::error!(
    model_id = %state.model_id(),
    error = %e,
    chunk_index,
    "ASR inference failed"
);
let error_msg = serde_json::json!({
    "error": client_message,
    "chunk_index": chunk_index,
});
```

### CR-03: Unchecked `as u32` truncation in Whisper token decode

**File:** `crates/hephaestus-core/src/pipeline.rs:1298-1302`
**Issue:** Whisper decoder output tokens are cast from `i64` to `u32` without bounds checking:
```rust
let output_ids: Vec<u32> = tokens
    .iter()
    .skip(1)
    .map(|&t| t as u32)
    .collect();
```
This contrasts with the Seq2Seq pipeline (lines 621-628) which correctly uses `u32::try_from(id).map_err(...)`. The `as u32` cast silently truncates values outside the `u32` range. While current Whisper vocab sizes are small (~52K), this violates the project's own safety pattern established in WR-02 of a prior review and is inconsistent within the same file. With `panic = "abort"` in release, any downstream failure from a corrupted token ID is unrecoverable.

**Fix:** Use the same checked conversion as Seq2Seq:
```rust
let output_ids: Vec<u32> = tokens
    .iter()
    .skip(1)
    .map(|&t| {
        u32::try_from(t).map_err(|_| {
            CoreError::Inference(format!("invalid token ID {t} in Whisper decoder output"))
        })
    })
    .collect::<Result<Vec<u32>, CoreError>>()?;
```

## Warnings

### WR-01: Missing readiness gate in WebSocket handler

**File:** `crates/hephaestus-api/src/ws.rs:203-219`
**Issue:** The `ws_transcribe` handler does not check `state.is_ready()` before accepting WebSocket connections. Both the HTTP handler (handlers.rs:49) and gRPC handler (grpc/inference.rs:47) gate on readiness. During graceful shutdown, readiness is set to false to drain traffic, but new WebSocket connections can still be established. The 30-second idle timeout partially mitigates this, but an active sender would hold the connection open indefinitely.

**Fix:** Add a readiness check before upgrading:
```rust
pub async fn ws_transcribe(
    ws: WebSocketUpgrade,
    Query(params): Query<TranscribeParams>,
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, ApiError> {
    if !state.is_ready() {
        return Err(ApiError::NotReady);
    }
    // ... rest of validation
}
```

### WR-02: AudioBuffer infinite loop when window_samples truncates to zero

**File:** `crates/hephaestus-api/src/ws.rs:136-137, 169`
**Issue:** `AudioBuffer::new` computes `window_samples = (window_secs * sample_rate as f32) as usize`. If `window_secs` is very small but positive (e.g., 0.00001), the truncation to `usize` produces 0. The `push()` method's loop `while self.samples.len() >= self.window_samples` becomes `while len >= 0`, which is always true for `usize`, causing an infinite loop that hangs the WebSocket handler thread forever.

Config validation (config.rs:299) accepts `window_size_secs > 0.0`, so this edge case passes validation. The practical default is 30.0s, but the code path exists.

**Fix:** Add a guard in `AudioBuffer::new`:
```rust
pub fn new(window_secs: f32, overlap_secs: f32, sample_rate: u32) -> Self {
    let window_samples = (window_secs * sample_rate as f32) as usize;
    let overlap_samples = (overlap_secs * sample_rate as f32) as usize;
    assert!(window_samples > 0, "window_samples must be > 0");
    assert!(window_samples > overlap_samples, "window must exceed overlap");
    // ...
}
```
Or better, validate in config.rs that `window_size_secs * 16000.0 >= 1.0` (at minimum sample rate).

### WR-03: Whisper decoder output shape not validated before indexing

**File:** `crates/hephaestus-core/src/pipeline.rs:1272`
**Issue:** `logit_shape[logit_shape.len() - 1]` indexes the last dimension without checking that `logit_shape` has at least one dimension. If the decoder produces a 0-D tensor, `logit_shape.len() - 1` underflows on `usize`, panicking in debug mode and wrapping to `usize::MAX` in release (then panicking on index). With `panic = "abort"`, this is an instant process crash.

Similarly, line 1275-1276 slices into `logit_data` without verifying the data length is sufficient:
```rust
let last_pos_start = (seq_len - 1) * vocab_size;
let last_pos_logits = &logit_data[last_pos_start..last_pos_start + vocab_size];
```

**Fix:** Add shape validation matching the CTC path (pipeline.rs:1186-1191):
```rust
if logit_shape.len() != 3 {
    return Err(CoreError::Inference(format!(
        "expected 3D decoder output (batch, seq, vocab), got {}-D",
        logit_shape.len()
    )));
}
let vocab_size = logit_shape[2] as usize;
```

### WR-04: `mel_spec` dependency not workspace-managed

**File:** `crates/hephaestus-core/Cargo.toml:14`
**Issue:** `mel_spec = "0.4"` is declared directly in the crate's Cargo.toml instead of being inherited from the workspace. All other dependencies use `dep.workspace = true`. This violates `rules/proj-workspace-deps.md` and risks version drift if mel_spec is added to other crates later.

**Fix:** Add to workspace Cargo.toml:
```toml
# In root Cargo.toml [workspace.dependencies]
mel_spec = "0.4"
```
Then in crate Cargo.toml:
```toml
mel_spec.workspace = true
```

### WR-05: CTC vocab.json large max_id can exhaust memory

**File:** `crates/hephaestus-core/src/pipeline.rs:958-962`
**Issue:** The CTC pipeline reads `vocab.json` and allocates a vector sized by the maximum token ID:
```rust
let max_id = vocab_map.values().copied().max().unwrap_or(0);
let mut vocab = vec![String::new(); max_id + 1];
```
A maliciously crafted `vocab.json` with `{"<pad>": 999999999}` would allocate a ~1 billion element vector of Strings, exhausting memory. While model files are generally trusted, this is a defense-in-depth gap.

**Fix:** Add a sanity check:
```rust
let max_id = vocab_map.values().copied().max().unwrap_or(0);
if max_id > 1_000_000 {
    return Err(CoreError::ModelLoad(format!(
        "vocab.json max_id {max_id} exceeds limit of 1,000,000"
    )));
}
```

### WR-06: No NaN/Inf validation on f32 PCM audio samples

**File:** `crates/hephaestus-api/src/ws.rs:96-101`
**Issue:** `f32_bytes_to_samples` interprets raw bytes as `f32` values without validating that the results are finite. Arbitrary byte patterns can produce NaN or Infinity values, which propagate through mel spectrogram computation and ONNX inference. NaN values in model input can cause silent quality degradation or unexpected ONNX Runtime behavior.

**Fix:** Filter or reject non-finite values:
```rust
pub fn f32_bytes_to_samples(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|chunk| {
            let sample = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            if sample.is_finite() { sample } else { 0.0 }
        })
        .collect()
}
```

### WR-07: WebSocket `channel` query parameter has no length limit

**File:** `crates/hephaestus-api/src/ws.rs:31`
**Issue:** The `channel` field in `TranscribeParams` is an unbounded `String` from the query URL. While it is only used as a display label in transcript JSON messages (never in file paths), an extremely long value (e.g., 10MB in the URL query string) would be cloned into every `TranscriptMessage` for the lifetime of the connection, wasting memory and inflating response payloads. The `sample_rate` and `encoding` fields are validated, but `channel` is not.

**Fix:** Add a length check alongside the other validations:
```rust
if params.channel.len() > 256 {
    return Err(ApiError::BadRequest(
        "channel label must be 256 characters or fewer".into(),
    ));
}
```

---

_Reviewed: 2026-08-29T13:45:00Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
