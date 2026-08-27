---
phase: 08-inference-quality-and-concurrency
fixed_at: 2026-08-26T12:30:00Z
review_path: .planning/phases/08-inference-quality-and-concurrency/08-REVIEW.md
iteration: 1
findings_in_scope: 4
fixed: 4
skipped: 0
status: all_fixed
---

# Phase 08: Code Review Fix Report

**Fixed at:** 2026-08-26T12:30:00Z
**Source review:** .planning/phases/08-inference-quality-and-concurrency/08-REVIEW.md
**Iteration:** 1

**Summary:**
- Findings in scope: 4
- Fixed: 4
- Skipped: 0

## Fixed Issues

### CR-01: Production `assert!` in Result-returning functions crashes server on unexpected model output

**Files modified:** `crates/hephaestus-core/src/postprocess.rs`, `crates/hephaestus-core/src/pipeline.rs`
**Commit:** 15ea1db
**Applied fix:** Replaced panicking `assert_eq!` and `assert!` with proper `Err(CoreError::Inference(...))` returns in three functions: `mean_pool` (return type changed from `Vec<f32>` to `Result<Vec<f32>, CoreError>`), `softmax_argmax_per_token`, and `argmax_per_token`. Updated callers in `pipeline.rs` (both single-inference and batch paths) to propagate the new `Result` with `?`. Updated two test call sites to use `.expect()`. Doc comment on `mean_pool` changed from `# Panics` to `# Errors`.

### WR-01: NER entity merging incorrectly spans across O-tagged gaps

**Files modified:** `crates/hephaestus-core/src/postprocess.rs`
**Commit:** fee168c
**Applied fix:** Added `prev_was_entity` boolean tracking to `merge_subword_entities`. The variable is set to `false` when an O-tagged word is encountered and `true` after processing any entity-tagged word. The `should_extend` condition now requires `prev_was_entity` to be true, preventing I-tags after O-tags from incorrectly extending a prior entity span. This matches standard BIO convention and HuggingFace transformers pipeline behavior.

### WR-02: Drain timeout watchdog does not actually force server shutdown

**Files modified:** `crates/hephaestus/src/main.rs`
**Commit:** cdbdba5
**Applied fix:** Replaced the `Notify`-in-`select!` pattern (where the `notified()` receiver was dropped when `shutdown_signal` completed first) with a `tokio::select!` that races the `serve_fut` against a watchdog async block. The watchdog polls for the readiness flip, waits the drain timeout, then its branch completes -- cancelling the serve future directly. Removed the spawned watchdog task and the `Arc<Notify>` indirection entirely.

### WR-03: Stale documentation references Mutex instead of RwLock

**Files modified:** `crates/hephaestus-core/src/pipeline.rs`, `crates/hephaestus-api/src/batcher.rs`
**Commit:** 5a39f1f
**Applied fix:** Updated `PipelineKind` doc comment in `pipeline.rs` from "holds `Mutex<PipelineKind>`" to "holds `RwLock<PipelineKind>`". Updated `batcher_loop` doc comment in `batcher.rs` from "Locks the pipeline mutex" to "Write-locks the pipeline RwLock".

---

_Fixed: 2026-08-26T12:30:00Z_
_Fixer: Claude (gsd-code-fixer)_
_Iteration: 1_
