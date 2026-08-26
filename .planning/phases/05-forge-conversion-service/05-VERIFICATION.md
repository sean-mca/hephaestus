---
phase: 05-forge-conversion-service
verified: 2026-08-26T20:00:00Z
status: passed
score: 9/10 must-haves verified
behavior_unverified: 1
overrides_applied: 0
behavior_unverified_items:
  - truth: "Concurrent requests for the same model_id block and receive the same result; only one conversion runs at a time (D-08, D-10)"
    test: "Fire two or more concurrent ConversionQueue.convert(model_id) calls for the SAME model_id (with convert_model/validate_model/upload_to_s3 mocked to a slow coroutine) and confirm: (a) only one underlying conversion executes, (b) all callers receive the identical cached ConvertResponse, (c) a second call started after the first completes hits the cache and does not re-run convert_model."
    expected: "Semaphore(1) + per-model asyncio.Lock + _results cache produce exactly one execution and shared results across concurrent callers; on exception, temp dir is cleaned up and the lock is released so a retry is possible."
    why_human: "This is a concurrency/ordering invariant. Grep confirms asyncio.Semaphore(1), defaultdict(asyncio.Lock), and a _results dict are present and referenced inside convert(), but no test in forge/tests/ ever calls ConversionQueue.convert() unmocked or drives two concurrent calls through it — test_api.py always patches ConversionQueue.convert() itself, bypassing the semaphore/lock/cache logic entirely. The SUMMARY.md for 05-01 self-reports this exact gap (coverage id D5, human_judgment: true)."
human_verification:
  - test: "Fire two concurrent POST /convert (or two concurrent ConversionQueue.convert() calls) for the same model_id against a real (unmocked) queue and confirm only one conversion executes and both callers get the same result."
    expected: "Second caller blocks on the per-model lock, then returns the cached ConvertResponse from the first caller without re-invoking convert_model/validate_model/upload_to_s3."
    why_human: "No test exercises ConversionQueue with its real convert() logic (only patched/mocked in tests/test_api.py); the dedup mechanism has zero behavioral test coverage."
  - test: "Run `docker build -t forge:test forge/` from the repo root."
    expected: "Multi-stage build completes successfully and the resulting image starts, exposing GET /health returning 200."
    why_human: "No Docker daemon is available in this environment to execute the build. The Dockerfile was reviewed statically (multi-stage, uv sync, HEALTHCHECK, correct CMD) but never actually built. This matches 05-01-SUMMARY.md's own admission (coverage id D6, human_judgment: true)."
---

# Phase 05: Forge Conversion Service Verification Report

**Phase Goal:** Forge conversion service — standalone Python FastAPI service that converts HuggingFace models to ONNX format, validates conversions, uploads to S3, and returns results. Pairs with Rust-side HttpForgeClient integration completing the 3-tier resolution chain.
**Verified:** 2026-08-26T20:00:00Z
**Status:** human_needed
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths

**From 05-01-PLAN.md (Forge Python service):**

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | POST /convert triggers ONNX conversion via optimum and returns S3 paths + metadata | ✓ VERIFIED | `forge/src/forge/api.py:19-49` calls `queue.convert()`; `forge/src/forge/queue.py:61-92` `_do_convert` calls `convert_model` → `validate_model` → `upload_to_s3` in order, building `ConvertResponse`. `forge/src/forge/converter.py:22-53` `convert_model` calls `optimum.exporters.onnx.main_export` + `tokenizer.save_pretrained`. `tests/test_api.py::test_convert_success` (mocked queue) confirms endpoint contract; `tests/test_converter.py` exercises `validate_model` directly (real onnx.checker + ort inference). All 17 pytest tests pass (`uv run pytest tests/` — 17 passed). |
| 2 | Converted model validated with `onnx.checker.check_model()` AND dummy onnxruntime inference before S3 upload | ✓ VERIFIED | `converter.py:83-108` — Stage 1 `onnx.checker.check_model(model_path)` with file-path string (not `onnx.load`, avoiding pitfall 2). Stage 2 builds `ort.InferenceSession`, generates dummy inputs, calls `session.run()`. `queue.py:70-73` calls `convert_model` then `validate_model` strictly before `upload_to_s3` (line 76). Behavioral test `tests/test_converter.py::TestValidateModelSuccess::test_valid_model_passes` builds a real minimal ONNX graph via `onnx.helper` and runs `validate_model` against it — passes. 5 additional failure-mode tests (missing/invalid onnx, tokenizer, config) all pass. |
| 3 | Validated model files uploaded to S3 with correct prefix | ✓ VERIFIED | `storage.py:11-43` `upload_to_s3` builds keys as `{prefix}/{model_id}/{filename}` (or `{model_id}/{filename}` when prefix empty) — matches Rust resolver's `format_s3_key` in `crates/hephaestus-resolve/src/s3.rs:195` (verified by reading both implementations — same layout). `tests/test_storage.py` (moto mock_aws) — 4/4 tests pass: prefix layout, no-prefix layout, retrievability via `get_object`, subdirectory skip. |
| 4 | Concurrent requests for the same model_id block and receive the same result; only one conversion runs at a time (D-08, D-10) | ⚠️ PRESENT_BEHAVIOR_UNVERIFIED | `queue.py:20-59` — `ConversionQueue` has `asyncio.Semaphore(1)`, `defaultdict(asyncio.Lock)` per model_id, and a `_results` cache checked *inside* the per-model lock before acquiring the semaphore. Code is present and correctly wired into `api.py`. However, no test drives concurrent/real calls through `ConversionQueue.convert()` — `tests/test_api.py` always `patch.object(ConversionQueue, "convert", ...)`, which bypasses the semaphore/lock/cache entirely. There is no `test_queue.py`. See Human Verification below. |
| 5 | Forge runs as a persistent FastAPI service with health check | ✓ VERIFIED | `main.py:45-59` `create_app()` builds a `FastAPI` app with `lifespan` context manager (not deprecated `on_event`), registers router, and defines `GET /health` returning `{"status": "ok"}`. `__main__` block runs `uvicorn.run(...)` for persistent serving. `tests/test_api.py::TestHealthEndpoint::test_health_returns_ok` passes. (Container packaging of this persistent service is covered separately below — Docker build unverified.) |

**From 05-02-PLAN.md (Rust HttpForgeClient + resolver generalization):**

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 6 | HttpForgeClient sends POST /convert with JSON `{"model_id": ...}` and deserializes ForgeResponse containing s3_paths and metadata | ✓ VERIFIED | `crates/hephaestus-resolve/src/forge.rs:114-144` — `convert()` POSTs `ConvertRequest{model_id}` as JSON to `{base_url}/convert`, checks status, deserializes `ForgeResponse`. `forge_response_deserializes_from_json` test verifies deserialization of a representative payload (s3_paths + full ConversionMetadata) — passes. `http_forge_client_stores_base_url` / `http_forge_client_trims_trailing_slash` verify construction. Full network round-trip (actual POST against a live/mock server) is not tested — no HTTP-mocking crate (wiremock/mockito) present in `Cargo.toml` — but the implementation is straightforward reqwest glue code with no complex state, and error-mapping branches (non-success status, deserialization failure, send failure) are all present in the code and map to `ResolveError::ForgeConversion`. |
| 7 | reqwest Client has configurable timeout from FORGE_TIMEOUT_SECS env var with 600s default | ✓ VERIFIED | `crates/hephaestus/src/config.rs:79-81,133-135` — `forge_timeout_secs: u64` field, `default_forge_timeout_secs() -> 600`. `test_forge_timeout_default` passes. `forge.rs:101-111` `HttpForgeClient::new` applies `.timeout(Duration::from_secs(timeout_secs))`. `main.rs:62` wires `HttpForgeClient::new(forge_url, config.forge_timeout_secs)`. |
| 8 | ModelResolver accepts any ForgeClient implementation via generic type parameter with StubForgeClient as default | ✓ VERIFIED | `crates/hephaestus-resolve/src/resolver.rs:25` — `pub struct ModelResolver<F: ForgeClient = StubForgeClient>`. `new_with_stub` (line 79) and `new_with_client<F: ForgeClient>` (line 99) constructors both present. `resolver_new_without_s3_has_no_client`, `resolver_new_with_s3_creates_client` pass. |
| 9 | When FORGE_URL is set, main.rs constructs HttpForgeClient and passes it to ModelResolver; when unset, StubForgeClient is used | ✓ VERIFIED | `crates/hephaestus/src/main.rs:61-87` — explicit `if let Some(ref forge_url) = config.forge_url { HttpForgeClient::new(...) + new_with_client(...) } else { new_with_stub(...) }` branching. Verified by direct code reading; straightforward conditional with no hidden state. `cargo build --release` compiles cleanly. |
| 10 | ForgeConversion error variant captures HTTP status and body for debugging failed conversions | ✓ VERIFIED | `crates/hephaestus-resolve/src/error.rs:56-66` — `ForgeConversion { model_id, reason }` with `#[error("Forge conversion failed for model '{model_id}': {reason}")]`. `forge.rs:128-135` constructs `reason` as `"HTTP {status}: {body_text}"` on non-success responses, and `"invalid response: {e}"` on deserialize failure. These specific branches lack a dedicated unit test (no mock HTTP server), but the code is simple string formatting with no concurrency/state concerns. |

**Score:** 9/10 truths verified (1 present, behavior-unverified)

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `forge/pyproject.toml` | uv project, 9 runtime deps, dev group | ✓ VERIFIED | All deps present incl. added `pytest-asyncio` (undeclared in plan, added as deviation, justified) |
| `forge/src/forge/config.py` | ForgeSettings (BaseSettings) | ✓ VERIFIED | All 7 fields present with correct defaults |
| `forge/src/forge/models.py` | ConvertRequest/Response/Metadata | ✓ VERIFIED | field_validator rejects empty, `..`, special chars — matches Rust `validate_model_id` regex intent |
| `forge/src/forge/converter.py` | convert_model, validate_model | ✓ VERIFIED | Two-stage validation, ConversionError raised on failure, order correct |
| `forge/src/forge/storage.py` | upload_to_s3 | ✓ VERIFIED | Correct key layout, TransferConfig present |
| `forge/src/forge/queue.py` | ConversionQueue | ✓ VERIFIED (wiring) / ⚠️ (concurrency behavior unproven) | Semaphore(1) + per-model Lock + cache present and wired into api.py |
| `forge/src/forge/main.py` | create_app() factory | ✓ VERIFIED | lifespan pattern, /health route, uvicorn entrypoint |
| `forge/src/forge/api.py` | POST /convert router | ✓ VERIFIED | Error handling for ConversionError, TimeoutError, generic Exception |
| `forge/Dockerfile` | Multi-stage build | ✓ VERIFIED (structure) / ⚠️ (build unexecuted) | Correct multi-stage layout, uv sync, HEALTHCHECK, correct CMD — not actually built (no docker daemon in this environment) |
| `forge/tests/test_api.py` | Endpoint tests | ✓ VERIFIED | 7/7 tests pass |
| `forge/tests/test_converter.py` | Validation tests | ✓ VERIFIED | 6/6 tests pass |
| `forge/tests/test_storage.py` | S3 upload tests | ✓ VERIFIED | 4/4 tests pass |
| `crates/hephaestus-resolve/src/forge.rs` | HttpForgeClient, ForgeResponse, ConversionMetadata | ✓ VERIFIED | All types present, ForgeClient trait return type updated |
| `crates/hephaestus-resolve/src/error.rs` | ForgeConversion variant | ✓ VERIFIED | Present with model_id + reason fields |
| `crates/hephaestus-resolve/src/resolver.rs` | Generic ModelResolver<F> | ✓ VERIFIED | new_with_stub / new_with_client split, forge_url field removed |
| `crates/hephaestus/src/config.rs` | forge_timeout_secs field | ✓ VERIFIED | Default 600, tested |
| `crates/hephaestus/src/main.rs` | Conditional client construction | ✓ VERIFIED | if/else branching on config.forge_url |

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|----|--------|---------|
| `api.py` `/convert` | `queue.convert()` | direct call, `request.app.state.queue` | ✓ WIRED | Confirmed in api.py:30 |
| `queue._do_convert()` | `converter.convert_model()` → `validate_model()` → `storage.upload_to_s3()` | `asyncio.to_thread(...)` sequential calls | ✓ WIRED | Correct order confirmed in queue.py:70-82 (validate before upload) |
| `resolver.rs` tier 3 | `self.forge.convert(model_id)` | generic trait dispatch | ✓ WIRED | resolver.rs:206, destructures `ForgeResponse`, logs metadata, then re-derives S3 key from `model_id` via `s3::download_model_from_s3` (intentional design per plan — not literally consuming `s3_paths`, but key layout matches Forge's upload layout, confirmed in both `storage.py` and `s3.rs::format_s3_key`) |
| `main.rs` | `HttpForgeClient::new()` / `ModelResolver::new_with_client()` | conditional construction on `config.forge_url` | ✓ WIRED | main.rs:61-87 |
| `lib.rs` | public re-exports | `pub use forge::{...}` | ✓ WIRED | HttpForgeClient, ForgeResponse, ConversionMetadata, StubForgeClient, ForgeClient all exported |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|------------|--------------|--------|----------|
| FORG-01 | 05-01 | Forge is a persistent Python service converting HF models to ONNX via optimum | ✓ SATISFIED | main.py persistent FastAPI app + converter.py `main_export` |
| FORG-02 | 05-01 | Forge uploads converted ONNX files to S3 | ✓ SATISFIED | storage.py `upload_to_s3`, tested with moto |
| FORG-03 | 05-01, 05-02 | Forge exposes API Hephaestus calls when S3+HF lack ONNX | ✓ SATISFIED | api.py `/convert` + HttpForgeClient + resolver tier-3 wiring |
| FORG-04 | 05-01 | Forge validates ONNX integrity before uploading | ✓ SATISFIED | converter.py two-stage validation, called before `upload_to_s3` |

No orphaned requirements — REQUIREMENTS.md maps only FORG-01..04 to Phase 5, and both plans declare exactly these IDs (05-01 declares all four; 05-02 declares FORG-03, consistent with its scope).

### Anti-Patterns Found

None. Scanned all files modified in this phase (`forge/src/`, `forge/tests/`, `forge/Dockerfile`, `forge/pyproject.toml`, and the 5 modified Rust files) for `TBD`/`FIXME`/`XXX`/`TODO`/`HACK`/`PLACEHOLDER` and stub-return patterns (`return null`, empty handlers, hardcoded empty collections flowing to output). No matches found. No debt markers.

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Full Forge Python test suite | `cd forge && uv run pytest tests/ -v` | 17 passed | ✓ PASS |
| Full Rust workspace test suite | `cargo test --workspace` | 40 (hephaestus-resolve) + 14 (hephaestus) + others, all pass, 0 failed | ✓ PASS |
| Release build compiles | `cargo build --release` | Finished, no errors | ✓ PASS |
| Docker image build | `docker build -t forge:test forge/` | Not run — no Docker daemon available in this environment | ? SKIP (routed to human verification) |
| Concurrent dedup behavior | (would require a dedicated test driving `ConversionQueue.convert()` unmocked) | No such test exists | ? SKIP (routed to human verification) |

### Probe Execution

No `scripts/*/tests/probe-*.sh` probes declared or found for this phase. Skipped.

### Human Verification Required

### 1. ConversionQueue concurrency/dedup behavior

**Test:** Fire two (or more) concurrent calls to `ConversionQueue.convert("same/model", settings)` — with `convert_model`/`validate_model`/`upload_to_s3` mocked to a short-sleeping stand-in — and confirm only one underlying conversion runs, both callers receive the identical `ConvertResponse`, and a subsequent call after completion returns the cached result without re-running the pipeline.

**Expected:** Semaphore(1) + per-model `asyncio.Lock` + `_results` dict produce exactly one execution; concurrent callers block and share the result; on failure, the temp directory is cleaned up and the lock releases so retry is possible.

**Why human:** This is a concurrency/ordering invariant that presence-and-wiring checks cannot prove. No existing test exercises the real `ConversionQueue.convert()` logic — `tests/test_api.py` always patches `ConversionQueue.convert` itself. `05-01-SUMMARY.md` self-reports this exact gap (coverage `D5`, `human_judgment: true`).

### 2. Docker image build

**Test:** Run `docker build -t forge:test forge/` from the repo root, then run the container and curl `/health`.

**Expected:** Multi-stage build succeeds; container starts; `GET /health` returns `{"status": "ok"}`.

**Why human:** No Docker daemon was available in this verification environment. The Dockerfile was statically reviewed (correct multi-stage layout, `uv sync`, `HEALTHCHECK`, correct `CMD`) but never actually built. `05-01-SUMMARY.md` self-reports this same gap (coverage `D6`, `human_judgment: true`).

### Gaps Summary

No blocking gaps. All artifacts exist, are substantive, and are correctly wired. All automated tests (17 Python + 54 Rust) pass. Two items — the `ConversionQueue` concurrency/dedup invariant (D-08/D-10) and the Docker image build — are present in code/config but have no behavioral proof in this verification pass, matching the executing agent's own honest self-reported gaps in both SUMMARY.md files. Neither is a stub or missing piece; both need a human (or a follow-up automated test / a Docker-capable environment) to close out.

---

_Verified: 2026-08-26T20:00:00Z_
_Verifier: Claude (gsd-verifier)_
