# Phase 6: OpenDAL Storage Abstraction - Pattern Map

**Mapped:** 2026-08-26
**Files analyzed:** 10
**Analogs found:** 10 / 10

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|-------------------|------|-----------|----------------|---------------|
| `crates/hephaestus-resolve/src/storage.rs` | service | file-I/O | `crates/hephaestus-resolve/src/s3.rs` | exact (replaces) |
| `crates/hephaestus-resolve/src/resolver.rs` | service | request-response | self (modify in place) | exact |
| `crates/hephaestus-resolve/src/error.rs` | model | transform | self (modify in place) | exact |
| `crates/hephaestus-resolve/src/lib.rs` | config | N/A | self (modify in place) | exact |
| `crates/hephaestus-resolve/Cargo.toml` | config | N/A | self (modify in place) | exact |
| `crates/hephaestus/src/config.rs` | config | transform | self (modify in place) | exact |
| `crates/hephaestus/src/main.rs` | controller | request-response | self (modify in place) | exact |
| `Cargo.toml` (workspace) | config | N/A | self (modify in place) | exact |
| `forge/src/forge/storage.py` | service | file-I/O | self (modify in place) | exact |
| `forge/src/forge/config.py` | config | transform | self (modify in place) | exact |

## Pattern Assignments

### `crates/hephaestus-resolve/src/storage.rs` (service, file-I/O) -- NEW, replaces s3.rs

**Analog:** `crates/hephaestus-resolve/src/s3.rs`

**Imports pattern** (lines 1-13):
```rust
//! S3 model cache operations.
//! [update doc comment to reflect backend-agnostic nature]

use std::path::{Path, PathBuf};

// REMOVE these:
// use aws_sdk_s3::Client as S3Client;
// use aws_sdk_s3::operation::get_object::GetObjectError;
// use aws_sdk_s3::primitives::ByteStream;

// ADD:
// use opendal::Operator;
// use opendal::ErrorKind;

use crate::error::ResolveError;
```

**Constants pattern** (lines 16-26):
```rust
pub(crate) const MODEL_ONNX: &str = "model.onnx";
pub(crate) const TOKENIZER_JSON: &str = "tokenizer.json";
pub(crate) const CONFIG_JSON: &str = "config.json";
pub(crate) const ONNX_SUBDIR_MODEL: &str = "onnx/model.onnx";

// Rename from S3_CACHE_SUBDIR:
pub(crate) const STORAGE_CACHE_SUBDIR: &str = "hephaestus/storage-cache";
```

**Core download pattern** (lines 36-120) -- preserve structure, swap S3 calls for OpenDAL:
```rust
// Signature changes from:
pub(crate) async fn download_model_from_s3(
    client: &S3Client,
    bucket: &str,
    s3_prefix: &str,
    model_id: &str,
    cache_dir: &Path,
) -> Result<Option<PathBuf>, ResolveError> {
// To:
pub(crate) async fn download_model(
    op: &Operator,
    model_id: &str,
    cache_dir: &Path,
) -> Result<Option<PathBuf>, ResolveError> {
    // Note: no bucket/prefix args -- OpenDAL Operator has root configured
```

**Cache miss detection pattern** (lines 57-68) -- replace string matching with ErrorKind:
```rust
// OLD (fragile string matching):
Err(ResolveError::S3(ref msg)) if msg.contains("NoSuchKey") => {
    continue;
}

// NEW (typed error):
Err(e) if e.kind() == ErrorKind::NotFound => {
    continue;  // cache miss
}
```

**Single file download pattern** (lines 205-232) -- replace S3 get_object with Operator::read:
```rust
// OLD:
async fn download_s3_file(client: &S3Client, bucket: &str, key: &str) -> Result<Vec<u8>, ResolveError> {
    let resp = client.get_object().bucket(bucket).key(key).send().await...

// NEW:
async fn download_file(op: &Operator, path: &str) -> Result<Option<Vec<u8>>, ResolveError> {
    match op.read(path).await {
        Ok(data) => Ok(Some(data.to_vec())),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(ResolveError::Storage(format!("read {path}: {e}"))),
    }
}
```

**Upload pattern** (lines 131-189) -- replace put_object with Operator::write:
```rust
// OLD:
pub(crate) async fn upload_model_to_s3(
    client: &S3Client, bucket: &str, s3_prefix: &str, model_id: &str, local_dir: &Path,
) -> Result<(), ResolveError> {

// NEW:
pub(crate) async fn upload_model(
    op: &Operator, model_id: &str, local_dir: &Path,
) -> Result<(), ResolveError> {
    // Walk local_dir, read each file, op.write(path, bytes).await
```

**Path formatting pattern** (lines 196-202):
```rust
// OLD:
fn format_s3_key(s3_prefix: &str, model_id: &str, filename: &str) -> String {

// NEW: prefix is in Operator root, so just:
fn format_storage_path(model_id: &str, filename: &str) -> String {
    format!("{model_id}/{filename}")
}
```

**Atomic download pattern** (lines 96-119) -- preserved exactly, only error variant changes:
```rust
let temp_dir = tempfile::TempDir::new_in(parent)
    .map_err(|e| ResolveError::Storage(format!("failed to create temp dir: {e}")))?;
// ... write files ...
tokio::fs::rename(temp_dir.path(), &final_dir).await?;
let _ = temp_dir.keep();
```

**Test pattern** (lines 234-415) -- replace S3 client with Memory backend:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use opendal::{Operator, services::Memory};

    fn memory_operator() -> Operator {
        Operator::new(Memory::default()).unwrap()
    }

    #[tokio::test]
    async fn download_returns_none_on_cache_miss() {
        let op = memory_operator();
        // No files written -- should return None
        let result = download_model(&op, "org/model", &cache_dir).await;
        assert!(matches!(result, Ok(None)));
    }
}
```

---

### `crates/hephaestus-resolve/src/resolver.rs` (service, request-response) -- MODIFY

**Analog:** self

**Struct field changes** (lines 25-31):
```rust
// OLD:
pub struct ModelResolver<F: ForgeClient = StubForgeClient> {
    cache_dir: PathBuf,
    s3_client: Option<aws_sdk_s3::Client>,
    s3_bucket: Option<String>,
    s3_prefix: Option<String>,
    forge: F,
}

// NEW:
pub struct ModelResolver<F: ForgeClient = StubForgeClient> {
    cache_dir: PathBuf,
    operator: Option<opendal::Operator>,
    forge: F,
}
```

**Constructor change** (lines 99-132):
```rust
// OLD: new_with_client takes s3_bucket, s3_prefix, loads aws_config
// NEW: new_with_client takes Option<Operator>, no aws_config loading

pub async fn new_with_client(
    operator: Option<opendal::Operator>,
    forge: F,
) -> Result<Self, ResolveError> {
    let cache_dir = ...;  // same HF_HOME logic
    Ok(Self { cache_dir, operator, forge })
}
```

**Resolve tier 1 change** (lines 146-167):
```rust
// OLD:
if let (Some(client), Some(bucket)) = (&self.s3_client, &self.s3_bucket) {
    let prefix = self.s3_prefix.as_deref().unwrap_or("");
    s3::download_model_from_s3(client, bucket, prefix, model_id, cache_dir).await

// NEW:
if let Some(op) = &self.operator {
    storage::download_model(op, model_id, &self.cache_dir).await
```

**Import change** (line 14):
```rust
// OLD: use crate::s3;
// NEW: use crate::storage;
```

**spawn_cache_back change** (lines 257-291):
```rust
// OLD: clones s3_client, s3_bucket, s3_prefix
// NEW: clones operator (Operator is Clone)
fn spawn_cache_back(&self, model_id: &str, local_dir: &Path) {
    let Some(op) = self.operator.clone() else { return };
    let model_id = model_id.to_string();
    let local_dir = local_dir.to_path_buf();
    tokio::spawn(async move {
        // Note: with_retry NOT needed for storage ops -- RetryLayer handles it
        if let Err(e) = storage::upload_model(&op, &model_id, &local_dir).await {
            tracing::warn!(model_id, error = %e, "failed to cache model to storage");
        } else {
            tracing::info!(model_id, "successfully cached model to storage");
        }
    });
}
```

---

### `crates/hephaestus-resolve/src/error.rs` (model, transform) -- MODIFY

**Analog:** self

**Variant rename** (lines 22-24):
```rust
// OLD:
#[error("S3 error: {0}")]
S3(String),

// NEW:
#[error("storage error: {0}")]
Storage(String),
```

---

### `crates/hephaestus/src/config.rs` (config, transform) -- MODIFY

**Analog:** self

**Field changes** (lines 64-72):
```rust
// OLD:
#[serde(default)]
pub s3_bucket: Option<String>,
#[serde(default)]
pub s3_prefix: Option<String>,

// NEW:
#[serde(default = "default_storage_type")]
pub storage_type: String,
#[serde(default)]
pub storage_bucket: Option<String>,
#[serde(default)]
pub storage_prefix: Option<String>,
#[serde(default)]
pub storage_root: Option<String>,
#[serde(default)]
pub storage_region: Option<String>,
```

**Default function** (add):
```rust
fn default_storage_type() -> String {
    "s3".to_string()
}
```

**Test helper update** (lines 223-243) -- replace s3_bucket/s3_prefix with storage fields in `config_with_model_path`.

---

### `crates/hephaestus/src/main.rs` (controller, request-response) -- MODIFY

**Analog:** self

**Operator construction** (insert between config load and resolver construction, around line 57):
```rust
// NEW: build OpenDAL Operator from config
let operator = if config.storage_type == "none" {
    None
} else {
    let mut cfg = std::collections::HashMap::new();
    if let Some(ref bucket) = config.storage_bucket {
        cfg.insert("bucket".to_string(), bucket.clone());
    }
    if let Some(ref region) = config.storage_region {
        cfg.insert("region".to_string(), region.clone());
    }
    if let Some(ref prefix) = config.storage_prefix {
        cfg.insert("root".to_string(), format!("/{prefix}"));
    }
    if let Some(ref root) = config.storage_root {
        cfg.insert("root".to_string(), root.clone());
    }
    let op = opendal::Operator::via_iter(&config.storage_type, cfg)
        .context("failed to build storage operator")?
        .layer(opendal::layers::RetryLayer::new().with_max_times(3));
    Some(op)
};
```

**Resolver construction change** (lines 61-88):
```rust
// OLD: ModelResolver::new_with_client(config.s3_bucket.as_deref(), config.s3_prefix.as_deref(), forge_client)
// NEW: ModelResolver::new_with_client(operator.clone(), forge_client)
```

---

### `forge/src/forge/storage.py` (service, file-I/O) -- MODIFY

**Analog:** self (current boto3 version)

**Full replacement** (lines 1-45):
```python
# OLD:
import boto3
from boto3.s3.transfer import TransferConfig
def upload_to_s3(local_dir, bucket, prefix, model_id) -> list[str]:
    s3 = boto3.client("s3")
    ...

# NEW:
import opendal
def upload_to_storage(op: opendal.Operator, model_id: str, local_dir: str) -> list[str]:
    uploaded: list[str] = []
    for root, _dirs, files in os.walk(local_dir):
        for filename in sorted(files):
            filepath = os.path.join(root, filename)
            relative = os.path.relpath(filepath, local_dir)
            path = f"{model_id}/{relative}"
            with open(filepath, "rb") as f:
                op.write(path, f.read())
            uploaded.append(path)
    return uploaded
```

---

### `forge/src/forge/config.py` (config, transform) -- MODIFY

**Analog:** self

**Field changes:**
```python
# OLD:
class ForgeSettings(BaseSettings):
    s3_bucket: str = ""
    s3_prefix: str = ""

# NEW:
class ForgeSettings(BaseSettings):
    storage_type: str = "s3"
    storage_bucket: str = ""
    storage_prefix: str = ""
    storage_root: str = ""
    storage_region: str = ""
```

---

## Shared Patterns

### Error Handling (Rust)
**Source:** `crates/hephaestus-resolve/src/error.rs` lines 1-71
**Apply to:** `storage.rs`, `resolver.rs`
```rust
// All storage errors map to ResolveError::Storage(String)
// Pattern: map_err with context string
.map_err(|e| ResolveError::Storage(format!("{context}: {e}")))?;
```

### Atomic Download Pattern
**Source:** `crates/hephaestus-resolve/src/s3.rs` lines 96-119
**Apply to:** `storage.rs`
```rust
let parent = final_dir.parent().unwrap_or(cache_dir);
tokio::fs::create_dir_all(parent).await?;
let temp_dir = tempfile::TempDir::new_in(parent)
    .map_err(|e| ResolveError::Storage(format!("failed to create temp dir: {e}")))?;
// ... write files to temp_dir ...
tokio::fs::rename(temp_dir.path(), &final_dir).await?;
let _ = temp_dir.keep();
```

### Config from Env (Rust)
**Source:** `crates/hephaestus/src/config.rs` lines 1-10
**Apply to:** config.rs updates
```rust
use anyhow::{Context, bail};
use serde::Deserialize;
// All config fields use #[serde(default)] or #[serde(default = "fn_name")]
// Load via: envy::from_env::<Self>()
```

### Config from Env (Python)
**Source:** `forge/src/forge/config.py`
**Apply to:** config.py updates
```python
from pydantic_settings import BaseSettings
class ForgeSettings(BaseSettings):
    # Fields map to env vars of the same name (case-insensitive)
    storage_type: str = "s3"
```

## No Analog Found

No files in this phase lack analogs -- every file is either a direct replacement of an existing file or a modification of one.

## Metadata

**Analog search scope:** `crates/hephaestus-resolve/src/`, `crates/hephaestus/src/`, `forge/src/forge/`
**Files scanned:** 10
**Pattern extraction date:** 2026-08-26
