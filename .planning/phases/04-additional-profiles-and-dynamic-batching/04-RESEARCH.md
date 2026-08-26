# Phase 4: Additional Profiles and Dynamic Batching - Research

**Researched:** 2026-08-26
**Domain:** ONNX inference pipeline profiles (embeddings, seq2seq, token classification) + dynamic request batching
**Confidence:** HIGH

## Summary

Phase 4 extends Hephaestus from a single-profile classifier runtime to a multi-profile inference engine supporting embeddings, seq2seq (fused single-pass), and token classification models. It also adds optional dynamic batching via a channel-based collector that accumulates requests over a configurable time window before running a single batched inference call.

The existing `Pipeline` trait with its two-step `prepare()`/`execute()` API was explicitly designed to enable this phase (Phase 1, D-04). All three new profiles share the same tokenization path (input_ids + attention_mask) but differ in post-processing: mean pooling + L2 normalization for embeddings, token ID decoding for seq2seq, and BIO tag merging for NER. Profile detection reads the model's `config.json` `architectures` field to auto-select the correct pipeline, with an optional `MODEL_PROFILE` env var override.

Dynamic batching is a zero-overhead opt-in: when disabled (default), requests flow through the existing `prepare()` then `execute()` path unchanged. When enabled, a background tokio task collects `PreparedInput` values from an mpsc channel, pads them to uniform sequence length, runs a single batched ONNX inference call, and fans results back to waiting handlers via oneshot channels. No new crate dependencies are required -- `tokio::sync::{mpsc, oneshot}` and `tokio::time` are already available transitively, and `ndarray` (already a dependency) handles tensor construction for batching.

**Primary recommendation:** Implement the three new pipeline types first (they are independent of each other), then generalize AppState/handler to PipelineKind enum dispatch, then add the batcher as an orthogonal layer on top.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- **D-01:** Auto-detect profile from the model's `config.json` -- use `architectures` field (e.g., `*ForSequenceClassification` -> classifier, `*ForTokenClassification` -> NER, `*ForConditionalGeneration` -> seq2seq) and `pipeline_tag` when present. Operators just set `MODEL_ID`. No mandatory profile config.
- **D-02:** Optional `MODEL_PROFILE` env var as an override for ambiguous models. When set, it takes precedence over auto-detection. When unset (the common case), auto-detection handles it.
- **D-03:** Enum dispatch via a `PipelineKind` enum wrapping each concrete pipeline. `AppState` holds `Mutex<PipelineKind>` instead of `Mutex<ClassifierPipeline>`. Match on variant in the handler. No trait objects, no dynamic dispatch overhead.
- **D-04:** Single `/infer` endpoint for all profiles. Response shape is determined by the model's output -- Hephaestus reads the ONNX graph's output tensors, applies profile-appropriate post-processing, and passes the result through. No hardcoded output schemas per profile.
- **D-05:** Output shapes are model-determined, not prescribed by Hephaestus. The runtime does NOT define fixed response structs per profile. It faithfully represents whatever the ONNX graph produces after post-processing.
- **D-06:** Channel-based batcher. Handler calls `prepare()` immediately, then sends the `PreparedInput` + a oneshot response sender into a tokio::mpsc channel. Background task collects up to `max_batch_size` or `max_wait_time`, calls `execute()` as a batch, fans results back via oneshot channels.
- **D-07:** When batching is disabled (the default), requests bypass the channel entirely and call `prepare()` then `execute()` directly -- identical to the current classifier flow. Zero overhead when batching is off.
- **D-08:** All profiles support batching. Operators enable it per-deployment. Profiles that don't benefit simply won't have it enabled.
- **D-09:** Three env vars following the existing envy config pattern: `BATCH_ENABLED` (bool, default false), `BATCH_MAX_SIZE` (u32, default 8), `BATCH_MAX_WAIT_MS` (u64, default 50).
- **D-10:** Single-pass inference only for v1. Support models exported as a single fused ONNX graph (e.g., via optimum with beam search baked in). No auto-regressive decode loop in Hephaestus. Full decode support deferred to a future phase.

### Claude's Discretion
- NER post-processing details (BIO tag handling, span merging vs raw tokens)
- Embeddings post-processing (L2 normalization, mean pooling strategy)
- Profile detection heuristics for ambiguous models (e.g., base encoder models without task heads)
- Response serialization approach for model-determined output shapes

### Deferred Ideas (OUT OF SCOPE)
- Auto-regressive seq2seq decoding (separate encoder/decoder ONNX files, token-by-token decode loop with KV cache) -- future phase
- gRPC API for high-throughput internal callers (v2 requirement APIX-01)
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| PROF-02 | Embeddings profile tokenizes input text, runs inference, applies L2 normalization, and returns a float vector | Mean pooling + L2 norm post-processing patterns documented in Architecture Patterns; EmbeddingsPipeline implements Pipeline trait |
| PROF-03 | Seq2seq profile tokenizes input text, runs inference, decodes output tokens, and returns generated text | Single-pass fused ONNX graph approach documented; uses tokenizer.decode() for output token decoding |
| PROF-04 | Token classifier profile tokenizes input text, runs inference, and returns per-token labels (NER, POS) | BIO tag post-processing and subword merging patterns documented in Architecture Patterns |
| BTCH-01 | Runtime supports configurable dynamic batching -- collecting requests over a short window and running as a single inference call | Channel-based batcher pattern using tokio mpsc + oneshot documented; tensor padding/stacking for batch execution covered |
| BTCH-02 | Dynamic batching is disabled by default; enabled via configuration per deployment | Config struct extension with BATCH_ENABLED=false default using existing envy pattern |
| BTCH-03 | Batching configuration includes max batch size and max wait time | BATCH_MAX_SIZE and BATCH_MAX_WAIT_MS env vars following established serde(default) pattern |
</phase_requirements>

## Project Constraints (from CLAUDE.md)

- **Language**: Rust 2024 edition, workspace resolver 3 -- no exceptions
- **Code Convention**: Traits follow Ousterhout deep module pattern (1-3 methods hiding significant complexity)
- **Rules**: All code must adhere to all rules in `rules/`
- **GSD Workflow**: Changes must go through GSD workflow commands
- **Config pattern**: No Clap -- env vars only via envy (k8s-only service)

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Profile detection (config.json parsing) | Core crate (`hephaestus-core`) | Binary crate (startup) | Core owns model knowledge; binary orchestrates startup |
| Pipeline implementations (embeddings, seq2seq, token classifier) | Core crate (`hephaestus-core`) | -- | All inference logic belongs in core |
| Post-processing (mean pooling, L2 norm, BIO tags) | Core crate (`hephaestus-core/postprocess.rs`) | -- | Extension of existing postprocess module |
| PipelineKind enum dispatch | Core crate (`hephaestus-core`) | API crate (state.rs) | Core defines the enum; API crate uses it in AppState |
| Dynamic batcher (channel collector, batch execution) | API crate (`hephaestus-api`) | -- | Batcher is a serving-layer concern, not core inference |
| Config extensions (MODEL_PROFILE, BATCH_*) | Binary crate (`hephaestus/config.rs`) | -- | All config lives in the binary crate |
| Handler generalization (/infer for all profiles) | API crate (`hephaestus-api/handlers.rs`) | -- | Handler dispatches via PipelineKind |

## Standard Stack

### Core (already in workspace -- no new dependencies)
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| ort | 2.0.0-rc.13 | ONNX inference | Already in workspace. Session::run accepts batched tensors natively. |
| ndarray | 0.17 | Tensor construction | Already in workspace. Array2 for batch tensor construction, concatenation. |
| tokenizers | 0.23 | Tokenization + decoding | Already in workspace. encode() for prepare, decode() for seq2seq output. |
| tokio | 1.53 | Async runtime + channels | Already in workspace. sync::{mpsc, oneshot} for batcher, time::sleep for max_wait. Features available transitively. |
| serde_json | 1.0 | Dynamic output serialization | Already in workspace. serde_json::Value for model-determined output shapes. |

### Supporting (no additions needed)
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| metrics | 0.24 | Metrics recording | StageTimer already handles this -- new profiles use the same timer interface |
| tracing | 0.1 | Instrumentation | #[instrument] on new pipeline methods |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| serde_json::Value for output | Per-profile response structs | Value is more flexible for D-05 (model-determined output); structs would contradict the decision |
| ndarray manual padding | tract-ndarray or batch helper | Not worth adding a dependency for simple padding logic |
| Enum dispatch (PipelineKind) | Trait objects (dyn Pipeline) | Enum dispatch has zero dynamic dispatch overhead and is locked by D-03 |

**Installation:**
```bash
# No new dependencies needed. All crates are already in the workspace.
```

**Version verification:** All versions verified against existing Cargo.toml workspace dependencies. No new packages to install. [VERIFIED: codebase grep]

## Package Legitimacy Audit

No new packages are installed in this phase. All dependencies (ort, ndarray, tokenizers, tokio, serde_json) are already present in the workspace Cargo.toml and were verified in prior phases.

**Packages removed due to [SLOP] verdict:** none
**Packages flagged as suspicious [SUS]:** none

## Architecture Patterns

### System Architecture Diagram

```
HTTP Request (POST /infer, JSON body)
  |
  v
Handler: validate input, call prepare()
  |
  +-- [batching disabled] --> lock PipelineKind mutex --> execute() --> serialize output --> HTTP Response
  |
  +-- [batching enabled]  --> send (PreparedInput, oneshot::Sender) to mpsc channel
                                |
                                v
                          Batcher Background Task
                            |
                            +-- collect up to max_batch_size items
                            +-- OR timeout after max_wait_ms
                            |
                            v
                          Pad inputs to uniform seq_len
                          Stack into batch tensors (batch_size, max_seq_len)
                          Lock PipelineKind mutex
                          execute_batch() --> single ONNX Session::run()
                          Split output tensors per-request
                          Apply per-profile post-processing
                          Fan results back via oneshot channels
                            |
                            v
                          Each handler receives its result --> serialize --> HTTP Response
```

### Recommended Project Structure
```
crates/
├── hephaestus-core/src/
│   ├── pipeline.rs          # Pipeline trait + ClassifierPipeline (existing)
│   │                        # + EmbeddingsPipeline, Seq2SeqPipeline, TokenClassifierPipeline (new)
│   │                        # + PipelineKind enum (new)
│   ├── postprocess.rs       # softmax, argmax (existing) + mean_pool, l2_normalize, merge_bio_spans (new)
│   ├── profile.rs           # Profile detection from config.json (new)
│   ├── error.rs             # CoreError (existing, may need new variants)
│   └── lib.rs               # Re-exports (update)
├── hephaestus-api/src/
│   ├── handlers.rs          # /infer handler generalized for PipelineKind (modify)
│   ├── state.rs             # AppState: Mutex<PipelineKind> + optional batcher handle (modify)
│   ├── batcher.rs           # Batcher: mpsc collector, batch execution, result fan-out (new)
│   └── ...                  # Other files unchanged
└── hephaestus/src/
    ├── config.rs            # Add MODEL_PROFILE, BATCH_ENABLED, BATCH_MAX_SIZE, BATCH_MAX_WAIT_MS (modify)
    └── main.rs              # Add profile detection, PipelineKind construction, batcher init (modify)
```

### Pattern 1: PipelineKind Enum Dispatch
**What:** Enum wrapping all concrete pipeline types, with methods that dispatch to the inner pipeline.
**When to use:** In AppState and handler -- anywhere the runtime needs to call prepare/execute without knowing the profile type.
**Example:**
```rust
// Source: D-03 decision, codebase pattern
pub enum PipelineKind {
    Classifier(ClassifierPipeline),
    Embeddings(EmbeddingsPipeline),
    Seq2Seq(Seq2SeqPipeline),
    TokenClassifier(TokenClassifierPipeline),
}

impl PipelineKind {
    /// Prepare input for any profile. All profiles accept text input.
    pub fn prepare(&self, input: String) -> Result<PreparedInput, CoreError> {
        match self {
            Self::Classifier(p) => p.prepare(input),
            Self::Embeddings(p) => p.prepare(input),
            Self::Seq2Seq(p) => p.prepare(input),
            Self::TokenClassifier(p) => p.prepare(input),
        }
    }

    /// Execute single inference and return model-determined output as JSON value.
    pub fn execute(&mut self, prepared: PreparedInput) -> Result<serde_json::Value, CoreError> {
        match self {
            Self::Classifier(p) => {
                let out = p.execute(prepared)?;
                // serialize ClassifierOutput to JSON value
                Ok(serde_json::json!({ "label": out.label, "score": out.score }))
            }
            Self::Embeddings(p) => {
                let out = p.execute(prepared)?;
                Ok(serde_json::json!({ "embedding": out }))
            }
            // ... other variants
        }
    }
}
```
[ASSUMED]

### Pattern 2: Channel-Based Dynamic Batcher
**What:** Background tokio task that collects requests from an mpsc channel and runs batched inference.
**When to use:** When BATCH_ENABLED=true.
**Example:**
```rust
// Source: rules/async-mpsc-queue.md, rules/async-oneshot-response.md
use tokio::sync::{mpsc, oneshot};

struct BatchRequest {
    prepared: PreparedInput,
    reply: oneshot::Sender<Result<serde_json::Value, CoreError>>,
}

async fn batcher_loop(
    mut rx: mpsc::Receiver<BatchRequest>,
    pipeline: Arc<Mutex<PipelineKind>>,
    max_batch_size: usize,
    max_wait: Duration,
) {
    let mut batch: Vec<BatchRequest> = Vec::with_capacity(max_batch_size);

    loop {
        // Wait for first request
        let first = match rx.recv().await {
            Some(req) => req,
            None => break, // channel closed
        };
        batch.push(first);

        // Collect more up to max_batch_size or max_wait
        let deadline = tokio::time::sleep(max_wait);
        tokio::pin!(deadline);

        loop {
            if batch.len() >= max_batch_size {
                break;
            }
            tokio::select! {
                _ = &mut deadline => break,
                Some(req) = rx.recv() => batch.push(req),
                else => break,
            }
        }

        // Execute batch and fan out results
        let mut pipeline = pipeline.lock().await;
        let inputs: Vec<PreparedInput> = batch.iter().map(|r| /* take prepared */).collect();
        let results = pipeline.execute_batch(inputs);
        // Fan results back via oneshot channels
        for (req, result) in batch.drain(..).zip(results) {
            let _ = req.reply.send(result);
        }
    }
}
```
[ASSUMED]

### Pattern 3: Mean Pooling + L2 Normalization for Embeddings
**What:** Post-processing for embeddings models that converts per-token hidden states to a single normalized sentence vector.
**When to use:** In EmbeddingsPipeline::execute() after ONNX inference.
**Example:**
```rust
// Source: [CITED: sbert.net/docs/package_reference/sentence_transformer/modules.html]
// Source: [CITED: huggingface.co/sentence-transformers/all-MiniLM-L6-v2/discussions/67]

/// Mean-pool token embeddings using the attention mask.
///
/// token_embeddings: shape (seq_len, hidden_dim) -- one sample
/// attention_mask: shape (seq_len,) -- 1 for real tokens, 0 for padding
/// Returns: shape (hidden_dim,) -- single pooled vector
pub(crate) fn mean_pool(token_embeddings: &[f32], attention_mask: &[i64], hidden_dim: usize) -> Vec<f32> {
    let seq_len = attention_mask.len();
    let mut pooled = vec![0.0f32; hidden_dim];
    let mut mask_sum = 0.0f32;

    for t in 0..seq_len {
        let mask_val = attention_mask[t] as f32;
        mask_sum += mask_val;
        for d in 0..hidden_dim {
            pooled[d] += token_embeddings[t * hidden_dim + d] * mask_val;
        }
    }

    let denom = mask_sum.max(1e-9);
    for d in 0..hidden_dim {
        pooled[d] /= denom;
    }
    pooled
}

/// L2-normalize a vector to unit length.
pub(crate) fn l2_normalize(v: &mut [f32]) {
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-12);
    for x in v.iter_mut() {
        *x /= norm;
    }
}
```
[CITED: sbert.net/docs/package_reference/sentence_transformer/modules.html]

### Pattern 4: BIO Tag Merging for NER
**What:** Post-processing that converts per-token BIO predictions into entity spans, handling subword tokens.
**When to use:** In TokenClassifierPipeline::execute().
**Example:**
```rust
// Source: [ASSUMED] based on standard NER pipeline patterns

pub struct Entity {
    pub word: String,
    pub entity: String,
    pub score: f32,
    pub start: usize,
    pub end: usize,
}

/// Merge subword token predictions into word-level entity spans.
///
/// For each token:
///   - Skip special tokens ([CLS], [SEP], padding)
///   - If it's a continuation subword (starts with ##), append to current word
///   - Otherwise, flush current entity if any, start new word
///   - Only first subword of a word gets its label used
///
/// Then group consecutive B-X I-X tokens into entity spans.
```
[ASSUMED]

### Pattern 5: Profile Detection from config.json
**What:** Auto-detect which pipeline to construct based on the model's `config.json` `architectures` field.
**When to use:** At startup, between model resolution and pipeline construction.
**Example:**
```rust
// Source: [CITED: huggingface.co/docs/transformers/main_classes/configuration]

pub enum ModelProfile {
    Classifier,
    Embeddings,
    Seq2Seq,
    TokenClassifier,
}

/// Detect model profile from config.json fields.
///
/// Priority: MODEL_PROFILE env override > architectures field > pipeline_tag
///
/// Architecture suffix heuristics:
///   *ForSequenceClassification  -> Classifier
///   *ForTokenClassification     -> TokenClassifier
///   *ForConditionalGeneration   -> Seq2Seq
///   *ForCausalLM                -> Seq2Seq (fused, single-pass)
///   *Model / *ForMaskedLM       -> Embeddings (base encoder, commonly used for embeddings)
pub fn detect_profile(config: &serde_json::Value, override_profile: Option<&str>) -> Result<ModelProfile, CoreError> {
    // Check override first (D-02)
    if let Some(profile_str) = override_profile {
        return parse_profile_string(profile_str);
    }

    // Check architectures field (D-01)
    if let Some(archs) = config.get("architectures").and_then(|v| v.as_array()) {
        for arch in archs {
            if let Some(name) = arch.as_str() {
                if name.ends_with("ForSequenceClassification") { return Ok(ModelProfile::Classifier); }
                if name.ends_with("ForTokenClassification") { return Ok(ModelProfile::TokenClassifier); }
                if name.ends_with("ForConditionalGeneration") { return Ok(ModelProfile::Seq2Seq); }
                // Base models default to embeddings
                if name.ends_with("Model") || name.ends_with("ForMaskedLM") {
                    return Ok(ModelProfile::Embeddings);
                }
            }
        }
    }

    // Fallback: check pipeline_tag if present
    if let Some(tag) = config.get("pipeline_tag").and_then(|v| v.as_str()) {
        match tag {
            "text-classification" | "sentiment-analysis" => return Ok(ModelProfile::Classifier),
            "token-classification" | "ner" => return Ok(ModelProfile::TokenClassifier),
            "text2text-generation" | "translation" | "summarization" => return Ok(ModelProfile::Seq2Seq),
            "feature-extraction" | "sentence-similarity" => return Ok(ModelProfile::Embeddings),
            _ => {}
        }
    }

    Err(CoreError::Config("unable to detect model profile from config.json".to_string()))
}
```
[CITED: huggingface.co/docs/transformers/main_classes/configuration]

### Anti-Patterns to Avoid
- **Holding Mutex across await in batcher:** The batcher must lock the pipeline mutex only for the duration of `execute_batch()`, not across the entire collection window. Lock, execute, unlock -- never hold while waiting for more requests. [CITED: rules/anti-lock-across-await.md]
- **Unbounded mpsc channel for batcher:** Use `mpsc::channel(capacity)` with a bounded buffer. Unbounded channels can cause OOM under load. Capacity should be a small multiple of max_batch_size (e.g., 4x). [CITED: rules/async-bounded-channel.md]
- **Fixed response structs per profile:** D-05 explicitly forbids this. Use `serde_json::Value` at the dispatch boundary.
- **Trait objects for pipeline dispatch:** D-03 explicitly requires enum dispatch, not `dyn Pipeline`.
- **Adding `execute_batch` to the Pipeline trait:** This would increase the trait to 3 required methods. Instead, put batch execution logic on `PipelineKind` directly or on each concrete type as an inherent method.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Tokenization | Custom tokenizer | `tokenizers` crate `encode()` and `decode()` | Exact fidelity with model training tokenizer is critical; any deviation degrades quality silently |
| Tensor construction for batch | Manual memory layout | `ndarray::Array2::from_shape_vec()` + padding | ndarray handles strides, memory layout, views correctly; manual buffers risk alignment bugs |
| Channel-based request collection | Custom queue + condvar | `tokio::sync::mpsc` + `oneshot` | Battle-tested async primitives with backpressure; custom solutions miss edge cases (drop handling, cancellation) |
| Timeout-based batch collection | Manual timer management | `tokio::time::sleep` + `tokio::select!` | select! correctly handles cancellation; manual approaches leak timers |
| Mean pooling math | ndarray broadcasting ops | Simple loop over flattened slice | For single-request inference, a loop is clearer and avoids ndarray broadcasting complexity. For batch, use ndarray. |

**Key insight:** This phase adds no new dependencies because the existing stack (ort, ndarray, tokenizers, tokio) already provides everything needed. The complexity is in the post-processing logic and batcher orchestration, not in the infrastructure.

## Common Pitfalls

### Pitfall 1: Forgetting to Pad Inputs for Batched Inference
**What goes wrong:** Different requests have different sequence lengths. Concatenating unpadded inputs produces invalid tensors with mismatched dimensions. ONNX Runtime rejects the input or produces garbage.
**Why it happens:** Single-request inference always has shape (1, seq_len) so padding is never needed. Batching requires (batch_size, max_seq_len) with all sequences padded to the same length.
**How to avoid:** Before stacking, find `max_seq_len` in the batch. Pad each request's `input_ids` with 0 (PAD token) and `attention_mask` with 0 to `max_seq_len`. Construct the batch tensor from padded vectors.
**Warning signs:** ONNX Runtime shape mismatch error; output tensor dimensions don't match expected batch size.

### Pitfall 2: Mean Pooling Without Attention Mask
**What goes wrong:** Pooling includes padding tokens in the average, diluting the embedding and producing incorrect vectors. Cosine similarity scores become unreliable.
**Why it happens:** Naive mean pooling (`sum / seq_len`) doesn't account for padding tokens that should be excluded.
**How to avoid:** Always multiply token embeddings by the expanded attention mask before summing, and divide by the sum of the attention mask (not the sequence length). Clamp denominator to 1e-9 to avoid division by zero.
**Warning signs:** Embeddings differ from Python SentenceTransformer output; cosine similarity gives unexpected results.

### Pitfall 3: Subword Token Handling in NER
**What goes wrong:** The model predicts labels for every subword token (including `##` continuations), but the user expects word-level entities. Reporting subword-level entities produces fragmented, unusable output.
**Why it happens:** BPE/WordPiece tokenizers split words into subword tokens. The model predicts a label for each subword.
**How to avoid:** Only use the prediction for the first subword token of each word. Skip subsequent subwords. The `tokenizers` crate's encoding provides `word_ids()` which maps tokens to their original word index -- use this to identify word boundaries.
**Warning signs:** Entity words appear as fragments (e.g., "New" and "##York" as separate entities).

### Pitfall 4: Seq2Seq Output Token Decoding
**What goes wrong:** Fused seq2seq ONNX models output raw token IDs, not text. Without decoding, the response is an array of integers.
**Why it happens:** The ONNX graph produces the generated sequence as token IDs. The Python pipeline silently decodes these.
**How to avoid:** After inference, extract the output tensor as `Vec<i64>`, convert to `Vec<u32>`, and call `tokenizer.decode(&ids, skip_special_tokens)` to get the generated text.
**Warning signs:** Response contains numeric arrays instead of text.

### Pitfall 5: Batcher Deadlock from Holding Pipeline Mutex During Collection
**What goes wrong:** If the batcher holds the pipeline mutex while waiting for more requests, no other request can call `prepare()` (which also needs the pipeline for tokenization). The system deadlocks.
**Why it happens:** Tempting to lock the pipeline early to "have it ready" for batch execution.
**How to avoid:** `prepare()` is called by each handler BEFORE sending to the batcher channel. The batcher only locks the mutex for the brief `execute_batch()` call. Collection happens entirely without holding the lock. This is why the two-step prepare/execute split exists (Phase 1, D-04).
**Warning signs:** Requests hang indefinitely when batching is enabled; throughput drops to zero.

### Pitfall 6: Oneshot Channel Dropped Without Sending
**What goes wrong:** If the batcher task panics or the pipeline returns an error for only some items in the batch, some oneshot senders may be dropped without sending. The corresponding handlers hang forever on `rx.await`.
**Why it happens:** Error handling in batch execution doesn't account for partial failures.
**How to avoid:** Always send a result (Ok or Err) through every oneshot channel. Use a `finally`-style pattern: if batch execution fails, send the error to all pending channels. Wrap the batch execution in a block that ensures all channels are resolved.
**Warning signs:** Requests occasionally hang with no timeout error; increases under load.

### Pitfall 7: PipelineKind Enum Variant Size Disparity
**What goes wrong:** If pipeline types have very different sizes, the enum wastes memory on every instance (size = largest variant).
**Why it happens:** Different pipelines may store different amounts of data (e.g., id2label vectors).
**How to avoid:** Check `std::mem::size_of::<PipelineKind>()` in tests. If variants differ significantly, Box the large variant per rules/mem-box-large-variant.md. In practice, all pipelines hold roughly the same fields (Session + Tokenizer + metadata), so this is unlikely to be an issue.
**Warning signs:** Clippy `large_enum_variant` warning.

## Code Examples

### Extending Config for Phase 4

```rust
// Source: crates/hephaestus/src/config.rs (existing pattern)
// [VERIFIED: codebase grep]

/// Add to Config struct:
/// Optional model profile override (env `MODEL_PROFILE`).
#[serde(default)]
pub model_profile: Option<String>,

/// Enable dynamic batching (env `BATCH_ENABLED`, default: false).
#[serde(default)]
pub batch_enabled: bool,

/// Maximum batch size when batching is enabled (env `BATCH_MAX_SIZE`, default: 8).
#[serde(default = "default_batch_max_size")]
pub batch_max_size: u32,

/// Maximum wait time in milliseconds for batch collection (env `BATCH_MAX_WAIT_MS`, default: 50).
#[serde(default = "default_batch_max_wait_ms")]
pub batch_max_wait_ms: u64,

fn default_batch_max_size() -> u32 { 8 }
fn default_batch_max_wait_ms() -> u64 { 50 }
```

### Tokenizer Decode for Seq2Seq Output

```rust
// Source: [ASSUMED] based on tokenizers crate API
// tokenizers::Tokenizer::decode converts token IDs back to text

let output_ids: Vec<u32> = output_tensor_i64.iter().map(|&id| id as u32).collect();
let generated_text = tokenizer
    .decode(&output_ids, true) // true = skip special tokens
    .map_err(|e| CoreError::Inference(e.to_string()))?;
```

### Batch Tensor Construction with Padding

```rust
// Source: [ASSUMED] based on ndarray API patterns
use ndarray::Array2;

fn pad_and_stack(inputs: &[PreparedInput]) -> (Array2<i64>, Array2<i64>) {
    let batch_size = inputs.len();
    let max_seq_len = inputs.iter().map(|i| i.sequence_length).max().unwrap_or(0);

    let mut input_ids = Array2::<i64>::zeros((batch_size, max_seq_len));
    let mut attention_mask = Array2::<i64>::zeros((batch_size, max_seq_len));

    for (i, inp) in inputs.iter().enumerate() {
        for (j, &id) in inp.input_ids.iter().enumerate() {
            input_ids[[i, j]] = id;
        }
        for (j, &mask) in inp.attention_mask.iter().enumerate() {
            attention_mask[[i, j]] = mask;
        }
    }

    (input_ids, attention_mask)
}
```

### Generalized Handler for PipelineKind

```rust
// Source: crates/hephaestus-api/src/handlers.rs (existing pattern)
// [VERIFIED: codebase grep]

pub async fn infer(
    State(state): State<Arc<AppState>>,
    Json(req): Json<InferRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Gate on readiness
    if !state.is_ready() { return Err(ApiError::NotReady); }
    if req.text.is_empty() { return Err(ApiError::BadRequest("text field must not be empty".into())); }

    let request_start = Instant::now();
    let timer = StageTimer::new(state.model_id().to_string());

    let result = tokio::time::timeout(state.request_timeout(), async {
        if state.is_batching_enabled() {
            // Batching path: prepare, then submit to batcher
            let pipeline = state.lock_pipeline().await;
            let prepared = timer.time("tokenization", || pipeline.prepare(req.text.clone()))?;
            drop(pipeline); // release lock before sending to batcher
            state.submit_batch(prepared).await
        } else {
            // Direct path: prepare + execute under lock
            let mut pipeline = state.lock_pipeline().await;
            let prepared = timer.time("tokenization", || pipeline.prepare(req.text.clone()))?;
            timer.time("inference", || pipeline.execute(prepared))
        }
    }).await;

    // ... timeout handling, metrics, response construction (same pattern as current)
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Per-model Python runtimes | Unified ONNX runtime (Hephaestus) | This project | Single binary replaces N services |
| Hardcoded profile per binary | Auto-detect from config.json | Phase 4 | Operators just set MODEL_ID |
| Serial request processing | Optional dynamic batching | Phase 4 | Throughput scales with batch size |
| Fixed response schemas | Model-determined output shapes | Phase 4 (D-05) | Runtime faithfully represents any model output |

**Deprecated/outdated:**
- None for this phase. All patterns are standard and current.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | ort Session::run accepts batch_size > 1 in the first tensor dimension and processes all samples in a single call | Architecture Patterns (Batching) | Batching would need to fall back to sequential execution, eliminating throughput benefit |
| A2 | Fused seq2seq ONNX models (exported via optimum with beam search) output a tensor of generated token IDs that can be decoded with tokenizer.decode() | Architecture Patterns (Seq2Seq) | Seq2Seq pipeline would need different output handling; D-10 scope may need revision |
| A3 | The tokenizers crate Encoding provides word_ids() mapping subword tokens to original word indices | Pitfall 3 (Subword handling) | NER subword merging would need a different approach (e.g., tracking ## prefixes manually) |
| A4 | Embeddings ONNX models output hidden states with shape (batch, seq_len, hidden_dim) at output index 0 | Pattern 3 (Mean Pooling) | Different models may output at different indices; would need output name matching |
| A5 | Pipeline_tag field exists in config.json for sentence-transformer and task-specific models | Pattern 5 (Profile Detection) | pipeline_tag may only be in model card metadata (not config.json); detection would fall back to architectures only |
| A6 | `*ForMaskedLM` and `*Model` architecture suffixes indicate base encoder models suitable for embeddings | Pattern 5 (Profile Detection) | Some `*Model` architectures may be decoders (GPT2Model); heuristic needs refinement |

## Open Questions (RESOLVED)

1. **Word IDs availability in tokenizers crate** (RESOLVED)
   - What we know: Python tokenizers provides `encoding.word_ids` for subword-to-word mapping
   - Resolution: Verified — `tokenizers` 0.23.1 exposes `Encoding::get_word_ids(&self) -> &[Option<u32>]` at `src/tokenizer/encoding.rs:129`. Plan 04-02's `merge_subword_entities` can use `encoding.get_word_ids()` directly.

2. **Seq2Seq fused model output format**
   - What we know: Optimum can export seq2seq models as single fused ONNX graphs with beam search
   - What's unclear: Exact output tensor name and shape (e.g., `sequences` vs `output_ids`, 2D vs 3D for beam outputs)
   - Recommendation: Test with a real fused model (e.g., `t5-small` exported with `--task text2text-generation-with-past`). Inspect output tensor names at load time.

3. **PipelineKind variant sizes**
   - What we know: All pipelines hold Session + Tokenizer + profile-specific metadata
   - What's unclear: Whether size difference between variants triggers clippy large_enum_variant
   - Recommendation: Add `#[cfg(test)] assert!(std::mem::size_of::<PipelineKind>() < threshold)` and Box if needed

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| Rust toolchain | All code | Yes | 1.97.1 (2024 edition) | -- |
| cargo | Build | Yes | 1.97.1 | -- |
| tokio sync feature | Batcher (mpsc, oneshot) | Yes (transitive via tokio-util) | 1.53.1 | Add "sync" to workspace features explicitly |
| tokio time feature | Batcher (sleep for max_wait) | Yes (transitive via tower-http) | 1.53.1 | Add "time" to workspace features explicitly |
| ndarray | Batch tensor construction | Yes (workspace dep) | 0.17 | -- |

**Missing dependencies with no fallback:** none
**Missing dependencies with fallback:** none

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in test framework + mockall 0.15 |
| Config file | None needed (Rust convention: inline `#[cfg(test)]` + `tests/` directory) |
| Quick run command | `cargo test --lib` |
| Full suite command | `cargo test` |

### Phase Requirements -> Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| PROF-02 | Embeddings pipeline: tokenize, infer, mean pool, L2 normalize, return float vector | unit | `cargo test -p hephaestus-core embeddings` | No -- Wave 0 |
| PROF-03 | Seq2Seq pipeline: tokenize, infer, decode output tokens, return text | unit | `cargo test -p hephaestus-core seq2seq` | No -- Wave 0 |
| PROF-04 | Token classifier pipeline: tokenize, infer, merge BIO spans, return entities | unit | `cargo test -p hephaestus-core token_classifier` | No -- Wave 0 |
| BTCH-01 | Dynamic batching collects requests and runs single inference call | unit + integration | `cargo test -p hephaestus-api batcher` | No -- Wave 0 |
| BTCH-02 | Batching disabled by default, bypasses batcher entirely | unit | `cargo test -p hephaestus config::tests::batch_disabled_by_default` | No -- Wave 0 |
| BTCH-03 | Batch config: max_batch_size and max_wait_time configurable | unit | `cargo test -p hephaestus config::tests::batch_config` | No -- Wave 0 |
| D-01 | Profile detection from config.json architectures field | unit | `cargo test -p hephaestus-core profile` | No -- Wave 0 |
| D-03 | PipelineKind enum dispatch works for all profiles | unit | `cargo test -p hephaestus-core pipeline_kind` | No -- Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test --lib`
- **Per wave merge:** `cargo test`
- **Phase gate:** Full suite green before `/gsd-verify-work`

### Wave 0 Gaps
- [ ] Unit tests for `postprocess::mean_pool()` and `postprocess::l2_normalize()`
- [ ] Unit tests for `profile::detect_profile()` with various config.json fixtures
- [ ] Unit tests for `PipelineKind::prepare()` and `PipelineKind::execute()` dispatch via mockall
- [ ] Unit tests for batcher collection logic (max_size trigger, max_wait trigger, zero-overhead when disabled)
- [ ] Unit tests for config extensions (batch defaults, model_profile parsing)
- [ ] Unit tests for BIO tag merging in postprocess

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | No | Internal service, no auth |
| V3 Session Management | No | Stateless request handling |
| V4 Access Control | No | Single-tenant internal service |
| V5 Input Validation | Yes | Text input validation (empty check, truncation to 512 tokens), batch size bounded by config |
| V6 Cryptography | No | No crypto operations |

### Known Threat Patterns for This Phase

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Batch size DoS (BATCH_MAX_SIZE set too high) | Denial of Service | Config validation at startup: reject max_size > 64 (or sensible upper bound). Bounded mpsc channel prevents unbounded queue growth. |
| Memory exhaustion from large batches | Denial of Service | Bounded mpsc channel (capacity = small multiple of max_batch_size). Padding to max_seq_len within batch is bounded by tokenizer truncation (512 tokens). |
| Input text DoS via long strings | Denial of Service | Already mitigated: tokenizer truncation to 512 tokens (existing from Phase 1). |
| Oneshot channel leak on panic | Denial of Service | Ensure batcher sends Err to all pending oneshot channels on any failure. Wrap batch execution in catch_unwind or structured error handling. |

## Sources

### Primary (HIGH confidence)
- Codebase inspection: `crates/hephaestus-core/src/pipeline.rs`, `postprocess.rs`, `error.rs` -- existing Pipeline trait, PreparedInput, ClassifierPipeline [VERIFIED: codebase grep]
- Codebase inspection: `crates/hephaestus-api/src/state.rs`, `handlers.rs`, `metrics.rs` -- existing AppState, handler patterns, StageTimer [VERIFIED: codebase grep]
- Codebase inspection: `crates/hephaestus/src/config.rs`, `main.rs` -- existing Config with envy, startup flow [VERIFIED: codebase grep]
- Codebase inspection: `Cargo.toml` -- workspace dependencies, no new crates needed [VERIFIED: codebase grep]
- Codebase inspection: `rules/async-mpsc-queue.md`, `async-oneshot-response.md`, `async-bounded-channel.md`, `anti-lock-across-await.md`, `mem-box-large-variant.md` -- channel patterns and constraints [VERIFIED: codebase grep]
- Phase 1 CONTEXT.md D-04: Pipeline two-step API explicitly designed for batching [VERIFIED: codebase grep]
- Phase 4 CONTEXT.md: All locked decisions (D-01 through D-10) [VERIFIED: codebase grep]

### Secondary (MEDIUM confidence)
- [HuggingFace Configuration docs](https://huggingface.co/docs/transformers/main_classes/configuration) -- architectures field definition, is_encoder_decoder, id2label [CITED]
- [Sentence Transformers modules docs](https://sbert.net/docs/package_reference/sentence_transformer/modules.html) -- Pooling and Normalize module reference [CITED]
- [HuggingFace all-MiniLM-L6-v2 discussion #67](https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/discussions/67) -- mean pooling + L2 norm code for raw ONNX models [CITED]
- tokio feature tree verification via `cargo tree -e features` -- sync and time features available transitively [VERIFIED: codebase grep]

### Tertiary (LOW confidence)
- WebSearch: dynamic batching patterns in Rust inference servers [ASSUMED]
- WebSearch: BIO tagging NER post-processing for subword tokens [ASSUMED]
- Training knowledge: ort Session::run batch dimension behavior [ASSUMED]
- Training knowledge: tokenizers crate decode() and word_ids() API [ASSUMED]
- Training knowledge: seq2seq fused ONNX model output format [ASSUMED]

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- no new dependencies, all crates verified in workspace
- Architecture: HIGH -- PipelineKind enum dispatch, batcher pattern well-defined by locked decisions
- Post-processing (embeddings): MEDIUM -- mean pooling + L2 norm formulas cited from official docs
- Post-processing (NER): LOW -- BIO tag merging based on training knowledge, not verified from Rust crate docs
- Post-processing (seq2seq): LOW -- fused model output format is assumed, needs verification with real model
- Batching mechanics: MEDIUM -- channel pattern well-supported by project rules, tensor padding logic is standard
- Profile detection: MEDIUM -- architectures field documented by HuggingFace, but exact suffix patterns assumed

**Research date:** 2026-08-26
**Valid until:** 2026-09-25 (30 days -- stable domain, no fast-moving APIs)
