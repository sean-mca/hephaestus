---
phase: 02
slug: http-serving-and-observability
status: draft
nyquist_compliant: false
wave_0_complete: false
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
| TBD | TBD | TBD | API-01 | — | N/A | integration | `cargo test --test api` | ❌ W0 | ⬜ pending |
| TBD | TBD | TBD | API-02 | — | N/A | integration | `cargo test --test api` | ❌ W0 | ⬜ pending |
| TBD | TBD | TBD | API-03 | — | N/A | integration | `cargo test --test health` | ❌ W0 | ⬜ pending |
| TBD | TBD | TBD | API-04 | — | N/A | Graceful drain | integration | `cargo test --test shutdown` | ❌ W0 | ⬜ pending |
| TBD | TBD | TBD | CORE-04 | — | N/A | unit | `cargo test --lib` | ❌ W0 | ⬜ pending |
| TBD | TBD | TBD | OBSV-01 | — | N/A | integration | `cargo test --test metrics` | ❌ W0 | ⬜ pending |
| TBD | TBD | TBD | OBSV-02 | — | N/A | unit | `cargo test --lib` | ❌ W0 | ⬜ pending |
| TBD | TBD | TBD | OBSV-03 | — | N/A | integration | `cargo test --test tracing` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] Test infrastructure for integration tests (axum test client)
- [ ] Shared fixtures for mock inference pipeline
- [ ] Test helpers for metrics endpoint parsing

*Existing `cargo test` infrastructure covers unit tests; integration test scaffolding needed for HTTP endpoints.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Graceful shutdown under load | API-04 | Requires signal handling + concurrent requests | Send SIGTERM during active requests, verify drain |
| OTel trace propagation | OBSV-03 | Requires OTel collector endpoint | Deploy with collector, verify traces appear |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 30s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
