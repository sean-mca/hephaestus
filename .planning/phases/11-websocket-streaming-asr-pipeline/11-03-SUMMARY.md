---
phase: 11-websocket-streaming-asr-pipeline
plan: 03
subsystem: core, api
tags: [asr, ctc, whisper, mel-spectrogram, websocket, streaming, onnx]

requires:
  - phase: 11-websocket-streaming-asr-pipeline
    plan: 01
    provides: InferenceInput/PreparedData/PipelineOutput type system
  - phase: 11-websocket-streaming-asr-pipeline
    plan: 02
    provides: WebSocket transport layer at /ws/transcribe
provides:
  - ModelProfile::Asr detection for CTC and Whisper architectures
  - CTC greedy decoder (ctc.rs) for wav2vec2/HuBERT models
  - Mel spectrogram module (mel.rs) wrapping mel_spec crate for Whisper preprocessing
  - AsrPipeline struct with CTC and EncoderDecoder modes
  - PipelineKind::Asr variant with full prepare/execute dispatch
  - FEATURE_EXTRACTOR, CHUNKING_STRATEGY, WINDOW_SIZE_SECS, OVERLAP_SECS config fields
  - WebSocket-to-pipeline inference integration with real transcript text
affects: []

tech-stack:
  added: [mel_spec 0.4]
  patterns:
    - "AsrPipeline deep-module pattern hiding CTC vs Whisper inference complexity"
    - "Dual-mode AsrMode enum for CTC greedy decode vs Whisper autoregressive generation"
    - "Configurable AudioBuffer window/overlap via AppState fields"

key-files:
  created:
    - crates/hephaestus-core/src/ctc.rs
    - crates/hephaestus-core/src/mel.rs
  modified:
    - crates/hephaestus-core/src/profile.rs
    - crates/hephaestus-core/src/pipeline.rs
    - crates/hephaestus-core/src/lib.rs
    - crates/hephaestus-core/Cargo.toml
    - crates/hephaestus/src/config.rs
    - crates/hephaestus/src/main.rs
    - crates/hephaestus-api/src/ws.rs
    - crates/hephaestus-api/src/state.rs

key-decisions:
  - "Whisper ForConditionalGeneration check precedes generic suffix to avoid Seq2Seq misdetection"
  - "mel_spec::stft::Spectrogram::compute_mel_spectrogram_cpu for batch mel computation"
  - "AsrPipeline loads vocab.json for CTC (blank_id from <pad> or | token) and tokenizer.json for Whisper"
  - "Whisper decoder input name detected at load time (input_ids vs decoder_input_ids)"
  - "Warmup skipped for ASR profile since audio input unavailable at startup"
  - "run_asr_inference helper encapsulates read/write lock pattern for WebSocket handler"

patterns-established:
  - "ASR model detection via architecture suffix (ForCTC) and exact match (WhisperForConditionalGeneration)"
  - "Config field validation against explicit allowlists (T-11-10)"
  - "WebSocket inference error sent as JSON error message without crashing connection"

requirements-completed: [PRFX-01, APIX-02]

coverage:
  - id: D1
    description: "ModelProfile::Asr detected for CTC architectures"
    requirement: "PRFX-01"
    verification:
      - kind: unit
        ref: "crates/hephaestus-core/src/profile.rs#test_detect_asr_from_ctc_architecture"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-core/src/profile.rs#test_detect_asr_from_hubert_ctc_architecture"
        status: pass
    human_judgment: false
  - id: D2
    description: "ModelProfile::Asr detected for Whisper architecture"
    requirement: "PRFX-01"
    verification:
      - kind: unit
        ref: "crates/hephaestus-core/src/profile.rs#test_detect_asr_from_whisper_architecture"
        status: pass
    human_judgment: false
  - id: D3
    description: "ModelProfile::Asr from pipeline_tag and override"
    verification:
      - kind: unit
        ref: "crates/hephaestus-core/src/profile.rs#test_detect_asr_from_pipeline_tag"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-core/src/profile.rs#test_override_asr"
        status: pass
    human_judgment: false
  - id: D4
    description: "CTC greedy decoder produces correct text from known logits"
    verification:
      - kind: unit
        ref: "crates/hephaestus-core/src/ctc.rs#test_ctc_greedy_decode_basic"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-core/src/ctc.rs#test_ctc_greedy_decode_all_blank"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-core/src/ctc.rs#test_ctc_greedy_decode_single_token"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-core/src/ctc.rs#test_ctc_greedy_decode_repeated_different_tokens"
        status: pass
    human_judgment: false
  - id: D5
    description: "Mel spectrogram outputs correct shape [n_mels, num_frames]"
    verification:
      - kind: unit
        ref: "crates/hephaestus-core/src/mel.rs#test_mel_spectrogram_shape_from_sine_wave"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-core/src/mel.rs#test_mel_spectrogram_rejects_short_audio"
        status: pass
    human_judgment: false
  - id: D6
    description: "Config validates FEATURE_EXTRACTOR and CHUNKING_STRATEGY"
    verification:
      - kind: unit
        ref: "crates/hephaestus/src/config.rs#test_validate_rejects_invalid_feature_extractor"
        status: pass
      - kind: unit
        ref: "crates/hephaestus/src/config.rs#test_validate_accepts_mel_and_none"
        status: pass
      - kind: unit
        ref: "crates/hephaestus/src/config.rs#test_validate_rejects_invalid_chunking_strategy"
        status: pass
      - kind: unit
        ref: "crates/hephaestus/src/config.rs#test_validate_rejects_negative_window_size"
        status: pass
      - kind: unit
        ref: "crates/hephaestus/src/config.rs#test_validate_rejects_overlap_exceeding_window"
        status: pass
    human_judgment: false
  - id: D7
    description: "All existing text pipelines and REST/gRPC endpoints unchanged"
    verification:
      - kind: integration
        ref: "cargo test --workspace -q (189 tests pass, 0 failures)"
        status: pass
    human_judgment: false

duration: 10min
completed: 2026-08-29
status: complete
---

# Phase 11 Plan 03: ASR Pipeline and WebSocket Integration Summary

**CTC and Whisper ASR pipeline with mel spectrogram preprocessing, profile detection, configurable windowed chunking, and end-to-end WebSocket-to-transcript data path**

## Performance

- **Duration:** 10 min
- **Started:** 2026-08-29T12:20:09Z
- **Completed:** 2026-08-29T12:30:33Z
- **Tasks:** 2
- **Files modified:** 11

## Accomplishments

- Added ModelProfile::Asr variant with architecture detection for ForCTC (wav2vec2, HuBERT) and WhisperForConditionalGeneration
- Created CTC greedy decoder in ctc.rs (argmax per timestep, collapse repeats, remove blanks)
- Created mel.rs wrapping mel_spec crate's compute_mel_spectrogram_cpu for Whisper-compatible log-mel features
- Built AsrPipeline struct supporting two modes: CTC (single ONNX session + vocab.json) and EncoderDecoder (Whisper encoder + decoder sessions + tokenizer)
- Added PipelineKind::Asr variant with full prepare/execute/execute_batch dispatch
- Added FEATURE_EXTRACTOR, CHUNKING_STRATEGY, WINDOW_SIZE_SECS, OVERLAP_SECS config fields with validation
- Wired AsrPipeline construction in main.rs with warmup skip for ASR profiles
- Replaced WebSocket placeholder empty text with real pipeline inference (prepare + execute)
- Made AudioBuffer window/overlap configurable via AppState fields
- T-11-07: ONNX input name validation at load time for both CTC and Whisper models
- T-11-08: Whisper decoder loop bounded by max_target_positions (default 448)
- T-11-10: Config fields validated against explicit allowlists at startup

## Task Commits

Each task was committed atomically:

1. **Task 1: ASR modules, profile detection, and AsrPipeline with PipelineKind::Asr variant** - `f45c9e0` (feat)
2. **Task 2: ASR configuration, binary wiring, and WebSocket-pipeline integration** - `b335383` (feat)

## Files Created/Modified

- `crates/hephaestus-core/src/ctc.rs` (created) - CTC greedy decoder with argmax, repeat collapse, blank removal
- `crates/hephaestus-core/src/mel.rs` (created) - Mel spectrogram computation wrapping mel_spec crate
- `crates/hephaestus-core/src/profile.rs` - ModelProfile::Asr variant and detection for ForCTC/Whisper architectures
- `crates/hephaestus-core/src/pipeline.rs` - AsrPipeline struct (CTC + Whisper modes), PipelineKind::Asr variant
- `crates/hephaestus-core/src/lib.rs` - Re-exports for AsrPipeline, pub mod ctc and mel
- `crates/hephaestus-core/Cargo.toml` - Added mel_spec = "0.4" dependency
- `crates/hephaestus/src/config.rs` - FEATURE_EXTRACTOR, CHUNKING_STRATEGY, WINDOW_SIZE_SECS, OVERLAP_SECS fields with validation
- `crates/hephaestus/src/main.rs` - AsrPipeline construction branch, warmup skip for ASR, new config log fields
- `crates/hephaestus-api/src/ws.rs` - Pipeline inference calls, run_asr_inference helper, configurable AudioBuffer
- `crates/hephaestus-api/src/state.rs` - window_size_secs and overlap_secs fields with accessors
- `Cargo.lock` - Updated for mel_spec dependency

## Decisions Made

- Whisper ForConditionalGeneration check placed before generic suffix match to prevent Seq2Seq misdetection
- mel_spec::stft::Spectrogram::compute_mel_spectrogram_cpu used for batch mel computation (simplest batch API)
- AsrPipeline loads vocab.json for CTC (blank_id resolved from <pad> or | token, default 0)
- Whisper decoder input name detected at load time (checks for both "input_ids" and "decoder_input_ids")
- Warmup skipped for ASR profile since no audio input is available at startup (readiness still set)
- run_asr_inference helper encapsulates read/write lock acquisition pattern for WebSocket handler
- Inference errors sent as JSON error messages to WebSocket client without crashing the connection

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None.

## Known Stubs

None - the previously stubbed TranscriptMessage.text (from Plan 11-02) is now populated with real ASR inference output.

## User Setup Required

None - no external service configuration required. ASR models can be loaded by setting MODEL_ID to a wav2vec2 or Whisper model identifier with FEATURE_EXTRACTOR and CHUNKING_STRATEGY env vars.

## Self-Check: PASSED

All 11 modified/created files verified present on disk. Both task commits (f45c9e0, b335383) verified in git log. SUMMARY.md written.

---
*Phase: 11-websocket-streaming-asr-pipeline*
*Completed: 2026-08-29*
