---
phase: 06
slug: opendal-storage-abstraction
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-08-26
---

# Phase 06 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (Rust built-in) / pytest (Python Forge) |
| **Config file** | Cargo.toml (workspace) / forge/pyproject.toml |
| **Quick run command (Rust)** | `cargo test -p hephaestus-resolve` |
| **Quick run command (Python)** | `cd forge && uv run pytest tests/` |
| **Full suite command** | `cargo test --workspace && cd forge && uv run pytest tests/` |
| **Estimated runtime** | ~45 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p hephaestus-resolve` (Rust plans) or `cd forge && uv run pytest tests/` (Python plan)
- **After every plan wave:** Run `cargo test --workspace && cd forge && uv run pytest tests/`
- **Before `/gsd-verify-work`:** Full suite must be green
- **Max feedback latency:** 30 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 06-01-T1 | 06-01 | 1 | STOR-01 | T-06-01 | validate_model_id before storage ops | unit | `cargo check -p hephaestus-resolve` | ❌ W0 (storage.rs is new) | ⬜ pending |
| 06-01-T2 | 06-01 | 1 | STOR-01 | T-06-01, T-06-SC | Operator replaces S3 client | unit | `cargo test -p hephaestus-resolve` | Existing (resolver tests updated) | ⬜ pending |
| 06-02-T1 | 06-02 | 2 | STOR-01 | T-06-05 | STORAGE_TYPE allowlist validation | unit | `cargo test -p hephaestus -- config::tests` | Existing (config tests extended) | ⬜ pending |
| 06-02-T2 | 06-02 | 2 | STOR-01 | T-06-05, T-06-06 | Operator wiring from validated config | build | `cargo build --workspace` | N/A (build check) | ⬜ pending |
| 06-03-CP | 06-03 | 1 | STOR-01 | T-06-SC | Human verifies opendal Python package | checkpoint | Human approval | N/A | ⬜ pending |
| 06-03-T1 | 06-03 | 1 | STOR-01 | T-06-08, T-06-09 | OpenDAL replaces boto3 | import | `cd forge && python -c "from forge.storage import upload_to_storage, build_operator"` | ❌ W0 (storage.py rewritten) | ⬜ pending |
| 06-03-T2 | 06-03 | 1 | STOR-01 | — | Tests use memory backend | unit | `cd forge && uv run pytest tests/test_storage.py -v` | Existing (tests rewritten) | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `crates/hephaestus-resolve/src/storage.rs` — new module with OpenDAL download/upload functions and Memory backend tests
- [ ] Updated `crates/hephaestus/src/config.rs` tests — STORAGE_* field assertions and validation tests
- [ ] Updated `forge/tests/test_storage.py` — opendal-based upload tests replacing boto3/moto tests
- [ ] Updated `forge/tests/conftest.py` — fixtures using opendal Memory backend instead of moto S3 mock

*All Wave 0 items are created inline within plan tasks (no separate test scaffolding step needed).*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| S3-compatible backend works with real AWS | STOR-01 | Requires AWS credentials and live S3 bucket | Deploy to staging, run integration test suite |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 30s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
