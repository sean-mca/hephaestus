---
phase: 05
slug: forge-conversion-service
status: draft
nyquist_compliant: true
wave_0_complete: false
created: 2026-08-26
---

# Phase 05 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework (Python)** | pytest >= 8.0 |
| **Framework (Rust)** | cargo test (built-in) |
| **Config file (Python)** | `forge/pyproject.toml` [tool.pytest.ini_options] |
| **Config file (Rust)** | None needed |
| **Quick run command (Python)** | `cd forge && uv run pytest tests/ -x` |
| **Quick run command (Rust)** | `cargo test -p hephaestus-resolve` |
| **Full suite command** | `cargo test --workspace && cd forge && uv run pytest tests/ -v` |
| **Estimated runtime** | ~30 seconds (Rust) + ~15 seconds (Python unit tests) |

---

## Sampling Rate

- **After every task commit:** Run quick command for the language touched (Python or Rust)
- **After every plan wave:** Run full suite command
- **Before `/gsd-verify-work`:** Full suite must be green
- **Max feedback latency:** 45 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 05-01-01 | 01 | 1 | FORG-01, FORG-02, FORG-03, FORG-04 | T-05-01, T-05-02 | model_id validated by Pydantic field_validator rejecting traversal and special chars; S3 key built from validated components only | import check | `cd forge && python -c "from forge.config import ForgeSettings; from forge.models import ConvertRequest, ConvertResponse, ConversionMetadata; from forge.converter import convert_model, validate_model; from forge.storage import upload_to_s3; print('all imports OK')"` | Wave 0 | pending |
| 05-01-02 | 01 | 1 | FORG-01, FORG-03 | T-05-03 | Sequential semaphore(1) + per-model Lock prevent resource exhaustion; asyncio.wait_for enforces timeout | import check | `cd forge && python -c "from forge.main import create_app; app = create_app(); print('app created, routes:', [r.path for r in app.routes])"` | Wave 0 | pending |
| 05-01-03 | 01 | 1 | FORG-01, FORG-02, FORG-03, FORG-04 | T-05-SC | uv.lock for reproducible installs; all SUS verdicts reviewed | unit | `cd forge && uv sync && uv run pytest tests/ -x -v` | Wave 0 | pending |
| 05-02-01 | 02 | 1 | FORG-03 | T-05-R01, T-05-R02 | Strongly-typed ForgeResponse rejects unexpected JSON; reqwest Client configured with timeout | unit | `cd /Users/seanmcauliffe/Repos/minerva/blacksmith && cargo test -p hephaestus-resolve --lib forge -- --nocapture` | Wave 0 | pending |
| 05-02-02 | 02 | 1 | FORG-03 | T-05-R03, T-05-R04 | ForgeConversion error captures HTTP status and body; FORGE_URL validated by reqwest during request | unit + build | `cd /Users/seanmcauliffe/Repos/minerva/blacksmith && cargo test --workspace` | Wave 0 | pending |

*Status: pending -- all tests created alongside implementation in Wave 1*

---

## Wave 0 Requirements

- [ ] `forge/pyproject.toml` -- pytest config and dev dependencies (pytest, httpx, moto[s3])
- [ ] `forge/tests/conftest.py` -- shared fixtures (mock S3 via moto, test ForgeSettings, temp directory)
- [ ] `forge/tests/test_api.py` -- endpoint tests covering FORG-03
- [ ] `forge/tests/test_converter.py` -- validation tests covering FORG-01, FORG-04
- [ ] `forge/tests/test_storage.py` -- S3 upload tests covering FORG-02
- [ ] Framework install: `cd forge && uv sync` installs pytest and test dependencies

*Note: Wave 0 test files are created in Plan 05-01 Task 3 alongside the Dockerfile. Rust tests are created inline with Plan 05-02 tasks (existing cargo test infrastructure).*

---

## Manual-Only Verifications

*All phase behaviors have automated verification.*

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references
- [x] No watch-mode flags
- [x] Feedback latency < 45s
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
