---
phase: 04-additional-profiles-and-dynamic-batching
verified: 2026-08-26T16:19:18Z
status: gaps_found
score: 10/10 must-haves present and wired (2 flagged with unresolved correctness/robustness bugs)
behavior_unverified: 0
overrides_applied: 0
mvp_mode_note: "ROADMAP.md sets Mode: mvp for this phase, but the phase goal text ('Users can serve multiple model types beyond classifiers and optionally enable request batching for throughput') is NOT in the required user-story format ('As a ..., I want to ..., so that ...'). Confirmed via `gsd-tools query user-story.validate` -> valid=false. Per references/verify-mvp-mode.md this is a discrepancy that should be reformatted via `/gsd mvp-phase 4`. Standard (non-MVP) goal-backward verification was performed instead so the phase is not left unverified; the roadmap's 5 Success Criteria (already written in user-observable form) were used as the truth set."
gaps:
  - truth: "User can deploy a token classifier model and receive per-token labels (NER/POS) from text input"
    status: partial
    reason: "TokenClassifierPipeline is implemented, wired, and returns entities for standard models, but 04-REVIEW.md (produced by the project's own code-review agent after this phase's execution, commit b62351f) identified two unresolved CRITICAL correctness bugs that were never fixed in a follow-up commit: (1) extract_id2label() does not validate that id2label keys are contiguous 0..N -- a model with a gap (e.g. {\"0\":\"NEG\",\"2\":\"POS\"}) silently shifts every subsequent label by one index with no error (CR-02); (2) merge_subword_entities() merges multi-token entity scores via pairwise averaging (prev.score + score) / 2.0 instead of a running average, so for N merged tokens the last token's score dominates exponentially and earlier tokens' contributions decay -- confidence scores for multi-word entities are mathematically wrong (CR-03)."
    artifacts:
      - path: "crates/hephaestus-core/src/pipeline.rs"
        issue: "extract_id2label (~line 1008) sorts id2label entries by numeric key and discards the keys without checking they are contiguous from 0 -- CR-02"
      - path: "crates/hephaestus-core/src/postprocess.rs"
        issue: "merge_subword_entities score merge (~line 239): prev.score = (prev.score + score) / 2.0 is a pairwise average, not a running average -- CR-03"
    missing:
      - "Validate id2label keys are contiguous 0..N in extract_id2label(); return CoreError::ModelValidation on gaps instead of silently shifting indices"
      - "Fix multi-token entity score merging to use a running sum/count (or equivalent) instead of repeated pairwise averaging"
  - truth: "Batching is disabled by default; when enabled, max batch size and max wait time are configurable per deployment"
    status: partial
    reason: "batch_enabled/batch_max_size/batch_max_wait_ms are present, default correctly (false/8/50), and are read from env -- but no startup validation exists anywhere in the codebase despite (a) the 04-03-PLAN.md threat model explicitly registering T-04-06 ('BATCH_MAX_SIZE too large', disposition 'mitigate', mitigation text 'Validate at startup: reject batch_max_size > 64 or < 1') and (b) the Config.batch_max_size doc comment in config.rs literally stating 'Values > 64 or < 1 are rejected at startup.' Independently reproduced: Batcher::new(0) calls tokio::sync::mpsc::channel(4 * 0) = channel(0), and tokio's bounded-channel constructor contains `assert!(buffer > 0, \"mpsc bounded channel requires buffer > 0\")` (verified in the vendored tokio 1.53.1 source) -- so BATCH_ENABLED=true with BATCH_MAX_SIZE=0 panics the whole process at startup instead of failing with a clear config error. This was independently flagged as CR-01 in 04-REVIEW.md and never fixed in a follow-up commit."
    artifacts:
      - path: "crates/hephaestus/src/config.rs"
        issue: "batch_max_size doc comment (line 91) asserts validation behavior that is not implemented anywhere"
      - path: "crates/hephaestus/src/main.rs"
        issue: "No config.validate() call exists before Batcher::new(config.batch_max_size as usize) is invoked at line 119"
    missing:
      - "Add a Config::validate() method rejecting batch_max_size outside [1, 64] when batch_enabled is true, called from main.rs immediately after Config::from_env()"
human_verification: []
---

# Phase 4: Additional Profiles and Dynamic Batching Verification Report

**Phase Goal:** Users can serve multiple model types beyond classifiers and optionally enable request batching for throughput
**Verified:** 2026-08-26T16:19:18Z
**Status:** gaps_found
**Re-verification:** No — initial verification

> **Note on Mode: mvp** — See `mvp_mode_note` in the frontmatter above. The phase goal is not in user-story format, so this report follows the standard (non-MVP) goal-backward methodology against the roadmap's 5 Success Criteria, which are already user-observable ("User can deploy an embeddings model and receive...").

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | User can deploy an embeddings model and receive L2-normalized float vectors from text input | ✓ VERIFIED | `EmbeddingsPipeline` (crates/hephaestus-core/src/pipeline.rs:334-395) runs ONNX inference, `postprocess::mean_pool` + `postprocess::l2_normalize` (postprocess.rs:63-107) applied; `PipelineKind::execute` serializes `{"embedding": [...]}`; main.rs constructs it on `ModelProfile::Embeddings`; 4 unit tests pass (`test_mean_pool_excludes_padding`, `test_mean_pool_single_token`, `test_l2_normalize_unit_vector`, `test_l2_normalize_zero_vector`) |
| 2 | User can deploy a seq2seq model and receive generated text from text input | ✓ VERIFIED | `Seq2SeqPipeline` (pipeline.rs:407-461) extracts output token IDs (i64, f32 fallback) and calls `tokenizer.decode()`; `PipelineKind::execute` serializes `{"generated_text": "..."}`; main.rs constructs it on `ModelProfile::Seq2Seq`; `cargo build --workspace` and `cargo test -p hephaestus-core --lib` pass |
| 3 | User can deploy a token classifier model and receive per-token labels (NER/POS) from text input | ⚠️ VERIFIED WITH GAP | `TokenClassifierPipeline` (pipeline.rs:473-608) runs argmax per token, `merge_subword_entities` groups BIO spans into `Entity` structs, `PipelineKind::execute` serializes `{"entities": [...]}`. Functions for the happy path (contiguous id2label, single-token entities). **However** two unresolved critical bugs from `04-REVIEW.md` (CR-02, CR-03 — see Gaps below) mean multi-token entity scores are mathematically wrong and non-contiguous id2label configs silently corrupt labels. |
| 4 | User can enable dynamic batching via configuration, collecting requests over a time window for batched inference | ✓ VERIFIED | `Batcher`/`batcher_loop` (crates/hephaestus-api/src/batcher.rs) implement bounded-mpsc collection with `tokio::select!` deadline; handler branches on `state.is_batching_enabled()` (handlers.rs:65-87); main.rs conditionally spawns `batcher_loop` when `config.batch_enabled`; `PipelineKind::execute_batch` (pipeline.rs:652-740) handles all 4 profile variants |
| 5 | Batching is disabled by default; when enabled, max batch size and max wait time are configurable per deployment | ⚠️ VERIFIED WITH GAP | `Config.batch_enabled` defaults to `false` (config.rs:87-88, test `test_batch_config_defaults`), `batch_max_size`/`batch_max_wait_ms` default to 8/50 and are env-configurable (`BATCH_MAX_SIZE`, `BATCH_MAX_WAIT_MS`). **However** no startup validation exists despite the plan's own threat-mitigation commitment (T-04-06) and doc-comment claim — `BATCH_MAX_SIZE=0` crashes the process (CR-01, see Gaps below) instead of failing with a clear config error |

**Score:** 5/5 roadmap Success Criteria functionally present and wired (2 carry unresolved correctness/robustness bugs documented below)

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/hephaestus-core/src/profile.rs` | `ModelProfile` enum + `detect_profile()` | ✓ VERIFIED | 4 variants; architectures-suffix -> pipeline_tag fallback -> override precedence; 13 unit tests pass |
| `crates/hephaestus-core/src/pipeline.rs` — `PipelineKind` | Classifier/Embeddings/Seq2Seq/TokenClassifier variants | ✓ VERIFIED | All 4 variants present; `prepare()`/`execute()`/`execute_batch()` dispatch correctly on each |
| `crates/hephaestus-core/src/pipeline.rs` — `EmbeddingsPipeline` | Implements `Pipeline` trait | ✓ VERIFIED | session+tokenizer fields, `Output = Vec<f32>` |
| `crates/hephaestus-core/src/pipeline.rs` — `Seq2SeqPipeline` | Implements `Pipeline` trait | ✓ VERIFIED | session+tokenizer fields, `Output = String`, decode via tokenizer |
| `crates/hephaestus-core/src/pipeline.rs` — `TokenClassifierPipeline` | Implements `Pipeline` trait | ✓ VERIFIED | session+tokenizer+id2label fields, `Output = Vec<Entity>` |
| `crates/hephaestus-core/src/postprocess.rs` | `mean_pool`, `l2_normalize`, `argmax_per_token`, `merge_subword_entities` | ✓ VERIFIED (with known bug in `merge_subword_entities`, see Gaps) | All 4 functions present, `pub(crate)`, unit-tested |
| `crates/hephaestus-api/src/state.rs` | `Mutex<PipelineKind>` + optional `Batcher` | ✓ VERIFIED | `pipeline: Mutex<PipelineKind>`, `batcher: Option<Batcher>`, `is_batching_enabled()`/`batcher()` accessors |
| `crates/hephaestus-api/src/handlers.rs` | `Json<serde_json::Value>` return, batching branch | ✓ VERIFIED | `InferResponse` struct removed; handler branches on `is_batching_enabled()`; model_id/latency_ms inserted post-execution |
| `crates/hephaestus-api/src/batcher.rs` | `Batcher`, `BatchRequest`, `batcher_loop`, `pad_and_stack` | ✓ VERIFIED | Bounded mpsc (capacity = 4× max_batch_size, verified by `test_batcher_channel_is_bounded`), oneshot fan-out, `pad_and_stack` lives in `pipeline.rs` per an intentional plan deviation (documented in 04-03-SUMMARY.md) |
| `crates/hephaestus/src/config.rs` | `model_profile`, `batch_enabled`, `batch_max_size`, `batch_max_wait_ms` fields | ✓ VERIFIED (doc comment overstates behavior, see Gaps) | All 4 fields present with correct serde defaults |

### Key Link Verification

| From | To | Via | Status | Details |
|------|-----|-----|--------|---------|
| `main.rs` | `hephaestus_core::detect_profile` | reads config.json, calls detect_profile before pipeline construction | ✓ WIRED | main.rs:76-87 |
| `main.rs` | `PipelineKind` construction | matches on `ModelProfile`, constructs correct variant | ✓ WIRED | main.rs:90-115 — all 4 arms construct real pipelines, no bail stubs remain |
| `PipelineKind::execute()` | handler/batcher | returns `serde_json::Value` consumed by both direct and batch paths | ✓ WIRED | handlers.rs:84-87 (direct), batcher.rs:150 -> pipeline.rs:743-775 |
| `handlers.rs` infer | `Batcher::submit` | prepare-under-lock, drop lock, submit-await (anti-lock-across-await) | ✓ WIRED | handlers.rs:69-80 — lock scope explicitly closes before `.submit(prepared).await` |
| `batcher_loop` | `PipelineKind::execute_batch` | pipeline mutex locked only during execute_batch | ✓ WIRED | batcher.rs:148-152 — lock acquired only around `execute_batch` call, released immediately after |
| `main.rs` | `batcher_loop` spawn | conditional on `config.batch_enabled` | ✓ WIRED | main.rs:118-151 |
| `TokenClassifierPipeline::prepare()` | `PreparedInput.encoding` | `Some(encoding)` set only by TokenClassifier, `None` elsewhere | ✓ WIRED | pipeline.rs:527-532 vs. 202-207 (tokenize_text sets None) |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|-------------|--------------|--------|----------|
| PROF-02 | 04-01 | Embeddings profile: tokenize, infer, L2-normalize, return vector | ✓ SATISFIED | `EmbeddingsPipeline`, tested |
| PROF-03 | 04-02 | Seq2seq profile: tokenize, infer, decode, return text | ✓ SATISFIED | `Seq2SeqPipeline`, single-pass per D-10 |
| PROF-04 | 04-02 | Token classifier profile: tokenize, infer, return per-token labels | ⚠️ SATISFIED WITH KNOWN BUGS | `TokenClassifierPipeline` functions for standard inputs; CR-02/CR-03 unresolved (see Gaps) |
| BTCH-01 | 04-03 | Configurable dynamic batching, window-based collection | ✓ SATISFIED | `Batcher` + `batcher_loop`, bounded mpsc |
| BTCH-02 | 04-03 | Disabled by default, enabled via config | ✓ SATISFIED | `batch_enabled` defaults false, tested |
| BTCH-03 | 04-03 | Max batch size + max wait configurable | ⚠️ SATISFIED WITH KNOWN BUG | Configurable and tested for valid values; CR-01 unresolved (invalid values crash rather than error, see Gaps) |

No orphaned requirements — REQUIREMENTS.md lists PROF-02/03/04, BTCH-01/02/03 for Phase 4, and all six IDs appear in the `requirements:` frontmatter of 04-01/04-02/04-03 PLAN.md.

### Anti-Patterns Found

Carried forward from `.planning/phases/04-additional-profiles-and-dynamic-batching/04-REVIEW.md` (produced by the code-review agent on 2026-08-26T19:45:00Z, the last commit in this phase's history — `b62351f`, with **no fix commits following it**). Independently re-verified CR-01 (tokio `mpsc::channel(0)` panic assertion confirmed in vendored source) and the config.rs doc-comment/code mismatch during this verification pass.

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| `crates/hephaestus/src/config.rs` + `main.rs` | config.rs:91-93, main.rs:119 | CR-01: `batch_max_size` doc comment claims startup validation ("Values > 64 or < 1 are rejected") that does not exist; `BATCH_MAX_SIZE=0` → `mpsc::channel(0)` → tokio assertion panic | 🛑 Blocker | Contradicts the plan's own threat mitigation (T-04-06); operator misconfiguration crashes the whole process instead of a clear error |
| `crates/hephaestus-core/src/pipeline.rs` | ~1008-1034 | CR-02: `extract_id2label` does not validate id2label keys are contiguous 0..N; silently misindexes labels on gaps | 🛑 Blocker | Silent, undetectable wrong predictions for both Classifier and TokenClassifier profiles on malformed/unusual config.json |
| `crates/hephaestus-core/src/postprocess.rs` | ~239 | CR-03: multi-token entity score merge uses pairwise average instead of running average — exponential recency bias | ⚠️ Warning | Confidence scores for multi-word NER entities are mathematically wrong |
| `crates/hephaestus-core/src/pipeline.rs` | ~843 | CR-04: batch path silently returns `""` label on out-of-range argmax index instead of erroring like the single-request path (line ~312) | ⚠️ Warning | Inconsistent error behavior between direct and batched inference |
| `crates/hephaestus-core/src/postprocess.rs` | 17-24, 34-45 | WR-01: `softmax`/`argmax_with_score` panic or return nonsense on empty input instead of `Result` | ⚠️ Warning | Violates project rule `err-result-over-panic.md` for malformed model output |
| `crates/hephaestus-core/src/pipeline.rs` | 446, 449, 904, 924 | WR-02: unchecked `i64 as u32` / `f32.round() as u32` casts in seq2seq decode can silently wrap/saturate | ⚠️ Warning | Garbled decoded text on malformed model output rather than a clear error |
| `crates/hephaestus/src/main.rs` | 119 | WR-03: `_receiver` binding is actually used (misleading underscore-prefix convention) | ℹ️ Info | Readability only, no functional bug |
| `crates/hephaestus-api/src/metrics.rs` | 28 | WR-04: `install_recorder()` returns `anyhow::Error` from a library crate | ⚠️ Warning | Violates `err-thiserror-lib.md` convention |
| `crates/hephaestus-core/src/pipeline.rs` | 297, 371, 444, 549, 821, 856, 894, 913, 946 | WR-05: `outputs[0]` direct indexing can panic on a model with zero output tensors | ⚠️ Warning | Defensive-programming gap for user-supplied models |
| `crates/hephaestus-core/src/pipeline.rs` | 571-587 | IN-01: dead code / leftover commentary in `TokenClassifierPipeline::execute` | ℹ️ Info | Readability only |
| `crates/hephaestus/src/config.rs` | 97-98 | IN-02: `batch_max_wait_ms` has no upper-bound validation against `request_timeout_secs` | ℹ️ Info | Confusing interaction, not a hard bug |
| `crates/hephaestus-api/src/batcher.rs` | 188 (test code) | New: `cargo clippy --workspace --all-targets -- -D warnings` fails on `let_unit_value` in a test | ℹ️ Info | Only surfaces with `--all-targets`; the plan's specified acceptance command `cargo clippy --workspace -- -D warnings` (without `--all-targets`) passes cleanly |

No `TBD`/`FIXME`/`XXX` debt markers found in any phase-modified file.

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Full workspace build | `cargo build --workspace` | Exit 0 | ✓ PASS |
| All unit + integration tests | `cargo test --workspace` | 91 unit tests pass (8+13+32+38 across 4 crates) + integration tests pass (several `ignored`, require real ONNX models) | ✓ PASS |
| Clippy (plan's exact acceptance command) | `cargo clippy --workspace -- -D warnings` | Exit 0 | ✓ PASS |
| Clippy including test targets | `cargo clippy --workspace --all-targets -- -D warnings` | 1 error: `let_unit_value` in `batcher.rs:188` (test code) | ✗ FAIL (not part of the plan's stated acceptance command, informational only) |
| tokio bounded mpsc requires buffer > 0 | grep vendored tokio 1.53.1 source | `assert!(buffer > 0, "mpsc bounded channel requires buffer > 0")` confirmed at `bounded.rs:160` | Confirms CR-01 is a real, reproducible panic path |

### Human Verification Required

None. All findings above are code-level and were resolved via static inspection, `cargo test`/`cargo clippy` execution, and direct verification of the tokio dependency's panic assertion — no runtime/visual/UX judgment is needed.

### Gaps Summary

The phase's own code-review agent (`04-REVIEW.md`, dated after the phase's execution commits) already found 4 critical and 5 warning issues and left the review status as `issues_found`. Git history shows no fix commits after `b62351f` ("docs(04): add code review report") — these findings were never addressed. Two of the four critical findings (CR-01, CR-02) directly undermine roadmap Success Criteria #3 and #5:

1. **CR-01 (blocker):** `BATCH_MAX_SIZE=0` (or any out-of-range value) is not validated at startup despite the phase's own threat model (T-04-06, disposition "mitigate") and doc-comment explicitly promising this validation. This crashes the process via a tokio internal assertion rather than a clear config error. Independently reproduced by inspecting the vendored tokio source.
2. **CR-02 (blocker):** `extract_id2label` does not validate that id2label keys are contiguous. A model with gaps in its label map silently produces wrong classifier/NER label assignments with no error signal — a data-integrity bug, not just a robustness gap.
3. **CR-03 (warning):** Multi-token NER entity confidence scores are computed via a mathematically incorrect running average, biasing toward the last-merged token.

These are real, reproducible, previously-identified-and-never-fixed defects — not new findings invented for this verification. Given the adversarial mandate to falsify SUMMARY.md claims rather than accept "complete" at face value, this phase is marked `gaps_found`. The core architecture (profile detection, PipelineKind dispatch, all four pipelines, the batcher) is sound, well-tested for the happy path, and matches every plan's must-haves at the presence/wiring level — the gaps are in edge-case robustness and a scoring-formula bug, not missing functionality.

**Recommended next step:** Route CR-01 and CR-02 (at minimum) through `/gsd-plan-phase --gaps` for a closure plan before proceeding to Phase 5, since CR-01 is a startup-crash risk on a documented, user-facing config knob and CR-02 is silent output corruption.

---

*Verified: 2026-08-26T16:19:18Z*
*Verifier: Claude (gsd-verifier)*
