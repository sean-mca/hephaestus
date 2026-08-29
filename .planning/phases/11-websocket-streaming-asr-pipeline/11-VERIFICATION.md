---
phase: 11-websocket-streaming-asr-pipeline
verified: 2026-08-29T13:55:00Z
status: human_needed
score: 9/9 must-haves verified
behavior_unverified: 0
overrides_applied: 0
human_verification:
  - test: "Stream real audio (16kHz PCM) to /ws/transcribe against a loaded wav2vec2 (CTC) ONNX model and confirm windowed transcript fragments arrive with correct text and correct chunk_index ordering."
    expected: "Non-empty, accurate transcript fragments streamed back as JSON TranscriptMessage objects while audio is sent; connection stays open across multiple windows; flush on close returns the final short window."
    why_human: "Requires a real ONNX model file, a live WebSocket client sending real audio, and human judgment of transcription accuracy and real-time pacing — none of which unit tests or grep can establish."
  - test: "Stream real audio to /ws/transcribe against a loaded Whisper (encoder+decoder+tokenizer) ONNX export and confirm the autoregressive decode loop produces a correct transcript without crashing."
    expected: "Decoder session accepts the hardcoded 'encoder_hidden_states' input name and produces valid text; loop terminates at eos_token_id well before max_target_positions for normal utterances."
    why_human: "crates/hephaestus-core/src/pipeline.rs:1261-1262 hardcodes the decoder's cross-attention input name ('encoder_hidden_states') with no load-time validation (unlike 'input_ids'/'decoder_input_ids', which are detected). Some Whisper ONNX exports use a different name (e.g. 'encoder_output'), which would load successfully but panic/crash the connection on first inference under the release profile's panic=\"abort\". This is flagged as CR-01 in 11-REVIEW.md; only a live run against real exported Whisper ONNX files can confirm the input name matches for the models actually deployed."
  - test: "Confirm real-time perceived latency of transcript fragments (i.e., fragments arrive promptly as ~30s windows complete, not all buffered until the very end of the stream)."
    expected: "Transcript fragments are pushed incrementally per completed window (or in response to the WINDOW_SIZE_SECS-configured chunk), not batched at connection close."
    why_human: "Real-time behavior/timing feel cannot be verified via static analysis; requires observing an actual streaming session."
---

# Phase 11: WebSocket Streaming & ASR Pipeline Verification Report

**Phase Goal:** Add a WebSocket streaming endpoint for real-time audio inference and an ASR model profile with 16kHz validation and feature extraction, enabling real-time transcription of audio streams
**Verified:** 2026-08-29T13:55:00Z
**Status:** human_needed
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths (Roadmap Success Criteria)

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | WebSocket endpoint accepts audio frames and streams transcript fragments back in real-time | ✓ VERIFIED (code path); real-time behavior ⚠️ needs human check | `crates/hephaestus-api/src/ws.rs` `ws_transcribe`/`handle_transcribe_socket` receive `Message::Binary`, convert PCM, buffer, run inference per window, send `TranscriptMessage` JSON incrementally. Route registered at `/ws/transcribe` (`routes.rs:39`). Timing/real-time feel is a human-verification item. |
| 2 | ASR models (e.g., Whisper ONNX) load and run inference via the existing PipelineKind dispatch | ✓ VERIFIED (load path + dispatch); Whisper execute-path robustness ⚠️ needs human check | `PipelineKind::Asr(AsrPipeline)` variant exists (`pipeline.rs:1319,1329`); `prepare`/`execute` dispatch tuples wired (`pipeline.rs:1354-1356,1567-1569`); `AsrPipeline::new_ctc`/`new_whisper` validate model input names at load time and construct sessions. CR-01 (unvalidated `encoder_hidden_states` input name) is a real runtime risk on the Whisper execute path — flagged in `11-REVIEW.md`, not yet fixed. |
| 3 | Audio preprocessing (16kHz validation, feature extraction) runs in Rust with no Python dependency | ✓ VERIFIED | `ws.rs` rejects `sample_rate != 16000` before upgrade (`ws.rs:209-214`, D-08). `crates/hephaestus-core/src/mel.rs` wraps `mel_spec` crate (pure Rust) for Whisper mel spectrograms; `crates/hephaestus-core/src/ctc.rs` implements CTC greedy decode in pure Rust. No Python/subprocess calls in any modified file. |
| 4 | Existing REST and gRPC endpoints remain unchanged | ✓ VERIFIED | `git diff` of `routes.rs` is purely additive (new `/ws/transcribe` route only). `handlers.rs`/`grpc/inference.rs` diffs are variable-rename + `.to_json()` call only — no behavior change. Full `cargo test --workspace -q` run: all suites pass, 0 failed. |

**Score:** 4/4 roadmap success criteria have supporting code that exists, is substantive, and is wired. 2 of the 4 truths (#1 real-time timing, #2 Whisper execute-path robustness) additionally require a human/live check that static verification cannot perform.

### Must-Have Truths (from PLAN frontmatter, 11-01/11-02/11-03)

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | `PipelineKind::prepare` accepts both text and audio via `InferenceInput` | ✓ VERIFIED | `pipeline.rs:1339-1360`; unit tests `inference_input_from_string_produces_text` etc. pass. |
| 2 | `execute`/`execute_batch` return typed `PipelineOutput` | ✓ VERIFIED | `pipeline.rs` `execute` returns `Result<PipelineOutput, CoreError>`; `handlers.rs`/`grpc/inference.rs` call `.to_json()` on the result. |
| 3 | Mismatched input/profile returns `CoreError::InvalidInput` | ✓ VERIFIED | `pipeline.rs:1360` catch-all arm for `(_, InferenceInput::Audio(_))`; `pipeline.rs:1574` catch-all for `(_, PreparedData::Audio(_))`; `error.rs` `InvalidInput` variant maps to `ApiError::BadRequest` (`hephaestus-api/src/error.rs`). |
| 4 | Existing REST/gRPC handlers work unchanged with text models | ✓ VERIFIED | See SC#4 above; `cargo test --workspace -q` all green. |
| 5 | Dynamic batcher operates with updated types | ✓ VERIFIED | `batcher.rs` uses `PreparedData`/`PipelineOutput` in `BatchRequest`, `submit`, and reply channel (`batcher.rs:15,23,25,67-68,139-141`). |
| 6 | WebSocket endpoint at `/ws/transcribe` with query param validation | ✓ VERIFIED | `ws.rs:203-219`; `sample_rate`/`encoding` validated pre-upgrade. |
| 7 | Non-16kHz connections rejected at upgrade | ✓ VERIFIED | `ws.rs:209-214`. |
| 8 | Non-f32/i16 encodings rejected at upgrade | ✓ VERIFIED | `AudioEncoding::from_str` called at `ws.rs:217`. |
| 9 | `AudioBuffer` windows with overlap; i16/f32 PCM conversion correct | ✓ VERIFIED | Unit tests in `ws.rs` (`audio_buffer_*`, `i16_bytes_to_f32_*`, `f32_bytes_to_samples_*`) all pass per `cargo test -p hephaestus-api`. |
| 10 | `ModelProfile::Asr` detected for CTC/Whisper architectures | ✓ VERIFIED | `profile.rs:51-59,80,109`; tests `test_detect_asr_from_ctc_architecture`, `test_detect_asr_from_whisper_architecture`, `test_detect_asr_from_pipeline_tag`, `test_override_asr` pass. |
| 11 | CTC greedy decoder correct on known logits | ✓ VERIFIED | `ctc.rs` `ctc_greedy_decode`; tests confirm collapse-repeat + blank-removal semantics (`ctc.rs` tests, all pass). |
| 12 | Mel spectrogram computes Whisper-compatible features | ✓ VERIFIED (shape only) | `mel.rs` wraps `mel_spec::stft::Spectrogram::compute_mel_spectrogram_cpu`; test verifies `[n_mels, num_frames]` shape and finite values on a synthetic sine wave. No test verifies numerical fidelity against a reference (e.g. librosa) mel spectrogram — acceptable for a shape/finiteness unit test, but exact Whisper-preprocessing parity is unverified. |
| 13 | `AsrPipeline` loads CTC (single file) and Whisper (encoder+decoder+tokenizer) models | ✓ VERIFIED (load path); execute path has known CR-01 gap | See SC#2 above. |
| 14 | `PipelineKind::Asr` dispatches prepare/execute | ✓ VERIFIED | See must-have #1/#3 evidence. |
| 15 | WebSocket handler calls `pipeline.prepare(InferenceInput::Audio)` and sends transcript back | ✓ VERIFIED | `ws.rs:379,385,389` via `run_asr_inference` helper. |
| 16 | `FEATURE_EXTRACTOR`/`CHUNKING_STRATEGY` env vars control ASR preprocessing | ✓ VERIFIED | `config.rs:106-124` fields, `config.rs:283-310` validation, tests pass. |
| 17 | `WINDOW_SIZE_SECS`/`OVERLAP_SECS` configurable | ✓ VERIFIED | `config.rs` same fields; `state.rs` `window_size_secs`/`overlap_secs` fields + accessors; `main.rs:201-208` passes config through to `AppState::new`; `ws.rs:245-249` reads from state. |
| 18 | Existing text pipelines and REST/gRPC endpoints remain unchanged | ✓ VERIFIED | Same evidence as SC#4. |

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/hephaestus-core/src/pipeline.rs` | `InferenceInput`, `PreparedData`, `PreparedAudio`, `PipelineOutput`, `AsrPipeline`, `PipelineKind::Asr` | ✓ VERIFIED | All types present, substantive (2142 lines, real logic), wired into dispatch and callers. |
| `crates/hephaestus-core/src/error.rs` | `InvalidInput` variant | ✓ VERIFIED | Present. |
| `crates/hephaestus-core/src/ctc.rs` | `ctc_greedy_decode` | ✓ VERIFIED | Present, 123 lines, 4 passing unit tests. |
| `crates/hephaestus-core/src/mel.rs` | `compute_mel_spectrogram` wrapping `mel_spec` | ✓ VERIFIED | Present, 118 lines, 2 passing unit tests. |
| `crates/hephaestus-core/src/profile.rs` | `ModelProfile::Asr` + detection | ✓ VERIFIED | Present, detection ordering correctly places Whisper check before generic `ForConditionalGeneration` suffix. |
| `crates/hephaestus-api/src/ws.rs` | `TranscribeParams`, `AudioBuffer`, PCM conversion, WS handler, pipeline wiring | ✓ VERIFIED | Present, 593 lines, real inference calls (not placeholder). |
| `crates/hephaestus-api/src/routes.rs` | `/ws/transcribe` registered | ✓ VERIFIED | `routes.rs:39`. |
| `crates/hephaestus/src/config.rs` | `FEATURE_EXTRACTOR`, `CHUNKING_STRATEGY`, `WINDOW_SIZE_SECS`, `OVERLAP_SECS` | ✓ VERIFIED | Present with validation and tests. |
| `crates/hephaestus/src/main.rs` | `AsrPipeline` construction branch, warmup skip | ✓ VERIFIED | `main.rs:176-177,245-248`. |
| Workspace `Cargo.toml` | axum `ws` feature, `futures-util` dep | ✓ VERIFIED | `Cargo.toml:46-47`. |

### Key Link Verification

| From | To | Via | Status | Details |
|------|-----|-----|--------|---------|
| `handlers.rs` / `grpc/inference.rs` | `PipelineOutput::to_json()` | Direct call after prepare/execute | ✓ WIRED | Confirmed at `handlers.rs:80,93` and `grpc/inference.rs:77,88`. |
| `CoreError::InvalidInput` | `ApiError::BadRequest` | `From<CoreError>` impl | ✓ WIRED | Confirmed present in `hephaestus-api/src/error.rs`. |
| `batcher.rs` | `PreparedData`/`PipelineOutput` channel types | `BatchRequest` struct + `submit`/`batcher_loop` | ✓ WIRED | Confirmed. |
| `ws.rs` `ws_transcribe` | `build_router` | `.route("/ws/transcribe", get(ws::ws_transcribe))` | ✓ WIRED | `routes.rs:39`. |
| `AudioBuffer::push` | Ready windows | Overlap-aware drain loop | ✓ WIRED | Unit-tested (`audio_buffer_push_returns_multiple_windows`, `audio_buffer_push_drains_correctly_with_overlap`). |
| `ws.rs` `run_asr_inference` | `PipelineKind::Asr` via `AppState` | `state.read_pipeline()` / `state.write_pipeline()` | ✓ WIRED | `ws.rs:372-389`. |
| `main.rs` | `AsrPipeline::new` | `ModelProfile::Asr` match arm | ✓ WIRED | `main.rs:176-177`. |
| `config.rs` fields | `AppState`/`AudioBuffer` | `main.rs` passes `config.window_size_secs`/`overlap_secs` into `AppState::new`; `ws.rs` reads `state.window_size_secs()`/`overlap_secs()` | ✓ WIRED | `main.rs:207-208`, `ws.rs:245-249`. |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Full workspace build | `cargo build --workspace` | Finished, 0 errors | ✓ PASS |
| Full workspace test suite (single run) | `cargo test --workspace -q` | All suites report `0 failed` (spot totals: 27, 38, 1, 69, 3, 49, ... all `ok`) | ✓ PASS |
| CTC greedy decode named test | Included in above run (`ctc::tests::*`) | 4/4 pass | ✓ PASS |
| Mel spectrogram shape test | Included in above run (`mel::tests::*`) | 2/2 pass, output shape `[n_mels, num_frames]`, all finite | ✓ PASS |
| ASR profile detection tests | Included in above run (`profile::tests::test_detect_asr_*`, `test_override_asr`) | pass | ✓ PASS |
| End-to-end WebSocket streaming with a real ONNX ASR model | N/A — no runnable model fixture in this environment | Not run | ? SKIP (routed to human verification) |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|--------------|-------------|-------------|--------|----------|
| PRFX-01 | 11-01, 11-03 | ASR profile (audio in, text out) | ✓ SATISFIED | `ModelProfile::Asr`, `AsrPipeline`, `PipelineKind::Asr` fully implemented and wired (see truths above). |
| APIX-02 | 11-01, 11-02, 11-03 | Streaming inference (SSE/WebSocket) — REQUIREMENTS.md v2 text says "for seq2seq models" | ✓ SATISFIED (scope reinterpreted for ASR, per ROADMAP) | `/ws/transcribe` WebSocket endpoint fully implemented for ASR audio streaming. Note: the original v2 requirement text in REQUIREMENTS.md describes streaming "for seq2seq models," but ROADMAP.md Phase 11 explicitly retargets APIX-02 to WebSocket audio/ASR streaming — this is a documented product decision at the roadmap level, not a phase implementation gap. |

**Traceability note (non-blocking):** `.planning/REQUIREMENTS.md` lists both `PRFX-01` and `APIX-02` under "v2 Requirements" (deferred/tracked list) without checkboxes and without an entry in the v1 "Traceability" table at the bottom of the file. ROADMAP.md correctly maps both IDs to Phase 11 and both are implemented, so the requirement IDs ARE accounted for — but REQUIREMENTS.md itself has not been updated post-completion to reflect that these v2 items are now delivered. This is a documentation-staleness issue, not a missing implementation.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| `crates/hephaestus-core/Cargo.toml` | 14 | `mel_spec = "0.4"` declared directly instead of `mel_spec.workspace = true` | ⚠️ Warning | Violates `rules/proj-workspace-deps.md`, which CLAUDE.md declares mandatory ("Rules compliance: Every file must adhere to all rules in `rules/`"). Already flagged as WR-04 in `11-REVIEW.md`. Does not affect runtime correctness, but is a real rules-compliance gap. |
| `crates/hephaestus-core/src/pipeline.rs` | 1261-1262 | Hardcoded `"encoder_hidden_states"` decoder input name, not validated at load time (unlike `input_ids`/`decoder_input_ids`) | ⚠️ Warning (CR-01 in 11-REVIEW.md) | Whisper ONNX exports using a different cross-attention input name will load successfully but crash on first inference (release profile has `panic = "abort"`). |
| `crates/hephaestus-core/src/pipeline.rs` | 1298-1302 | `t as u32` unchecked truncation on Whisper decoder output tokens | ⚠️ Warning (CR-03 in 11-REVIEW.md) | Inconsistent with the `u32::try_from()` pattern used elsewhere in this same file (Seq2Seq pipeline); theoretical crash risk on out-of-range token IDs. |
| `crates/hephaestus-api/src/ws.rs` | 298-308 | Internal error message (`format!("inference failed: {e}")`) sent verbatim to WebSocket client | ⚠️ Warning (CR-02 in 11-REVIEW.md) | Information-disclosure inconsistency: HTTP/gRPC handlers sanitize server errors to a generic message before sending to clients; the WebSocket handler does not. |
| `crates/hephaestus-api/src/ws.rs` | 203-219 | No `state.is_ready()` gate before WebSocket upgrade | ⚠️ Warning (WR-01 in 11-REVIEW.md) | HTTP and gRPC handlers both check readiness before serving; WebSocket does not, so new connections can be accepted during graceful-shutdown draining. |

No `TBD`/`FIXME`/`XXX`/`TODO`/`HACK`/`PLACEHOLDER` markers found in any of the 28 files modified by this phase. No blocker-level anti-patterns found.

**Note:** All five anti-pattern findings above were already independently identified and documented with concrete fixes in `.planning/phases/11-websocket-streaming-asr-pipeline/11-REVIEW.md` (code review artifact, status `issues_found`, 3 critical + 7 warning). This verification confirms those findings are real (spot-checked directly against the current codebase) rather than duplicating the full review. None of them are debt markers (no unreferenced `TBD`/`FIXME`/`XXX`), so none trigger the debt-marker gate; they are code-quality/robustness gaps, tracked for remediation via the review, not blockers to the phase's stated must-haves.

### Human Verification Required

### 1. Real-time WebSocket ASR transcription with a live CTC model

**Test:** Load a real wav2vec2 (or HuBERT) ONNX model via `MODEL_ID`, connect a WebSocket client to `/ws/transcribe?sample_rate=16000&encoding=i16&channel=test`, stream real 16kHz PCM audio, and observe the transcript messages returned.
**Expected:** Transcript fragments arrive as JSON `TranscriptMessage` objects with correct `chunk_index` and reasonably accurate `text`, delivered incrementally (not all at once at the end).
**Why human:** Requires a real ONNX model file and audio input; transcription accuracy and real-time pacing are not verifiable via static code inspection.

### 2. Whisper ONNX model end-to-end inference

**Test:** Load a real Whisper ONNX export (encoder + `decoder_model_merged.onnx` + `tokenizer.json`) via `MODEL_ID` with `FEATURE_EXTRACTOR=mel`, stream audio to `/ws/transcribe`, and confirm the decoder successfully runs to completion.
**Expected:** No crash on first inference; the autoregressive loop produces valid decoded text and terminates on `eos_token_id`.
**Why human:** `crates/hephaestus-core/src/pipeline.rs:1261-1262` hardcodes the decoder's `"encoder_hidden_states"` input name with no load-time validation. If the specific Whisper ONNX export in use names this input differently (e.g. `"encoder_output"`), the connection will crash on first inference under the release profile's `panic = "abort"`. Only a live run against the actual model files intended for deployment can confirm compatibility.

### 3. Perceived real-time streaming latency

**Test:** During a live streaming session, observe whether transcript fragments are pushed to the client progressively as each `WINDOW_SIZE_SECS` window completes, rather than buffered until connection close.
**Expected:** Fragments arrive with a cadence roughly matching the configured window size, not all at once at the end of the stream.
**Why human:** Timing/latency "feel" cannot be assessed from source code alone.

### Gaps Summary

No blocking gaps. All roadmap success criteria and all PLAN.md must-have truths for Phase 11 have corresponding, substantive, wired code, and the full workspace test suite (all crates) passes with zero failures. No debt markers, no stub/placeholder text, no orphaned artifacts, no unregistered routes.

The status is `human_needed` rather than `passed` because:
1. Two of the four roadmap success criteria (real-time streaming behavior, and reliable inference across arbitrary Whisper ONNX exports) assert runtime/behavioral properties that only a live run against real audio and real model files can confirm — no such fixture exists in this environment.
2. A concrete, already-documented (11-REVIEW.md CR-01) correctness risk exists on the Whisper execute path: the decoder's cross-attention input name is hardcoded and unvalidated, unlike the `input_ids`/`decoder_input_ids` detection used elsewhere in the same function. This does not fail phase's stated must-haves (which are about the AsrPipeline load path and PipelineKind dispatch, both of which are correctly implemented and unit-tested), but it is a real risk to the roadmap's "run inference" success criterion for some Whisper exports and should be either fixed or explicitly accepted before relying on Whisper support in production.

These items are routed to human verification per the escalation-gate pattern rather than blocking the phase, since the underlying code artifacts all exist, are substantive, and are wired correctly — the gap is in behavioral proof, not in implementation completeness.

---

_Verified: 2026-08-29T13:55:00Z_
_Verifier: Claude (gsd-verifier)_
