---
phase: 03-model-resolution
verified: 2026-08-26T00:00:00Z
status: passed
score: 7/10 must-haves verified
behavior_unverified: 3 # present + wired, no live-network test exercises the invariant (see behavior_unverified_items)
overrides_applied: 0
behavior_unverified_items:

  - truth: "On S3 cache hit, model loads from S3 without contacting HuggingFace (RSLV-01, D-01, D-02)"
    test: "Set S3_BUCKET to a real bucket pre-populated with {prefix}/{model_id}/model.onnx, tokenizer.json, config.json. Start the pod with MODEL_ID matching that key and no MODEL_PATH. Observe logs."
    expected: "Log line 'model resolved from S3 cache' appears (tier=s3); no HuggingFace network call is made; pod reaches readiness."
    why_human: "crates/hephaestus-resolve/src/s3.rs::download_model_from_s3 has no unit or integration test that exercises its actual get_object success path -- the only test covering that function (download_returns_existing_local_cache) short-circuits on the local-cache-hit branch before any S3 call is made. Exercising the real GetObject path requires a live or mocked S3 bucket, which the crate does not use (no trait abstraction over aws_sdk_s3::Client, so mockall cannot substitute it). RESEARCH.md's own test plan called for 'mockall setup for S3 and HF traits' as a Wave 0 deliverable; it was not implemented."

  - truth: "After HF download, model files are uploaded to S3 in background for future pods (RSLV-04, D-12, D-13)"
    test: "Start the pod with MODEL_ID set to an HF model with an ONNX export, S3_BUCKET configured, and no local/S3 cache present. After the pod reaches readiness, check the S3 bucket for {prefix}/{model_id}/model.onnx, tokenizer.json, config.json."
    expected: "Files appear in S3 within a few seconds of pod startup; a 'successfully cached model to S3' info log is emitted; the request that triggered the download is not blocked by the upload."
    why_human: "spawn_cache_back() and upload_model_to_s3() are never invoked by any test in the suite -- the closest coverage (upload_model_dir_discovers_files, upload_model_dir_handles_onnx_subdir) only asserts on filesystem fixture layout and never calls the actual upload function or a mocked S3 client. Requires a live S3 bucket to observe the end-to-end fire-and-forget path."

  - truth: "Operator deploys with MODEL_ID only (no MODEL_PATH), pod downloads model from HuggingFace and serves inference"
    test: "Set MODEL_ID=Xenova/distilbert-base-uncased-finetuned-sst-2-english, unset MODEL_PATH, unset S3_BUCKET. Start the binary. Send a classification request once readiness flips true."
    expected: "Pod downloads onnx/model.onnx, tokenizer.json, config.json from HuggingFace, constructs the pipeline, passes warmup, flips ready, and returns a valid classification for the request."
    why_human: "download_from_hf() in hf.rs is exercised by zero tests (unit or ignored-integration) -- only its pure helper split_model_id() is tested. The one live-network test in the workspace (hephaestus-core's classifier_e2e.rs) predates this phase and does not go through ModelResolver at all. No `crates/hephaestus-resolve/tests/resolve_e2e.rs` was created despite RESEARCH.md listing it as a Wave 0 gap. External HuggingFace network integration always needs human verification."
human_verification:

  - test: "Set S3_BUCKET to a real bucket pre-populated with model files under {prefix}/{model_id}/. Start pod with matching MODEL_ID, no MODEL_PATH. Watch logs."
    expected: "'model resolved from S3 cache' logged (tier=s3); no HuggingFace request occurs; pod becomes ready without internet access."
    why_human: "No test (mocked or live) exercises the S3 get_object success path in download_model_from_s3."

  - test: "Start pod with MODEL_ID pointing at an HF model with an ONNX export, S3_BUCKET configured, cold cache. After readiness, inspect the S3 bucket."
    expected: "model.onnx, tokenizer.json, config.json appear in S3 shortly after startup; pod did not wait on the upload to become ready."
    why_human: "spawn_cache_back()/upload_model_to_s3() are never invoked by any automated test."

  - test: "Start pod with MODEL_ID=Xenova/distilbert-base-uncased-finetuned-sst-2-english, no MODEL_PATH, no S3_BUCKET. Send a classification request after readiness."
    expected: "Model downloads from HuggingFace, pipeline builds, warmup passes, request returns a valid label + score."
    why_human: "download_from_hf() has zero test coverage (mocked or live); this is the phase's primary vertical-slice claim and needs a real network run to confirm."
---

# Phase 3: Model Resolution Verification Report

**Phase Goal:** Users can specify a model name and the runtime automatically resolves ONNX files from S3 cache or HuggingFace, building the cache as models are discovered
**Verified:** 2026-08-26
**Status:** human_needed
**Re-verification:** No — initial verification

## User Flow Coverage (Mode: mvp)

User story (from 03-01-PLAN.md / 03-02-PLAN.md): *"As a Kubernetes operator, I want to specify only a model name and have the runtime automatically resolve ONNX files from S3 cache or HuggingFace, so that pods self-provision without requiring pre-downloaded model files."*

| Step | Expected | Evidence | Status |
|------|----------|----------|--------|
| Operator sets MODEL_ID only (no MODEL_PATH) | Pod does not require pre-downloaded model files | `crates/hephaestus/src/main.rs:46-63` — branches on `config.model_path.is_some()`; when `None`, constructs `ModelResolver` and calls `resolver.resolve(&config.model_id)` | Present, wired |
| Runtime checks S3 cache first | S3 tier attempted before HuggingFace when `S3_BUCKET` is set | `crates/hephaestus-resolve/src/resolver.rs:128-150` — S3 tier gated on `(s3_client, s3_bucket)` both `Some`, checked before the HF block | ⚠️ Present, behavior unverified (no live/mocked S3 test) |
| Falls back to HuggingFace on S3 miss/absent | HF download attempted, ONNX files retrieved | `crates/hephaestus-resolve/src/resolver.rs:152-185`, `crates/hephaestus-resolve/src/hf.rs:34-102` | ⚠️ Present, behavior unverified (no test exercises `download_from_hf`) |
| Pod self-provisions and serves inference | Resolved `PathBuf` feeds `ClassifierPipeline::new()`, warmup runs, readiness flips, HTTP server accepts requests | `crates/hephaestus/src/main.rs:64-103` | ⚠️ Outcome unverified end-to-end (depends on the above) |

Standard technical-check sections follow below.

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | Operator deploys with MODEL_ID only (no MODEL_PATH), pod downloads model from HuggingFace and serves inference | ⚠️ PRESENT_BEHAVIOR_UNVERIFIED | Code path present and wired (main.rs -> resolver.rs -> hf.rs -> pipeline). No test, mocked or live, exercises `download_from_hf()`. |
| 2 | `resolve()` is the single public method hiding all download logic (RSLV-05, D-05) | ✓ VERIFIED | `ModelResolver` exposes only `new()` and `resolve()` as `pub async fn`; `spawn_cache_back` is private; tier logic (S3/HF/Forge) lives in `pub(crate)` modules. |
| 3 | `resolve()` validates MODEL_ID before any tier logic (T-03-01) | ✓ VERIFIED | `resolver.rs:126` calls `validate_model_id(model_id)?` as the first statement in `resolve()`. 9 unit tests cover path traversal, shell metacharacters, empty string, valid IDs — all pass. |
| 4 | Model without ONNX export returns clear NoOnnxExport error (D-04) | ✓ VERIFIED | `hf.rs:44-63` checks `onnx/model.onnx` then `model.onnx`, returns `ResolveError::NoOnnxExport` on double `EntryNotFound`. `error.rs` Display message contains model_id and "no ONNX export" — test `no_onnx_export_error_contains_model_id` passes. |
| 5 | Config accepts optional S3_BUCKET, S3_PREFIX, FORGE_URL env vars with None defaults (D-03, D-09) | ✓ VERIFIED | `config.rs:64-77` — three `Option<String>` fields with `#[serde(default)]`; `config_with_model_path()` test helper includes all three as `None`. |
| 6 | On S3 cache hit, model loads from S3 without contacting HuggingFace (RSLV-01, D-01, D-02) | ⚠️ PRESENT_BEHAVIOR_UNVERIFIED | `resolver.rs:128-150` gates the S3 block ahead of the HF block and returns early on `Ok(Some(path))`. `s3.rs::download_model_from_s3` real GetObject path is never exercised by a test — only the local-already-cached shortcut is tested. |
| 7 | After HF download, model files are uploaded to S3 in background for future pods (RSLV-04, D-12, D-13) | ⚠️ PRESENT_BEHAVIOR_UNVERIFIED | `resolver.rs:170` calls `self.spawn_cache_back(model_id, &model_dir)` in the HF success branch; `spawn_cache_back` uses `tokio::spawn` (fire-and-forget). Neither `spawn_cache_back` nor `upload_model_to_s3` is invoked by any test. |
| 8 | When no ONNX export exists and Forge is not configured, clear error mentions model name and Forge unavailability (RSLV-03, D-10) | ✓ VERIFIED | `StubForgeClient::convert()` returns `ResolveError::ForgeUnavailable`; tests `stub_forge_error_message_mentions_model` and `stub_forge_error_message_mentions_configuration` call the real function and assert on message content. |
| 9 | S3 downloads use atomic temp-dir-then-rename pattern to prevent serving partial files (D-06) | ✓ VERIFIED | `s3.rs:98-115` — `TempDir::new_in(parent)` (same filesystem), `tokio::fs::rename`, `temp_dir.keep()` after rename. Pattern re-verified in isolation by `atomic_download_creates_final_dir_via_rename` and `keep_prevents_destructor_cleanup`. |
| 10 | S3 upload retries with exponential backoff and logs warning on final failure without affecting serving (D-14) | ✓ VERIFIED | `spawn_cache_back` wraps `upload_model_to_s3` in `with_retry(3, 1s, ...)`; on exhaustion, logs `tracing::warn!` and returns from the spawned task without propagating to the caller. `with_retry` generic semantics are unit-tested (retry count, exhaustion, first-attempt success). |

**Score:** 7/10 truths verified (3 present, behavior-unverified)

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/hephaestus-resolve/src/resolver.rs` | `ModelResolver` struct with `new()` and `resolve()` | ✓ VERIFIED | Present, substantive, wired into `main.rs`; full 3-tier chain implemented. |
| `crates/hephaestus-resolve/src/error.rs` | `ResolveError` enum with S3, HuggingFace, NoOnnxExport, ForgeUnavailable, Io variants | ✓ VERIFIED | All 6 variants present (`InvalidModelId` also added per T-03-01, beyond the minimum). |
| `crates/hephaestus-resolve/src/hf.rs` | HuggingFace download with ONNX detection | ✓ VERIFIED (structurally) | `download_from_hf()` and `split_model_id()` present; logic matches RESEARCH.md pattern. Not exercised by any test (see truth #1). |
| `crates/hephaestus-resolve/src/lib.rs` | Module declarations and public re-exports | ✓ VERIFIED | `pub mod error/forge/resolver`, `pub(crate) mod hf/s3`, re-exports `ResolveError`, `ForgeClient`, `StubForgeClient`, `ModelResolver`. |
| `crates/hephaestus-resolve/src/s3.rs` | S3 download, upload, atomic cache operations with retry | ✓ VERIFIED (structurally) | `download_model_from_s3`, `upload_model_to_s3`, `format_s3_key`, `download_s3_file` present. Real network paths not exercised by any test (see truths #6, #7). |
| `crates/hephaestus-resolve/src/forge.rs` | ForgeClient trait with convert() method and StubForgeClient | ✓ VERIFIED | Single-method trait, `#[cfg_attr(test, mockall::automock)]`, `StubForgeClient` returns `ForgeUnavailable`, fully tested. |

### Key Link Verification

| From | To | Via | Status | Details |
|------|-----|-----|--------|---------|
| `crates/hephaestus/src/main.rs` | `crates/hephaestus-resolve/src/resolver.rs` | `ModelResolver::new()` + `resolve()` replaces `config.model_dir()` | ✓ WIRED | `main.rs:46-63` — branches correctly on `model_path.is_some()`. |
| `crates/hephaestus-resolve/src/resolver.rs` | `crates/hephaestus-resolve/src/hf.rs` | `hf::download_from_hf()` called from HF tier | ✓ WIRED | `resolver.rs:156` |
| `crates/hephaestus-resolve/src/resolver.rs` | `crates/hephaestus-core/src/pipeline.rs` | `resolve()` returns PathBuf consumed by `ClassifierPipeline::new()` | ✓ WIRED | `main.rs:71` |
| `crates/hephaestus-resolve/src/resolver.rs` | `crates/hephaestus-resolve/src/s3.rs` | `download_model_from_s3()` / `spawn_cache_back()` -> `upload_model_to_s3()` | ✓ WIRED | `resolver.rs:134, 251` |
| `crates/hephaestus-resolve/src/resolver.rs` | `crates/hephaestus-resolve/src/forge.rs` | `self.forge.convert()` | ✓ WIRED | `resolver.rs:189` |
| `crates/hephaestus-resolve/src/s3.rs` | AWS S3 service | `aws_sdk_s3::Client` get_object/put_object | ✓ WIRED (present, live behavior unverified) | `s3.rs:209-231, 173-182` |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Full workspace test suite (one run, all crates) | `cargo test --workspace` | 78 passed, 0 failed, 15 ignored | ✓ PASS |
| `hephaestus-resolve` unit tests | `cargo test -p hephaestus-resolve` | 38 passed, 0 failed | ✓ PASS |
| No debt markers in phase files | `grep -rn "TBD\|FIXME\|XXX" crates/hephaestus-resolve/src crates/hephaestus/src` | none found | ✓ PASS |
| Live HuggingFace download (`hf::download_from_hf`) | none available in suite | n/a | ? SKIP — no runnable entry point without live network |
| Live S3 GetObject/PutObject | none available in suite | n/a | ? SKIP — requires live/mocked AWS credentials and bucket |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|-------------|--------------|--------|----------|
| RSLV-01 | 03-02 | S3 cache check + load | ? NEEDS HUMAN | S3 tier wired first in `resolve()`; real GetObject success path untested (truth #6). |
| RSLV-02 | 03-01 | HF fallback download on S3 miss | ? NEEDS HUMAN | `download_from_hf()` present, error path (`NoOnnxExport`) verified in isolation; happy-path download never exercised by a test (truth #1). |
| RSLV-03 | 03-02 | Forge conversion call on HF miss | ✓ SATISFIED | Stub tier fully tested — `StubForgeClient::convert()` returns `ForgeUnavailable` with model name + config hint. |
| RSLV-04 | 03-02 | Upload to S3 after HF/Forge success | ? NEEDS HUMAN | `spawn_cache_back()` wired after HF success; upload path never invoked by a test (truth #7). |
| RSLV-05 | 03-01 | Single `resolve()` method abstracting 3-tier chain | ✓ SATISFIED | `ModelResolver` exposes only `new()`/`resolve()`; verified by code inspection and `resolve_rejects_invalid_model_id` test exercising the public surface. |

No orphaned requirements — all 5 phase requirement IDs (RSLV-01..05) appear in plan frontmatter and match REQUIREMENTS.md descriptions.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| `crates/hephaestus/src/config.rs` | 124 | Stale comment: `"MODEL_PATH is required (model resolution not yet implemented -- Phase 3)"` | ℹ️ Info | Message is now misleading — `model_dir()` is only reached when `model_path.is_some()` per `main.rs:46`, so this branch is effectively unreachable in the new flow, but the text still claims Phase 3 resolution doesn't exist. Not user-facing in practice; cosmetic cleanup recommended. |
| `crates/hephaestus-resolve/src/s3.rs`, `crates/hephaestus-resolve/src/hf.rs` | n/a | Testing-architecture deviation from RESEARCH.md's plan (mockall-based S3/HF trait abstraction, `resolve_e2e.rs` integration test) — not disclosed in either SUMMARY.md's "Deviations from Plan" section | ⚠️ Warning | The concrete `aws_sdk_s3::Client` and `hf_hub::HFClient` are called directly with no trait seam, so the real download/upload code paths cannot be unit-tested and were not integration-tested either. SUMMARY.md's "Known Stubs: None" (03-02) is accurate in the literal sense (no `todo!()`), but understates that core network paths have zero test coverage. |

No `TBD`, `FIXME`, or `XXX` markers found in phase-modified files. No blocking anti-patterns (no stub returns, no hardcoded empty data flowing to output).

## Deferred Items

None — no later-phase roadmap entries address the S3/HF live-network testing gap; Phase 5 (Forge) is a distinct concern (real HTTP Forge client), not S3/HF test coverage.

## Human Verification Required

### 1. S3 cache hit resolves without contacting HuggingFace

**Test:** Pre-populate a real (or LocalStack) S3 bucket with `{prefix}/{model_id}/model.onnx`, `tokenizer.json`, `config.json`. Start the pod with `S3_BUCKET`, `S3_PREFIX`, and matching `MODEL_ID` set, `MODEL_PATH` unset.
**Expected:** Log line `"model resolved from S3 cache"` (`tier=s3`) appears; no HuggingFace request occurs (verify via network monitoring or by blocking internet egress); pod reaches readiness.
**Why human:** `download_model_from_s3`'s real `get_object` success path has zero test coverage — the only passing test for that function short-circuits on the "already cached locally" branch before touching S3.

### 2. Background S3 cache-back after HuggingFace download

**Test:** Start the pod with `MODEL_ID` pointing at an HF model with an ONNX export, `S3_BUCKET` configured, cold local and S3 cache. After the pod reaches readiness, inspect the S3 bucket.
**Expected:** `model.onnx`, `tokenizer.json`, `config.json` appear under `{prefix}/{model_id}/` shortly after startup; a `"successfully cached model to S3"` info log appears; pod did not block readiness waiting for the upload.
**Why human:** `spawn_cache_back()` / `upload_model_to_s3()` are never invoked by any test in the suite.

### 3. End-to-end HuggingFace resolution and inference

**Test:** Start the pod with `MODEL_ID=Xenova/distilbert-base-uncased-finetuned-sst-2-english`, `MODEL_PATH` and `S3_BUCKET` unset. Once ready, send a classification request.
**Expected:** Model downloads from HuggingFace (`onnx/model.onnx`, `tokenizer.json`, `config.json`), pipeline constructs successfully, warmup passes, request returns a valid label + confidence score.
**Why human:** This is the phase's primary vertical-slice claim (D-title objective of 03-01-PLAN.md). `download_from_hf()` has zero automated test coverage, mocked or live.

## Gaps Summary

No FAILED truths, no MISSING/STUB artifacts, and no NOT_WIRED key links were found — every artifact required by the two plans exists, is substantive, and is wired into the resolution chain and into `main.rs`. The 3-tier orchestration logic (S3 -> HuggingFace -> Forge), the security gate (`validate_model_id`), the error taxonomy, and the Forge stub are all implemented and directly unit-tested (78/78 workspace tests pass, including 38/38 in `hephaestus-resolve`).

The gap is depth of test coverage, not missing implementation: the actual network-touching code paths (`hf::download_from_hf`, `s3::download_model_from_s3`'s real GetObject branch, `s3::upload_model_to_s3`) are never exercised by any test — neither live (an `#[ignore]`d integration test, which RESEARCH.md explicitly planned as a Wave 0 deliverable and which was never created) nor mocked (RESEARCH.md also planned `mockall` trait seams for S3 and HF operations, which were never introduced — both crates call the concrete AWS/HF SDK clients directly). This means the phase's central claim — "the runtime automatically resolves ONNX files from S3 cache or HuggingFace" — is proven by code inspection and structural tests but not by any executed behavior against a real or simulated external service. Three truths are downgraded to PRESENT_BEHAVIOR_UNVERIFIED and routed to human verification rather than counted as fully proven.

---

_Verified: 2026-08-26_
_Verifier: Claude (gsd-verifier)_
