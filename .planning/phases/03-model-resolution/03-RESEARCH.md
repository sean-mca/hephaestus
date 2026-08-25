# Phase 3: Model Resolution - Research

**Researched:** 2026-08-24
**Domain:** Model file acquisition and caching (S3, HuggingFace, Forge stub)
**Confidence:** HIGH

## Summary

Phase 3 implements the 3-tier model resolution chain inside the `hephaestus-resolve` crate. A single `resolve()` call checks S3 cache first, falls back to HuggingFace ONNX downloads, and finally attempts Forge conversion (stub in this phase). After resolution, the caller receives a local directory path that `ClassifierPipeline::new()` can consume directly. The resolver also uploads newly-resolved models back to S3 in the background for future pods.

The crate introduces three new workspace dependencies: `aws-sdk-s3` (S3 operations), `aws-config` (credential resolution), and `reqwest` (HTTP client for the Forge stub). The existing `hf-hub` dependency (already in workspace, currently dev-only in `hephaestus-core`) becomes a regular dependency for `hephaestus-resolve`. All three new crates are well-established (OK legitimacy verdicts, millions of weekly downloads, official maintainers).

**Primary recommendation:** Build the resolver as a single deep-module struct (`ModelResolver`) with `new()` and `resolve()` methods. Internally, define `pub(crate)` traits for S3 and HF operations to enable unit testing with `mockall`. The `ForgeClient` trait is public per D-10 for Phase 5 to implement.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- **D-01:** Flat model ID prefix in S3: `s3://{bucket}/{model_id}/model.onnx`, `tokenizer.json`, `config.json`. Model IDs with org namespaces (e.g., `sentence-transformers/all-MiniLM-L6-v2`) preserve slashes as S3 path segments.
- **D-02:** Full model directory cached to S3 -- everything `ClassifierPipeline::new()` needs (model.onnx or onnx/model.onnx, tokenizer.json, config.json). S3 cache hit requires zero HuggingFace contact.
- **D-03:** S3 bucket configured via `S3_BUCKET` env var, following the existing envy config pattern (Phase 1, D-11). Optional -- when unset, S3 tier is skipped and resolution starts at HuggingFace.
- **D-04:** When HuggingFace has the model but no ONNX export, fail with a clear error message: "model X has no ONNX export and Forge is not configured". Consistent with D-13 (fail hard on missing requirements). Do not silently fall through to the Forge stub.
- **D-05:** Retry within each tier (2-3 attempts with exponential backoff) before moving to the next tier or failing. Prevents transient S3 blips from triggering unnecessary HuggingFace downloads.
- **D-06:** Downloads go to a temporary directory first, then atomically renamed to the final local cache path. Prevents serving partially-downloaded models if the pod crashes mid-download.
- **D-07:** Local model cache uses the HuggingFace cache directory (`~/.cache/huggingface` or `HF_HOME`). Shares cache with `hf-hub`'s built-in caching. Integration tests already use this path.
- **D-08:** Forge client uses HTTP REST via `reqwest`. Simple POST with model ID, Forge returns S3 paths of converted files. Cross-language compatibility with the Python Forge service. Adds `reqwest` to workspace dependencies.
- **D-09:** Forge configured via optional `FORGE_URL` env var. When set, the third tier (Forge conversion) is active. When unset, resolution stops at tier 2 (HuggingFace) and fails if no ONNX export exists.
- **D-10:** Define a `ForgeClient` trait in `hephaestus-resolve` with a single `convert()` method. Phase 3 ships a stub implementation that returns "Forge unavailable". Phase 5 provides the real HTTP implementation. Testable with mockall.
- **D-11:** Forge conversion request sends only the model ID: `POST {"model_id": "org/model"}`. Forge handles downloading PyTorch weights, converting, uploading to S3, and returning the S3 paths.
- **D-12:** Background async upload to S3 after the pod starts serving. Download from HF -> load model -> start serving -> upload to S3 in a background tokio task. Faster pod startup; if the pod crashes before upload completes, next pod downloads from HF again.
- **D-13:** Upload unconditionally -- no HeadObject check before uploading. S3 PutObject is idempotent. Avoids the extra API call. Worst case on concurrent pod starts is redundant uploads, not corruption.
- **D-14:** Retry S3 upload with exponential backoff (2-3 attempts) on failure. On final failure, log a warning and continue serving. Upload failure has no impact on the running pod's inference capability.

### Claude's Discretion
No areas deferred to Claude's discretion -- all decisions made explicitly.

### Deferred Ideas (OUT OF SCOPE)
None -- discussion stayed within phase scope.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| RSLV-01 | Runtime checks S3 cache for ONNX model files and loads from S3 if present | aws-sdk-s3 get_object pattern, S3 cache layout (D-01/D-02), credential resolution via aws-config IRSA |
| RSLV-02 | On S3 miss, runtime checks HuggingFace for existing ONNX exports via hf-hub and downloads if available | hf-hub 1.0 API (HFClient, download_file, file_exists), HFError::EntryNotFound for missing ONNX detection |
| RSLV-03 | On HuggingFace miss, runtime calls the Forge service to convert the model to ONNX | ForgeClient trait with convert() method (D-10), reqwest for HTTP POST, stub implementation for Phase 3 |
| RSLV-04 | After downloading from HF or Forge, runtime uploads ONNX files back to S3 for future pods | Background tokio::spawn, ByteStream::from_path for upload, exponential backoff retry |
| RSLV-05 | Model resolution exposes a single resolve() method that abstracts the 3-tier chain (Ousterhout deep module pattern) | ModelResolver struct with resolve(&self, model_id) -> Result<PathBuf>, internal tier methods hidden |
</phase_requirements>

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| S3 model cache check + download | API / Backend | -- | S3 operations are server-side, credentials via IRSA |
| HuggingFace model download | API / Backend | -- | Network download at pod startup, not client-initiated |
| Forge conversion request | API / Backend | -- | Service-to-service HTTP call |
| S3 cache-back upload | API / Backend | -- | Background async task, fire-and-forget |
| Local cache management | API / Backend | -- | Filesystem operations on pod-local storage |
| Config extension (env vars) | API / Backend | -- | envy deserialization at startup |

## Standard Stack

### Core (New Dependencies for Phase 3)

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| aws-sdk-s3 | 1.143.0 | S3 get/put operations | Official AWS SDK for Rust. Handles IRSA credentials natively. 1.4M weekly downloads. [CITED: docs.rs/aws-sdk-s3] |
| aws-config | 1.11.0 | AWS credential resolution | Official AWS config loader. Default provider chain handles env vars, IMDS, IRSA for k8s. 2.2M weekly downloads. [CITED: docs.rs/aws-config] |
| reqwest | 0.13.4 | HTTP client for Forge | De facto Rust HTTP client. 13M weekly downloads. Uses hyper 1.x (compatible with axum 0.8). [CITED: docs.rs/reqwest] |

### Existing (Already in Workspace)

| Library | Version | Purpose | Phase 3 Role |
|---------|---------|---------|-------------|
| hf-hub | 1.0.0 | HuggingFace model downloads | Tier 2: download ONNX files from HF. Currently dev-only in hephaestus-core; becomes regular dep in hephaestus-resolve. [CITED: docs.rs/hf-hub/1.0.0] |
| tokio | 1.x | Async runtime | tokio::spawn for background S3 upload, tokio::time::sleep for backoff |
| thiserror | 2.0 | Error types | ResolveError enum for the resolve crate |
| tracing | 0.1 | Instrumentation | Structured logging in resolution flow |
| tempfile | 3.x | Temp directories | Atomic download: tempdir_in() -> rename pattern (D-06) |
| serde / serde_json | 1.0 | Serialization | Forge request/response JSON |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| reqwest | hyper (direct) | reqwest is higher-level, handles JSON, timeouts, TLS out of the box. Direct hyper is too low-level for a simple POST. |
| aws-sdk-s3 | rust-s3 (community) | aws-sdk-s3 is official, handles IRSA/IMDS natively. rust-s3 has less maintenance. |
| Manual retry | backon crate | Manual retry with tokio::time::sleep is 10 lines of code for 2-3 attempts. backon adds a dependency for trivial logic. |

**Installation (workspace Cargo.toml additions):**
```toml
# Model acquisition (Phase 3)
aws-sdk-s3 = "1.143"
aws-config = { version = "1.11", features = ["behavior-version-latest"] }
reqwest = { version = "0.13", features = ["json"] }
```

**Version verification:**
- `aws-sdk-s3`: 1.143.0 confirmed via `cargo search` [VERIFIED: crates.io registry]
- `aws-config`: 1.11.0 confirmed via `cargo search` [VERIFIED: crates.io registry]
- `reqwest`: 0.13.4 confirmed via `cargo search` [VERIFIED: crates.io registry]
- `hf-hub`: 1.0.0 already in workspace, confirmed via `cargo search` [VERIFIED: crates.io registry]

**Note on reqwest version:** The CLAUDE.md recommended stack listed reqwest 0.12, but the current latest is 0.13.4 (released 2026-07-28). The 0.13 release updates to hyper 1.x and is compatible with the existing axum 0.8 / tower 0.5 stack. Use 0.13. [CITED: crates.io/crates/reqwest]

## Package Legitimacy Audit

| Package | Registry | Age | Downloads | Source Repo | Verdict | Disposition |
|---------|----------|-----|-----------|-------------|---------|-------------|
| aws-sdk-s3 | crates.io | 5+ yrs | 1.4M/wk | github.com/awslabs/aws-sdk-rust | OK | Approved |
| aws-config | crates.io | 5+ yrs | 2.2M/wk | github.com/smithy-lang/smithy-rs | OK | Approved |
| reqwest | crates.io | 10+ yrs | 13M/wk | github.com/seanmonstar/reqwest | OK | Approved |
| hf-hub | crates.io | 3+ yrs | 555K/wk | github.com/huggingface/hf-hub | OK | Approved |

**Packages removed due to [SLOP] verdict:** none
**Packages flagged as suspicious [SUS]:** none

## Architecture Patterns

### System Architecture Diagram

```
                        MODEL_ID (env var)
                              |
                              v
                    +-------------------+
                    | ModelResolver      |
                    | resolve(model_id)  |
                    +-------------------+
                              |
              +---------------+---------------+
              |               |               |
              v               v               v
        +-----------+   +-----------+   +-----------+
        | S3 Tier   |   | HF Tier   |   | Forge Tier|
        | (cache)   |   | (source)  |   | (convert) |
        +-----------+   +-----------+   +-----------+
              |               |               |
              | get_object    | download_file | POST /convert
              | (with retry)  | (with retry)  | (stub: error)
              v               v               v
        +-----------+   +-----------+   +-----------+
        | S3 Bucket |   | HF Hub    |   | Forge API |
        +-----------+   +-----------+   +-----------+
              |               |
              |    +----------+
              |    |
              v    v
        +----------------+
        | Local Cache Dir|   <-- tempdir + atomic rename
        | (PathBuf)      |
        +----------------+
              |
              v
        +---------------------+
        | ClassifierPipeline   |
        | ::new(model_dir)     |
        +---------------------+

        Background: HF download success
              |
              v
        tokio::spawn --> S3 put_object (cache-back)
                         (fire-and-forget, retry, log on failure)
```

### Recommended Project Structure

```
crates/hephaestus-resolve/
├── Cargo.toml          # Dependencies: aws-sdk-s3, aws-config, hf-hub, reqwest, tokio, etc.
└── src/
    ├── lib.rs          # Public API: ModelResolver, ResolveError, ForgeClient trait, re-exports
    ├── error.rs        # ResolveError enum (thiserror)
    ├── resolver.rs     # ModelResolver struct, resolve() orchestration, tier fallback logic
    ├── s3.rs           # S3 download/upload operations, retry logic
    ├── hf.rs           # HuggingFace download operations, ONNX file detection
    └── forge.rs        # ForgeClient trait, StubForgeClient, convert() method
```

### Pattern 1: Deep Module Resolver

**What:** Single `resolve()` method hiding the 3-tier fallback chain, retry logic, caching, and atomic file operations.
**When to use:** Any time a caller needs model files -- they call `resolve()` and get a path.

```rust
// Source: CONTEXT.md D-10, RSLV-05
pub struct ModelResolver {
    s3_client: Option<aws_sdk_s3::Client>,
    s3_bucket: Option<String>,
    hf_client: hf_hub::HFClient,
    forge: Box<dyn ForgeClient>,
    cache_dir: PathBuf,
}

impl ModelResolver {
    /// Resolve a model ID to a local directory containing ONNX files.
    ///
    /// Checks S3 cache first, then HuggingFace, then Forge conversion.
    /// Returns the path to a directory containing model.onnx (or onnx/model.onnx),
    /// tokenizer.json, and config.json.
    pub async fn resolve(&self, model_id: &str) -> Result<PathBuf, ResolveError> {
        // Tier 1: S3 cache
        if let Some(path) = self.try_s3(model_id).await? {
            tracing::info!(model_id, "resolved from S3 cache");
            return Ok(path);
        }

        // Tier 2: HuggingFace
        if let Some(path) = self.try_hf(model_id).await? {
            tracing::info!(model_id, "resolved from HuggingFace");
            self.spawn_cache_back(model_id, &path);
            return Ok(path);
        }

        // Tier 3: Forge conversion
        self.try_forge(model_id).await
    }
}
```

### Pattern 2: Atomic Download with Temp Directory

**What:** Download to temp directory, then atomically rename to final path. Prevents serving partially-downloaded models.
**When to use:** S3 downloads (D-06). HF downloads use hf-hub's built-in atomic caching.

```rust
// Source: tempfile docs, D-06
use tempfile::TempDir;

async fn download_to_cache(
    &self,
    model_id: &str,
    files: &[(&str, bytes::Bytes)],
) -> Result<PathBuf, ResolveError> {
    let final_dir = self.cache_dir.join(model_id);

    // Already cached locally
    if final_dir.exists() {
        return Ok(final_dir);
    }

    // Create temp dir on SAME filesystem as final_dir for atomic rename
    if let Some(parent) = final_dir.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let temp_dir = TempDir::new_in(
        final_dir.parent().unwrap_or(&self.cache_dir),
    )?;

    // Write files to temp dir
    for (filename, data) in files {
        let file_path = temp_dir.path().join(filename);
        if let Some(parent) = file_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&file_path, data).await?;
    }

    // Atomic rename (same filesystem guarantees atomicity)
    tokio::fs::rename(temp_dir.path(), &final_dir).await?;
    // Prevent TempDir destructor from removing the directory
    let _ = temp_dir.into_path();

    Ok(final_dir)
}
```

### Pattern 3: Background Cache-Back with Fire-and-Forget

**What:** After resolving from HF, upload to S3 in a background task. Pod starts serving immediately.
**When to use:** D-12 -- after any successful HF download.

```rust
// Source: D-12, D-13, D-14
fn spawn_cache_back(&self, model_id: &str, local_dir: &Path) {
    let Some(s3_client) = self.s3_client.clone() else { return };
    let Some(bucket) = self.s3_bucket.clone() else { return };
    let model_id = model_id.to_string();
    let local_dir = local_dir.to_path_buf();

    tokio::spawn(async move {
        if let Err(e) = upload_to_s3_with_retry(
            &s3_client, &bucket, &model_id, &local_dir,
        ).await {
            // D-14: log warning and continue -- upload failure is non-fatal
            tracing::warn!(
                model_id,
                error = %e,
                "failed to cache model to S3 after retries"
            );
        }
    });
}
```

### Pattern 4: Exponential Backoff Retry

**What:** Retry transient failures 2-3 times with exponential backoff before failing or moving to next tier.
**When to use:** D-05 (tier retries) and D-14 (cache-back retries).

```rust
// Source: D-05, D-14
async fn with_retry<F, Fut, T, E>(
    max_attempts: u32,
    base_delay: Duration,
    mut operation: F,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    let mut attempt = 0;
    loop {
        attempt += 1;
        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) if attempt >= max_attempts => return Err(e),
            Err(e) => {
                let delay = base_delay * 2u32.pow(attempt - 1);
                tracing::warn!(
                    attempt,
                    max_attempts,
                    delay_ms = delay.as_millis() as u64,
                    error = %e,
                    "retrying after transient error"
                );
                tokio::time::sleep(delay).await;
            }
        }
    }
}
```

### Pattern 5: ForgeClient Trait with Stub

**What:** Public trait for Forge conversion with a stub implementation that returns an error.
**When to use:** D-10 -- Phase 3 ships the stub, Phase 5 ships the real impl.

```rust
// Source: D-10, D-11
/// Forge service client for converting models to ONNX format.
///
/// Phase 3 ships a stub implementation. Phase 5 provides the real
/// HTTP client that calls the Python Forge service.
#[cfg_attr(test, mockall::automock)]
pub trait ForgeClient: Send + Sync {
    /// Request model conversion to ONNX format.
    ///
    /// Returns the S3 paths of the converted model files.
    fn convert(
        &self,
        model_id: &str,
    ) -> impl std::future::Future<Output = Result<Vec<String>, ResolveError>> + Send;
}

/// Stub Forge client that always returns an unavailable error.
pub struct StubForgeClient;

impl ForgeClient for StubForgeClient {
    async fn convert(&self, model_id: &str) -> Result<Vec<String>, ResolveError> {
        Err(ResolveError::ForgeUnavailable {
            model_id: model_id.to_string(),
        })
    }
}
```

### Anti-Patterns to Avoid

- **Leaking tier internals to callers:** The resolver must hide S3/HF/Forge details. Callers should never know which tier resolved the model. Only `resolve()` is public.
- **Blocking the async runtime with file I/O:** Use `tokio::fs` for all file operations in async code, or `spawn_blocking` for CPU-intensive work (rule: async-spawn-blocking). Model file reads by ort happen synchronously in `ClassifierPipeline::new()`, which is acceptable because it runs once at startup.
- **Cross-filesystem rename:** `std::fs::rename` / `tokio::fs::rename` fails across filesystem boundaries. Always create the temp directory on the SAME filesystem as the final destination using `TempDir::new_in(parent_dir)`.
- **Holding the temp directory reference after rename:** After `tokio::fs::rename(temp_dir.path(), final_path)`, call `temp_dir.into_path()` to prevent the `TempDir` destructor from deleting the now-moved directory.
- **Ignoring HuggingFace `onnx/` subdirectory convention:** Some models store ONNX files in `onnx/model.onnx`, others at `model.onnx`. The resolver must check both locations. `ClassifierPipeline::new()` already handles both.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| AWS credential resolution | Manual env var parsing for AWS keys | `aws-config` default provider chain | Handles IRSA, IMDS, env vars, SSO, ECS creds. Gets it right for k8s without any custom code. |
| HuggingFace file download | Manual HTTP + cache management | `hf-hub` crate | Content-addressed caching, ETag-based deduplication, on-disk locking for concurrent downloads, auth token handling. |
| S3 operations | Manual HTTP to S3 REST API | `aws-sdk-s3` | Handles SigV4 signing, chunked uploads, error parsing. |
| HTTP POST with JSON | Manual hyper request building | `reqwest` | Handles TLS, JSON serialization, timeouts, connection pooling. |
| Temporary directory lifecycle | Manual mkdtemp + cleanup | `tempfile` crate | RAII cleanup, `new_in()` for same-filesystem guarantees, `into_path()` to prevent cleanup after rename. |

**Key insight:** Every external service interaction (S3, HuggingFace, Forge) has an official or well-established Rust client. The resolver's value is orchestrating the 3-tier fallback chain and cache management, not reimplementing HTTP or AWS auth.

## Common Pitfalls

### Pitfall 1: Cross-Filesystem Rename Failure
**What goes wrong:** `std::fs::rename(temp_path, final_path)` returns `EXDEV` (cross-device link) when temp dir and final dir are on different filesystems.
**Why it happens:** Default `tempfile::tempdir()` creates in `/tmp` which may be a different mount than the model cache directory.
**How to avoid:** Use `tempfile::TempDir::new_in(cache_dir_parent)` to ensure same filesystem.
**Warning signs:** `Os { code: 18, kind: CrossesDevices }` error on Linux.

### Pitfall 2: TempDir Destructor Removes Renamed Directory
**What goes wrong:** After `rename(temp.path(), final_path)`, the `TempDir` goes out of scope and its destructor tries to remove `temp.path()` -- which now points to the final directory.
**Why it happens:** `TempDir` implements `Drop` which removes the directory.
**How to avoid:** Call `temp_dir.into_path()` after a successful rename. This consumes the `TempDir` without running the destructor.
**Warning signs:** Model files disappear immediately after download.

### Pitfall 3: aws-config Behavior Version Not Set
**What goes wrong:** `aws_config::load_from_env().await` may use old default behavior without the `behavior-version-latest` feature.
**Why it happens:** AWS SDK Rust requires explicit behavior version to maintain backward compatibility.
**How to avoid:** Add `features = ["behavior-version-latest"]` to `aws-config` in Cargo.toml. Or use `aws_config::load_defaults(BehaviorVersion::latest()).await`. [CITED: docs.rs/aws-config]
**Warning signs:** Deprecation warnings at compile time, unexpected default region behavior.

### Pitfall 4: HuggingFace Model Without ONNX Export
**What goes wrong:** The resolver tries to download `onnx/model.onnx` from a HuggingFace repo that only has PyTorch weights. If not handled, it falls through silently or panics.
**Why it happens:** Most HuggingFace models don't have pre-exported ONNX files.
**How to avoid:** Check for `onnx/model.onnx` first, then `model.onnx`. If neither exists, return `ResolveError::NoOnnxExport` with a clear message (D-04). Use `hf_hub::HFError::EntryNotFound` to detect this. [CITED: docs.rs/hf-hub/1.0.0/hf_hub/enum.HFError.html]
**Warning signs:** `EntryNotFound` error from hf-hub.

### Pitfall 5: S3 Upload Blocking Pod Startup
**What goes wrong:** Waiting for S3 upload before serving traffic causes slow pod startup (models can be hundreds of MB).
**Why it happens:** Upload is in the critical path.
**How to avoid:** D-12 is explicit: spawn the upload as a background tokio task. The pod starts serving immediately after HF download. Use `tokio::spawn` (not `.await` on the upload). [ASSUMED]
**Warning signs:** Slow pod startup times, k8s readiness probe timeouts.

### Pitfall 6: HF Cache Sharing with hf-hub Owner/Name API
**What goes wrong:** The `HFClient.model(owner, name)` API takes separate owner and name, but `MODEL_ID` may be a combined string like `"Xenova/distilbert-base-uncased-finetuned-sst-2-english"`. Splitting incorrectly causes wrong repo lookups.
**Why it happens:** Some model IDs have no owner prefix (e.g., `"bert-base-uncased"`), others have org prefixes with slashes.
**How to avoid:** Split on the first `/` only. If no `/` is present, the model has no owner prefix -- use the model name directly (hf-hub may treat it as a user-owned model or require special handling). The existing integration test uses `client.model("Xenova", "distilbert-base-uncased-finetuned-sst-2-english")` as separate args.
**Warning signs:** `RepoNotFound` errors for models that exist on HuggingFace.

### Pitfall 7: reqwest Version Mismatch with Existing Stack
**What goes wrong:** Using reqwest 0.12 when the rest of the stack (axum 0.8) uses hyper 1.x causes duplicate hyper versions in the dependency tree.
**Why it happens:** reqwest 0.12 used hyper 0.14 internally.
**How to avoid:** Use reqwest 0.13.4 which uses hyper 1.x, aligning with the rest of the stack. [CITED: crates.io/crates/reqwest]
**Warning signs:** Multiple `hyper` versions in `cargo tree`, increased binary size.

## Code Examples

### S3 Download with Retry

```rust
// Source: aws-sdk-s3 docs.rs, D-05
use aws_sdk_s3::Client as S3Client;

async fn download_s3_file(
    client: &S3Client,
    bucket: &str,
    key: &str,
) -> Result<bytes::Bytes, ResolveError> {
    let resp = client
        .get_object()
        .bucket(bucket)
        .key(key)
        .send()
        .await
        .map_err(|e| ResolveError::S3(format!("get_object failed for {key}: {e}")))?;

    let body = resp
        .body
        .collect()
        .await
        .map_err(|e| ResolveError::S3(format!("failed to read body for {key}: {e}")))?;

    Ok(body.into_bytes())
}
```

### S3 Upload from File

```rust
// Source: aws-sdk-s3 docs.rs, D-13
use aws_sdk_s3::primitives::ByteStream;

async fn upload_to_s3(
    client: &S3Client,
    bucket: &str,
    key: &str,
    file_path: &Path,
) -> Result<(), ResolveError> {
    let body = ByteStream::from_path(file_path)
        .await
        .map_err(|e| ResolveError::S3(format!("failed to read {}: {e}", file_path.display())))?;

    client
        .put_object()
        .bucket(bucket)
        .key(key)
        .body(body)
        .send()
        .await
        .map_err(|e| ResolveError::S3(format!("put_object failed for {key}: {e}")))?;

    Ok(())
}
```

### HuggingFace Download with ONNX Detection

```rust
// Source: hf-hub 1.0.0 docs.rs, D-04
use hf_hub::{HFClient, HFError};

async fn download_from_hf(
    client: &HFClient,
    model_id: &str,
) -> Result<Option<PathBuf>, ResolveError> {
    let (owner, name) = split_model_id(model_id);
    let repo = client.model(&owner, &name);

    // Check if repo exists
    if !repo.exists().await.map_err(|e| ResolveError::HuggingFace(e.to_string()))? {
        return Ok(None);
    }

    // Try downloading ONNX model file (check onnx/ subdir first, then root)
    let onnx_path = match repo.download_file().filename("onnx/model.onnx").send().await {
        Ok(path) => path,
        Err(HFError::EntryNotFound { .. }) => {
            // Try flat layout
            match repo.download_file().filename("model.onnx").send().await {
                Ok(path) => path,
                Err(HFError::EntryNotFound { .. }) => {
                    // No ONNX export available (D-04)
                    return Err(ResolveError::NoOnnxExport {
                        model_id: model_id.to_string(),
                    });
                }
                Err(e) => return Err(ResolveError::HuggingFace(e.to_string())),
            }
        }
        Err(e) => return Err(ResolveError::HuggingFace(e.to_string())),
    };

    // Download supporting files
    let _tokenizer = repo.download_file().filename("tokenizer.json").send().await
        .map_err(|e| ResolveError::HuggingFace(
            format!("failed to download tokenizer.json: {e}")
        ))?;

    let _config = repo.download_file().filename("config.json").send().await
        .map_err(|e| ResolveError::HuggingFace(
            format!("failed to download config.json: {e}")
        ))?;

    // Navigate to snapshot root from the ONNX file path
    // onnx_path is {snapshot_root}/onnx/model.onnx or {snapshot_root}/model.onnx
    let snapshot_root = if onnx_path.parent().and_then(|p| p.file_name())
        == Some(std::ffi::OsStr::new("onnx"))
    {
        onnx_path.parent().unwrap().parent().unwrap().to_path_buf()
    } else {
        onnx_path.parent().unwrap().to_path_buf()
    };

    Ok(Some(snapshot_root))
}

fn split_model_id(model_id: &str) -> (String, String) {
    match model_id.split_once('/') {
        Some((owner, name)) => (owner.to_string(), name.to_string()),
        None => (model_id.to_string(), model_id.to_string()),
    }
}
```

### Config Extension

```rust
// Source: existing config.rs pattern, D-03, D-09
// Additions to the Config struct:

/// S3 bucket for model cache (optional, env `S3_BUCKET`).
/// When set, the resolver checks S3 before HuggingFace.
#[serde(default)]
pub s3_bucket: Option<String>,

/// S3 key prefix for model files (optional, env `S3_PREFIX`).
/// Prepended to model ID when constructing S3 keys.
#[serde(default)]
pub s3_prefix: Option<String>,

/// Forge service URL (optional, env `FORGE_URL`).
/// When set, enables the Forge conversion tier.
#[serde(default)]
pub forge_url: Option<String>,
```

### main.rs Integration Point

```rust
// Source: existing main.rs line 43, D-07
// Replace: let model_dir = config.model_dir()?;
// With:

// 3. Resolve model files (S3 -> HuggingFace -> Forge).
let resolver = hephaestus_resolve::ModelResolver::new(
    config.s3_bucket.as_deref(),
    config.s3_prefix.as_deref(),
    config.forge_url.as_deref(),
).await?;

let model_dir = resolver.resolve(&config.model_id).await?;
tracing::info!(model_id = %config.model_id, model_dir = %model_dir.display(), "model resolved");
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| hf-hub 0.3 `Api::new()` | hf-hub 1.0 `HFClient::new()` | 2024 | API renamed; `Api` -> `HFClient`, `ApiBuilder` -> `HFClientBuilder`. `download_file` is now async-first with builder chain. |
| reqwest 0.12 (hyper 0.14) | reqwest 0.13 (hyper 1.x) | 2026-07 | Aligns with axum 0.8 / tower 0.5 hyper version. No duplicate hyper in dep tree. |
| aws-config without behavior-version | `behavior-version-latest` feature | 2024 | Required to avoid deprecation warnings and get latest SDK defaults. |

**Deprecated/outdated:**
- hf-hub `Api` / `ApiRepo` types: replaced by `HFClient` / `HFRepository` in 1.0
- reqwest 0.12: superseded by 0.13 with hyper 1.x alignment
- aws-config without `behavior-version-latest`: emits deprecation warnings

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | reqwest 0.13 is compatible with the existing axum 0.8 / tonic 0.14 stack (both use hyper 1.x) | Standard Stack | Dependency conflict at compile time; fallback: pin reqwest 0.12 and accept duplicate hyper |
| A2 | hf-hub 1.0 `HFClient.model(owner, name)` API takes separate owner/name strings, not a combined model ID | Code Examples | Wrong split logic; existing integration test confirms the pattern |
| A3 | `tokio::fs::rename` across same-filesystem directories is atomic on Linux and macOS | Architecture Patterns | Partially-written model dirs could be served; mitigated by tempdir_in same fs |
| A4 | Background tokio::spawn for S3 upload does not interfere with graceful shutdown | Architecture Patterns | Upload may be interrupted; D-12 explicitly accepts this tradeoff |
| A5 | aws-sdk-s3 1.143 uses `body.collect().await?.into_bytes()` pattern for download | Code Examples | API changed; fallback: check docs.rs for current API |

## Open Questions

1. **Model ID splitting for hf-hub**
   - What we know: `HFClient.model(owner, name)` takes separate strings. The existing test uses `client.model("Xenova", "distilbert-base-uncased-finetuned-sst-2-english")`.
   - What's unclear: How to handle model IDs without an owner prefix (e.g., `"bert-base-uncased"`). Some HF models live under user accounts, not organizations.
   - Recommendation: Split on first `/`. If no `/`, treat the entire string as both owner and name. Validate with the test model. If hf-hub rejects this, check for a single-argument model ID method.

2. **S3 cache directory vs HF cache directory separation**
   - What we know: D-07 says "Local model cache uses the HuggingFace cache directory." For HF downloads, this is natural (hf-hub caches there). For S3 downloads, we need a local landing directory.
   - What's unclear: Should S3 downloads go into the HF cache structure, or a separate `{HF_HOME}/hephaestus/` directory?
   - Recommendation: Use a separate subdirectory `{HF_HOME}/hephaestus/s3-cache/{model_id}/` for S3 downloads. This avoids conflicting with hf-hub's content-addressed blob layout while still being under the same parent. The resolver returns whichever path has the files.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| Rust toolchain | Compilation | Yes | 1.97.1 | -- |
| cargo | Build | Yes | 1.97.1 | -- |
| S3 bucket | RSLV-01 (S3 cache) | Configurable | -- | S3_BUCKET unset -> skip S3 tier, start at HF |
| HuggingFace Hub | RSLV-02 (model download) | Yes (public API) | -- | HF_TOKEN for private repos (optional) |
| Forge service | RSLV-03 (conversion) | No (Phase 5) | -- | Stub returns error (by design) |
| Internet access | HF downloads | Required at startup | -- | Offline mode if local cache or S3 cache has the model |

**Missing dependencies with no fallback:**
- Internet access is required on first pod startup (cold cache). After first resolution, S3 cache eliminates HF dependency.

**Missing dependencies with fallback:**
- S3 bucket: when unset, resolution starts at HF tier
- Forge service: stub returns clear error (Phase 5 delivers real impl)

## Validation Architecture

### Test Framework

| Property | Value |
|----------|-------|
| Framework | Rust built-in test + tokio::test + mockall 0.15 |
| Config file | None needed (Rust built-in) |
| Quick run command | `cargo test -p hephaestus-resolve` |
| Full suite command | `cargo test --workspace` |

### Phase Requirements -> Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| RSLV-01 | S3 cache hit downloads and returns local path | unit (mock S3) | `cargo test -p hephaestus-resolve -- s3_cache_hit` | Wave 0 |
| RSLV-01 | S3 cache miss returns None, falls through | unit (mock S3) | `cargo test -p hephaestus-resolve -- s3_cache_miss` | Wave 0 |
| RSLV-02 | HF download succeeds for model with ONNX export | integration | `cargo test -p hephaestus-resolve --test resolve_e2e -- --ignored` | Wave 0 |
| RSLV-02 | HF returns NoOnnxExport error for model without ONNX | unit (mock HF) | `cargo test -p hephaestus-resolve -- hf_no_onnx` | Wave 0 |
| RSLV-03 | Forge stub returns ForgeUnavailable error | unit | `cargo test -p hephaestus-resolve -- forge_stub` | Wave 0 |
| RSLV-04 | Background cache-back spawns after HF download | unit (mock S3) | `cargo test -p hephaestus-resolve -- cache_back` | Wave 0 |
| RSLV-05 | resolve() returns PathBuf for successful resolution | unit (mock all tiers) | `cargo test -p hephaestus-resolve -- resolve_fallback` | Wave 0 |
| D-04 | Error message matches "model X has no ONNX export..." | unit | `cargo test -p hephaestus-resolve -- error_message_no_onnx` | Wave 0 |
| D-05 | Retry logic attempts 2-3 times before failing | unit | `cargo test -p hephaestus-resolve -- retry_logic` | Wave 0 |
| D-06 | Atomic temp dir + rename pattern | unit | `cargo test -p hephaestus-resolve -- atomic_download` | Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test -p hephaestus-resolve`
- **Per wave merge:** `cargo test --workspace`
- **Phase gate:** Full suite green before `/gsd-verify-work`

### Wave 0 Gaps
- [ ] `crates/hephaestus-resolve/src/error.rs` -- ResolveError types
- [ ] Unit tests in each module (s3, hf, forge, resolver)
- [ ] `crates/hephaestus-resolve/tests/resolve_e2e.rs` -- integration test with real HF download
- [ ] mockall setup for S3 and HF traits

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | No | Service-to-service auth via IRSA/IAM (aws-config) |
| V3 Session Management | No | Stateless service |
| V4 Access Control | No | Internal service, no user-facing auth |
| V5 Input Validation | Yes | MODEL_ID validated for path traversal and control characters |
| V6 Cryptography | No | S3 handles encryption at rest, HTTPS in transit |

### Known Threat Patterns for Rust + S3 + HuggingFace

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Path traversal via MODEL_ID | Tampering | Validate MODEL_ID contains only alphanumeric, `-`, `_`, `/`, `.` characters. Reject `..` components. |
| S3 key injection | Tampering | MODEL_ID is from env vars (operator-controlled), not user input. aws-sdk-s3 handles URL encoding. |
| Supply chain (malicious model) | Tampering | Out of scope for Phase 3 -- model integrity is a deployment concern. |
| Credential leakage in logs | Information Disclosure | Never log S3 credentials, HF tokens. aws-config and hf-hub handle secrets internally. |
| Disk exhaustion from large models | Denial of Service | K8s resource limits handle this. Not in scope for the resolver. |
| SSRF via FORGE_URL | Spoofing | FORGE_URL from env vars (operator-controlled). reqwest default redirect policy limits hops. |

## Project Constraints (from CLAUDE.md)

- **Language:** Rust only, 2024 edition, workspace resolver 3
- **Rules compliance:** All code must adhere to `rules/` directory
- **Deep module pattern:** Traits expose 1-3 methods hiding significant complexity. `resolve()` is the single public method.
- **Error handling:** thiserror for library errors (hephaestus-resolve), anyhow for application errors (hephaestus binary)
- **Config:** No Clap -- env vars only via envy (k8s service, per user feedback)
- **Workspace deps:** Central pinning in root Cargo.toml, crates use `dep.workspace = true`
- **GSD workflow:** All edits through GSD commands

## Sources

### Primary (HIGH confidence)
- Codebase analysis: `crates/hephaestus-resolve/`, `crates/hephaestus/src/config.rs`, `crates/hephaestus/src/main.rs`, `crates/hephaestus-core/src/pipeline.rs`, `crates/hephaestus-core/tests/classifier_e2e.rs` -- direct code inspection
- `crates.io` registry: version verification for all 4 packages via `cargo search`
- `03-CONTEXT.md`: 14 locked decisions (D-01 through D-14)
- `REQUIREMENTS.md`: RSLV-01 through RSLV-05 requirement definitions

### Secondary (MEDIUM confidence)
- [docs.rs/hf-hub/1.0.0](https://docs.rs/hf-hub/1.0.0) -- HFClient API, HFRepository methods, HFError variants
- [docs.rs/aws-sdk-s3](https://docs.rs/aws-sdk-s3) -- S3 client, get_object/put_object patterns
- [docs.rs/aws-config](https://docs.rs/aws-config) -- credential provider chain, behavior version
- [docs.rs/reqwest](https://docs.rs/reqwest) -- async client, JSON POST, version 0.13.4
- [docs.rs/tempfile](https://docs.rs/tempfile/latest/tempfile/struct.NamedTempFile.html) -- TempDir, new_in, persist, into_path
- [GitHub awslabs/aws-sdk-rust discussions/286](https://github.com/awslabs/aws-sdk-rust/discussions/286) -- S3 body download patterns

### Tertiary (LOW confidence)
- [WebSearch: atomic file operations in Rust](https://0xkiire.com/crash-consistency-fsync-rename/) -- fsync + rename crash consistency patterns
- [WebSearch: Rust S3 download retry](https://docs.rs/s3-filesystem) -- general S3 streaming patterns

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- all packages verified on crates.io, official maintainers, existing codebase patterns
- Architecture: HIGH -- 14 locked decisions from CONTEXT.md constrain the design; existing integration test confirms hf-hub API
- Pitfalls: MEDIUM -- based on docs.rs and community patterns, most verified via official documentation

**Research date:** 2026-08-24
**Valid until:** 2026-09-24 (30 days -- stable ecosystem, locked decisions)
