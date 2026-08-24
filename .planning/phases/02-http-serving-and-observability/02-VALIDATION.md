---
phase: 02
slug: http-serving-and-observability
status: draft
nyquist_compliant: true
wave_0_complete: true
created: 2026-08-23
---

# Phase 02 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (Rust built-in) |
| **Config file** | `Cargo.toml` workspace test configuration |
| **Quick run command** | `cargo test --lib` |
| **Full suite command** | `cargo test --all` |
| **Estimated runtime** | ~30 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test --lib`
- **After every plan wave:** Run `cargo test --all`
- **Before `/gsd-verify-work`:** Full suite must be green
- **Max feedback latency:** 30 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 02-00-T2 | 02-00 | W1 | API-01 | — | N/A | integration | `cargo test -p hephaestus-api --test api` | Stub (02-00), populated by 02-01 | ⬜ pending |
| 02-00-T2 | 02-00 | W1 | API-02 | — | N/A | integration | `cargo test -p hephaestus-api --test api` | Stub (02-00), populated by 02-01 | ⬜ pending |
| 02-00-T2 | 02-00 | W1 | API-03 | — | N/A | integration | `cargo test -p hephaestus-api --test health` | Stub (02-00), populated by 02-01 | ⬜ pending |
| 02-00-T2 | 02-00 | W1 | API-04 | — | N/A | integration | `cargo test -p hephaestus-api --test shutdown` | Stub (02-00), populated by 02-01 | ⬜ pending |
| 02-00-T2 | 02-00 | W1 | CORE-04 | — | N/A | integration | `cargo test -p hephaestus-api --test metrics` | Stub (02-00), populated by 02-01 | ⬜ pending |
| 02-00-T2 | 02-00 | W1 | OBSV-01 | — | N/A | integration | `cargo test -p hephaestus-api --test metrics` | Stub (02-00), populated by 02-02 | ⬜ pending |
| 02-00-T2 | 02-00 | W1 | OBSV-02 | — | N/A | integration | `cargo test -p hephaestus-api --test tracing` | Stub (02-00), populated by 02-02 | ⬜ pending |
| 02-00-T2 | 02-00 | W1 | OBSV-03 | — | N/A | integration | `cargo test -p hephaestus-api --test tracing` | Stub (02-00), populated by 02-02 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [x] Test infrastructure for integration tests (axum test client) -- 02-00-PLAN.md creates crate skeleton and test stubs; 02-01 populates with axum test utilities
- [x] Shared fixtures for mock inference pipeline -- 02-01 creates mock pipeline utilities when populating test stubs
- [x] Test helpers for metrics endpoint parsing -- 02-02 populates metrics test stubs with Prometheus text assertions

*Wave 0 plan (02-00-PLAN.md) creates the hephaestus-api crate skeleton and all 5 integration test stub files. Stubs are #[ignore] until populated by 02-01 and 02-02.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Graceful shutdown under load | API-04 | Requires signal handling + concurrent requests | Send SIGTERM during active requests, verify drain |
| OTel trace propagation | OBSV-03 | Requires OTel collector endpoint | Deploy with collector, verify traces appear |

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references -- 02-00-PLAN.md creates all 5 test stub files
- [x] No watch-mode flags
- [x] Feedback latency < 30s
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** approved (Wave 0 plan 02-00-PLAN.md addresses all gaps)
