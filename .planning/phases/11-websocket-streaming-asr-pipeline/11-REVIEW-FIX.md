---
phase: 11-websocket-streaming-asr-pipeline
fixed_at: 2026-08-29T14:05:00Z
review_path: .planning/phases/11-websocket-streaming-asr-pipeline/11-REVIEW.md
iteration: 1
findings_in_scope: 10
fixed: 10
skipped: 0
status: all_fixed
---

# Phase 11: Code Review Fix Report

**Fixed at:** 2026-08-29T14:05:00Z
**Source review:** .planning/phases/11-websocket-streaming-asr-pipeline/11-REVIEW.md
**Iteration:** 1

**Summary:**
- Findings in scope: 10
- Fixed: 10
- Skipped: 0

## Fixed Issues

### CR-01: Whisper decoder `encoder_hidden_states` input name hardcoded without load-time validation

**Files modified:** `crates/hephaestus-core/src/pipeline.rs`
**Commit:** 6a95391
**Applied fix:** Added `encoder_hidden_name` field to `AsrPipeline` struct. Added load-time validation in `new_whisper` that detects the encoder hidden states input name from the decoder session inputs, accepting `encoder_hidden_states`, `encoder_output`, or `encoder_last_hidden_state`. The hardcoded `"encoder_hidden_states"` string in `execute_whisper` was replaced with `self.encoder_hidden_name.as_str()`. CTC constructor sets the field to empty string (unused in CTC mode). If none of the expected names are found, model loading fails with a descriptive `ModelValidation` error.

### CR-02: WebSocket handler leaks internal error details to clients

**Files modified:** `crates/hephaestus-api/src/ws.rs`
**Commit:** d45e14b
**Applied fix:** Replaced `format!("inference failed: {e}")` with the static string `"inference failed"` in the WebSocket error message sent to clients. The full error is still logged server-side via `tracing::error!` with model_id and error details. This aligns with the sanitization pattern used by the HTTP handler (handlers.rs) and gRPC handler (grpc/inference.rs).

### CR-03: Unchecked `as u32` truncation in Whisper token decode

**Files modified:** `crates/hephaestus-core/src/pipeline.rs`
**Commit:** 5420ec3
**Applied fix:** Replaced bare `|&t| t as u32` cast with `u32::try_from(t).map_err(...)` pattern, matching the Seq2Seq pipeline's checked conversion (lines 621-628). On invalid token IDs, returns `CoreError::Inference` instead of silently truncating.

### WR-01: Missing readiness gate in WebSocket handler

**Files modified:** `crates/hephaestus-api/src/ws.rs`
**Commit:** 980ad0b
**Applied fix:** Added `state.is_ready()` check at the top of `ws_transcribe` before any parameter validation or WebSocket upgrade. Returns `ApiError::NotReady` (503) when service is not ready, consistent with the HTTP and gRPC handler patterns.

### WR-02: AudioBuffer infinite loop when window_samples truncates to zero

**Files modified:** `crates/hephaestus-api/src/ws.rs`
**Commit:** 5afb8ab
**Applied fix:** Added two `assert!` guards in `AudioBuffer::new`: (1) `window_samples > 0` to prevent infinite loop from zero-sized windows, and (2) `window_samples > overlap_samples` to ensure forward progress in the drain loop. Both include descriptive messages for debugging.

### WR-03: Whisper decoder output shape not validated before indexing

**Files modified:** `crates/hephaestus-core/src/pipeline.rs`
**Commit:** 822f05b
**Applied fix:** Added 3D shape validation (`logit_shape.len() != 3`) before accessing `logit_shape[2]`, matching the CTC path's validation pattern (lines 1186-1191). Also added bounds check on `logit_data.len()` before slicing `last_pos_logits`, preventing panics from undersized data. Both return descriptive `CoreError::Inference` errors.

### WR-04: `mel_spec` dependency not workspace-managed

**Files modified:** `Cargo.toml`, `crates/hephaestus-core/Cargo.toml`
**Commit:** 89de7fb
**Applied fix:** Added `mel_spec = "0.4"` to `[workspace.dependencies]` in root Cargo.toml. Changed crate-level declaration from `mel_spec = "0.4"` to `mel_spec.workspace = true`, aligning with the workspace dependency management pattern used by all other dependencies.

### WR-05: CTC vocab.json large max_id can exhaust memory

**Files modified:** `crates/hephaestus-core/src/pipeline.rs`
**Commit:** e4ffa7f
**Applied fix:** Added a safety check that rejects `vocab.json` files where the maximum token ID exceeds 1,000,000. Returns `CoreError::ModelLoad` with a descriptive message. This prevents malicious or corrupt vocab files from causing unbounded memory allocation.

### WR-06: No NaN/Inf validation on f32 PCM audio samples

**Files modified:** `crates/hephaestus-api/src/ws.rs`
**Commit:** 9f4638a
**Applied fix:** Added `is_finite()` check in `f32_bytes_to_samples`. Non-finite values (NaN, Infinity, -Infinity) from arbitrary byte patterns are replaced with `0.0` instead of propagating through mel spectrogram and ONNX inference.

### WR-07: WebSocket `channel` query parameter has no length limit

**Files modified:** `crates/hephaestus-api/src/ws.rs`
**Commit:** 89d1031
**Applied fix:** Added validation that rejects `channel` values longer than 256 characters with `ApiError::BadRequest`. Check is placed before sample rate and encoding validation in `ws_transcribe`, preventing memory waste from excessively long labels being cloned into every `TranscriptMessage`.

---

_Fixed: 2026-08-29T14:05:00Z_
_Fixer: Claude (gsd-code-fixer)_
_Iteration: 1_
