---
phase: 04
slug: additional-profiles-and-dynamic-batching
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-08-26
---

# Phase 04 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (Rust built-in) |
| **Config file** | `Cargo.toml` workspace config |
| **Quick run command** | `cargo test --workspace` |
| **Full suite command** | `cargo test --workspace --all-features` |
| **Estimated runtime** | ~30 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test --workspace`
- **After every plan wave:** Run `cargo test --workspace --all-features`
- **Before `/gsd-verify-work`:** Full suite must be green
- **Max feedback latency:** 30 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 04-01-01 | 01 | 1 | PROF-02 | — | N/A | integration | `cargo test --test embeddings` | ❌ W0 | ⬜ pending |
| 04-01-02 | 01 | 1 | PROF-03 | — | N/A | integration | `cargo test --test seq2seq` | ❌ W0 | ⬜ pending |
| 04-01-03 | 01 | 1 | PROF-04 | — | N/A | integration | `cargo test --test ner` | ❌ W0 | ⬜ pending |
| 04-02-01 | 02 | 2 | BTCH-01 | — | N/A | integration | `cargo test --test batching` | ❌ W0 | ⬜ pending |
| 04-02-02 | 02 | 2 | BTCH-02 | — | N/A | unit | `cargo test batcher` | ❌ W0 | ⬜ pending |
| 04-02-03 | 02 | 2 | BTCH-03 | — | N/A | unit | `cargo test batch_config` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] Integration test stubs for each new profile (embeddings, seq2seq, NER)
- [ ] Integration test stubs for batching enabled/disabled paths
- [ ] Unit test stubs for batcher collect/dispatch logic

*Existing cargo test infrastructure covers framework needs.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Batch throughput improvement | BTCH-01 | Benchmark, not pass/fail | Run with/without batching, compare latency |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 30s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
