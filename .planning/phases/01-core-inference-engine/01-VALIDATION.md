---
phase: 01
slug: core-inference-engine
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-08-22
---

# Phase 01 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (Rust built-in) |
| **Config file** | Cargo.toml workspace |
| **Quick run command** | `cargo test --workspace` |
| **Full suite command** | `cargo test --workspace -- --include-ignored` |
| **Estimated runtime** | ~30 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test --workspace`
- **After every plan wave:** Run `cargo test --workspace -- --include-ignored`
- **Before `/gsd-verify-work`:** Full suite must be green
- **Max feedback latency:** 30 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 01-01-01 | 01 | 1 | XCUT-02 | — | N/A | build | `cargo build --workspace` | ❌ W0 | ⬜ pending |
| 01-01-02 | 01 | 1 | CORE-01 | — | N/A | integration | `cargo test --test model_load` | ❌ W0 | ⬜ pending |
| 01-01-03 | 01 | 1 | TOKN-01, TOKN-02 | — | N/A | integration | `cargo test --test tokenizer` | ❌ W0 | ⬜ pending |
| 01-01-04 | 01 | 1 | TOKN-03 | — | N/A | unit | `cargo test --lib validation` | ❌ W0 | ⬜ pending |
| 01-01-05 | 01 | 1 | PROF-01 | — | N/A | integration | `cargo test --test classifier` | ❌ W0 | ⬜ pending |
| 01-01-06 | 01 | 1 | CORE-02 | — | N/A | unit | `cargo test --lib config` | ❌ W0 | ⬜ pending |
| 01-01-07 | 01 | 1 | CORE-03 | — | N/A | integration | `cargo test --test warmup` | ❌ W0 | ⬜ pending |
| 01-01-08 | 01 | 1 | PROF-05, XCUT-01 | — | N/A | unit | `cargo test --lib pipeline` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `hephaestus-core/tests/` — integration test directory structure
- [ ] `hephaestus-core/src/lib.rs` — module structure with testable units
- [ ] Test model download helper (hf-hub based, cached in ~/.cache/huggingface)

*Existing infrastructure covers framework needs (cargo test is built-in).*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Deep module interface review | XCUT-01 | Trait surface area is a design judgment | Review all public traits: each should have 1-3 methods |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 30s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
