---
phase: 03
slug: model-resolution
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-08-24
---

# Phase 03 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (Rust built-in) |
| **Config file** | Cargo.toml workspace |
| **Quick run command** | `cargo test -p hephaestus-core` |
| **Full suite command** | `cargo test --workspace` |
| **Estimated runtime** | ~30 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p hephaestus-core`
- **After every plan wave:** Run `cargo test --workspace`
- **Before `/gsd-verify-work`:** Full suite must be green
- **Max feedback latency:** 30 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| TBD | TBD | TBD | RSLV-01 | — | N/A | integration | `cargo test --workspace` | ❌ W0 | ⬜ pending |
| TBD | TBD | TBD | RSLV-02 | — | N/A | integration | `cargo test --workspace` | ❌ W0 | ⬜ pending |
| TBD | TBD | TBD | RSLV-03 | — | N/A | integration | `cargo test --workspace` | ❌ W0 | ⬜ pending |
| TBD | TBD | TBD | RSLV-04 | — | N/A | integration | `cargo test --workspace` | ❌ W0 | ⬜ pending |
| TBD | TBD | TBD | RSLV-05 | — | N/A | integration | `cargo test --workspace` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] Integration tests for S3 resolver (mock S3 client)
- [ ] Integration tests for HuggingFace resolver (mock HTTP)
- [ ] Integration tests for 3-tier resolution chain fallback
- [ ] Unit tests for cache-back logic

*If none: "Existing infrastructure covers all phase requirements."*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| S3 connectivity with real AWS credentials | RSLV-01 | Requires live AWS environment | Deploy to staging, verify model loads from S3 |
| HuggingFace download of real model | RSLV-02 | Requires network + HF token | Run with HF_TOKEN set, verify model download |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 30s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
