---
phase: 08-inference-quality-and-concurrency
reviewed: 2026-08-26T12:00:00Z
depth: standard
files_reviewed: 8
files_reviewed_list:
  - crates/hephaestus-api/src/batcher.rs
  - crates/hephaestus-api/src/handlers.rs
  - crates/hephaestus-api/src/state.rs
  - crates/hephaestus-core/Cargo.toml
  - crates/hephaestus-core/src/pipeline.rs
  - crates/hephaestus-core/src/postprocess.rs
  - crates/hephaestus-core/tests/integration.rs
  - crates/hephaestus/src/main.rs
findings:
  critical: 1
  warning: 3
  info: 1
  total: 5
status: issues_found
---

# Phase 08: Code Review Report

**Reviewed:** 2026-08-26T12:00:00Z
**Depth:** standard
**Files Reviewed:** 8
**Status:** issues_found

## Summary

Phase 08 delivered three changes: NER score normalization via `softmax_argmax_per_token`, Mutex-to-RwLock migration for concurrent tokenization, and feature-gated integration tests. The RwLock migration is well-structured -- read/write lock scoping is correct, guards are properly dropped before subsequent lock acquisitions, and no lock is held across an `.await` point. The softmax implementation is numerically stable and the new function correctly produces probability scores.

However, the review identified one critical rule violation (panicking assertions in production Result-returning functions), a broken drain-timeout watchdog, an NER entity merging bug that incorrectly spans across O-tagged gaps, and stale documentation from the Mutex era.

## Critical Issues

### CR-01: Production `assert!` in Result-returning functions crashes server on unexpected model output

**File:** `crates/hephaestus-core/src/postprocess.rs:185-190`
**Also affects:** `crates/hephaestus-core/src/postprocess.rs:76-80`
**Issue:** `softmax_argmax_per_token` uses `assert_eq!` (line 185) and `assert!` (line 190) to validate inputs, but the function signature returns `Result<Vec<(usize, f32)>, CoreError>`. If an ONNX model produces an output tensor whose shape does not match `num_tokens * num_labels` (e.g., a model that internally truncates or pads output differently than input), the assertion panics and crashes the entire server process. The same pattern exists in `mean_pool` (line 76, `assert_eq!` on embedding dimensions).

This violates project rules `err-result-over-panic.md` ("Libraries should almost never panic") and `anti-panic-expected.md` ("Malformed data -> Return Err"). Model output shape mismatches are recoverable errors -- the server should return HTTP 500 to the affected request, not crash the process and kill all in-flight requests.

The production call chain is: `TokenClassifierPipeline::execute` -> `softmax_argmax_per_token` (line 648 of pipeline.rs) and `batch_postprocess_token_classifier` -> `softmax_argmax_per_token` (line 1112 of pipeline.rs). For embeddings: `EmbeddingsPipeline::execute` -> `mean_pool` (line 449 of pipeline.rs).

**Fix:**
```rust
// In softmax_argmax_per_token (postprocess.rs:185-190), replace:
assert_eq!(
    logits.len(),
    num_tokens * num_labels,
    "logits length must equal num_tokens * num_labels"
);
assert!(num_labels > 0, "num_labels must be positive");

// With:
if num_labels == 0 {
    return Err(CoreError::Inference(
        "num_labels must be positive".into(),
    ));
}
if logits.len() != num_tokens * num_labels {
    return Err(CoreError::Inference(format!(
        "logits length {} does not equal num_tokens ({}) * num_labels ({})",
        logits.len(), num_tokens, num_labels,
    )));
}

// In mean_pool (postprocess.rs:76-80), change return type to Result and replace:
assert_eq!(
    token_embeddings.len(),
    seq_len * hidden_dim,
    "token_embeddings length must equal seq_len * hidden_dim"
);

// With:
if token_embeddings.len() != seq_len * hidden_dim {
    return Err(CoreError::Inference(format!(
        "token_embeddings length {} != seq_len ({}) * hidden_dim ({})",
        token_embeddings.len(), seq_len, hidden_dim,
    )));
}
```

Note: changing `mean_pool` to return `Result` requires updating its callers in `pipeline.rs` (lines 449, 988) to propagate the error with `?`.

## Warnings

### WR-01: NER entity merging incorrectly spans across O-tagged gaps

**File:** `crates/hephaestus-core/src/postprocess.rs:285-288`
**Issue:** `merge_subword_entities` checks whether to extend the previous entity using only entity type matching, without verifying that the previous word was actually part of that entity (no intervening O-tagged words). When the model predicts `[B-LOC, O, I-LOC]`, the O token is skipped by `continue` (line 279), and `entities.last()` still points to the B-LOC entity. The I-LOC then extends the B-LOC, producing a single entity whose character span incorrectly includes the O-tagged word.

Standard BIO convention treats an I-tag after an O-tag as starting a new entity. The HuggingFace transformers pipeline follows this convention.

**Fix:** Track whether the immediately preceding word was part of the current entity chain. One approach:

```rust
// Add a tracking variable before the loop:
let mut prev_was_entity = false;

// In the loop body, after the O check:
if etype == "O" {
    prev_was_entity = false;
    continue;
}

// Modify should_extend to also require adjacency:
let should_extend = prev_was_entity
    && label.starts_with("I-")
    && entities.last().is_some_and(|prev| prev.entity == etype);

// At the end of each non-O iteration:
prev_was_entity = true;
```

### WR-02: Drain timeout watchdog does not actually force server shutdown

**File:** `crates/hephaestus/src/main.rs:270-298`
**Issue:** The watchdog task (lines 270-285) is designed to force shutdown after `shutdown_timeout` if graceful drain does not complete. It detects `!is_ready()`, waits the timeout, then calls `force_shutdown.notify_one()`. However, the `server_notify.notified()` future (line 294) is inside a `tokio::select!` with `shutdown_signal`. When SIGTERM arrives, `shutdown_signal` completes first, the select resolves, and `server_notify.notified()` is dropped. When the watchdog later fires `force_shutdown.notify_one()`, no receiver exists -- the notification is lost.

The consequence is that `axum::serve(...).await` (line 290) could hang indefinitely if in-flight connections never close, because no mechanism actually forces hard shutdown after the timeout. The normal SIGTERM graceful shutdown path works correctly; only the safety-net timeout is broken.

**Fix:** Replace the `Notify`-in-select approach with a mechanism that can actually interrupt the serve future after the timeout. For example, wrap `serve().await` in a `tokio::select!` with the watchdog timer directly:

```rust
let shutdown_signal_fut = shutdown_signal(server_state);
let serve_fut = axum::serve(listener, app)
    .with_graceful_shutdown(shutdown_signal_fut);

tokio::select! {
    result = serve_fut => {
        result.context("HTTP server error")?;
    }
    () = async {
        // Wait for ready=false, then drain timeout
        loop {
            if !watchdog_state.is_ready() { break; }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        tokio::time::sleep(shutdown_timeout).await;
        tracing::warn!("drain timeout exceeded, forcing shutdown");
    } => {
        // serve_fut is cancelled, server stops
    }
}
```

### WR-03: Stale documentation references Mutex instead of RwLock

**File:** `crates/hephaestus-core/src/pipeline.rs:681`
**Also affects:** `crates/hephaestus-api/src/batcher.rs:96`
**Issue:** The doc comment on `PipelineKind` (pipeline.rs:681) says "`AppState` holds `Mutex<PipelineKind>`" but the actual implementation in `state.rs` now uses `RwLock<PipelineKind>`. Similarly, `batcher_loop`'s doc comment (batcher.rs:96) says "Locks the pipeline mutex" but the code acquires a write lock on an `RwLock`.

**Fix:** Update the doc comments to reference `RwLock`:

```rust
// pipeline.rs:681
/// `AppState` holds `RwLock<PipelineKind>` instead of a concrete pipeline.

// batcher.rs:96
/// Write-locks the pipeline RwLock only during `execute_batch` -- never
/// during the collection phase per [`rules/anti-lock-across-await.md`].
```

## Info

### IN-01: `token_type_ids_array` allocated unconditionally in single-inference path

**File:** `crates/hephaestus-core/src/pipeline.rs:266`
**Issue:** `Array2::<i64>::zeros((1, seq_len))` for `token_type_ids` is allocated on every inference call, even when `needs_token_type_ids` is false (DistilBERT models). The allocation is small and unlikely to matter at inference latencies, but it is wasted work for the majority of models that do not require this input.
**Fix:** Move the allocation inside the `if needs_token_type_ids` branch. This requires restructuring the tensor creation to keep the borrow lifetimes valid within the branch.

---

_Reviewed: 2026-08-26T12:00:00Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
