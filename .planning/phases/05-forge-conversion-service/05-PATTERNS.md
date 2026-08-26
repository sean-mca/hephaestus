# Phase 5: Forge Conversion Service - Pattern Map

**Mapped:** 2026-08-26
**Files analyzed:** 15 (8 Python new, 4 Rust modified, 3 config/build new)
**Analogs found:** 4 / 15

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|-------------------|------|-----------|----------------|---------------|
| `forge/src/forge/main.py` | controller | request-response | -- | no-analog |
| `forge/src/forge/api.py` | controller | request-response | -- | no-analog |
| `forge/src/forge/converter.py` | service | transform | -- | no-analog |
| `forge/src/forge/storage.py` | service | file-I/O | -- | no-analog |
| `forge/src/forge/queue.py` | service | event-driven | -- | no-analog |
| `forge/src/forge/models.py` | model | request-response | -- | no-analog |
| `forge/src/forge/config.py` | config | -- | `crates/hephaestus/src/config.rs` | role-match |
| `forge/src/forge/__init__.py` | config | -- | -- | no-analog |
| `forge/pyproject.toml` | config | -- | -- | no-analog |
| `forge/Dockerfile` | config | -- | -- | no-analog |
| `forge/tests/` | test | -- | -- | no-analog |
| `crates/hephaestus-resolve/src/forge.rs` (modify) | service | request-response | self (current version) | exact |
| `crates/hephaestus-resolve/src/resolver.rs` (modify) | service | CRUD | self (current version) | exact |
| `crates/hephaestus-resolve/src/error.rs` (modify) | model | -- | self (current version) | exact |
| `crates/hephaestus/src/config.rs` (modify) | config | -- | self (current version) | exact |

## Pattern Assignments

### `crates/hephaestus-resolve/src/forge.rs` (modify -- add HttpForgeClient)

**Analog:** Self (current version, lines 1-99)

**Trait pattern to preserve** (lines 18-28):
```rust
#[cfg_attr(test, mockall::automock)]
pub trait ForgeClient: Send + Sync {
    fn convert(
        &self,
        model_id: &str,
    ) -> impl std::future::Future<Output = Result<Vec<String>, ResolveError>> + Send;
}
```

**Key change:** Return type changes from `Result<Vec<String>, ResolveError>` to `Result<ForgeResponse, ResolveError>` where `ForgeResponse` contains `s3_paths: Vec<String>` and `metadata: ConversionMetadata`. The `#[cfg_attr(test, mockall::automock)]` attribute must remain.

**Error pattern to follow** (from error.rs lines 6-59):
```rust
#[derive(Error, Debug)]
pub enum ResolveError {
    // Add new variant:
    // #[error("Forge conversion failed for model '{model_id}': {reason}")]
    // ForgeConversion { model_id: String, reason: String },
}
```

**StubForgeClient pattern** (lines 35-43) -- keep as-is, update return type to match new trait signature.

---

### `crates/hephaestus-resolve/src/resolver.rs` (modify -- generalize to generic ForgeClient)

**Analog:** Self (current version, lines 1-309)

**Current concrete type** (lines 23-31):
```rust
pub struct ModelResolver {
    cache_dir: PathBuf,
    s3_client: Option<aws_sdk_s3::Client>,
    s3_bucket: Option<String>,
    s3_prefix: Option<String>,
    forge: StubForgeClient,
    #[allow(dead_code)]
    forge_url: Option<String>,
}
```

**Generalization pattern** (from RESEARCH.md):
```rust
pub struct ModelResolver<F: ForgeClient = StubForgeClient> {
    // ... same fields ...
    forge: F,
}
```

**Constructor pattern** (lines 81-115) -- `new()` signature changes to accept `F` parameter instead of hardcoding `StubForgeClient`. The `forge_url` field becomes unnecessary since the client is constructed externally.

**Forge tier usage** (line 189) -- `self.forge.convert(model_id).await` already calls through the trait. Update to destructure `ForgeResponse` instead of receiving `Vec<String>` directly.

---

### `crates/hephaestus/src/config.rs` (modify -- add forge_timeout_secs)

**Analog:** Self (current version)

**Config field pattern** (lines 64-77 for existing forge_url, lines 86-98 for batch fields):
```rust
/// Forge conversion service timeout in seconds (default: 600, env `FORGE_TIMEOUT_SECS`).
#[serde(default = "default_forge_timeout_secs")]
pub forge_timeout_secs: u64,
```

**Default function pattern** (lines 121-127):
```rust
fn default_forge_timeout_secs() -> u64 {
    600
}
```

**Test helper pattern** (lines 215-234) -- add `forge_timeout_secs: 600` to `config_with_model_path()` helper.

---

### `crates/hephaestus/src/main.rs` (modify -- wire real ForgeClient)

**Analog:** Self (current version)

**Resolver construction site** (lines 57-63):
```rust
let resolver = ModelResolver::new(
    config.s3_bucket.as_deref(),
    config.s3_prefix.as_deref(),
    config.forge_url.as_deref(),
)
.await
.context("failed to construct model resolver")?;
```

**Change:** When `config.forge_url` is `Some`, construct `HttpForgeClient::new(url, config.forge_timeout_secs)` and pass to `ModelResolver`. When `None`, use `StubForgeClient`.

---

### `crates/hephaestus-resolve/src/error.rs` (modify -- add ForgeConversion variant)

**Analog:** Self (current version)

**Error variant pattern** (lines 43-54):
```rust
#[error(
    "Forge is not configured for model '{model_id}': set FORGE_URL to enable conversion"
)]
ForgeUnavailable {
    model_id: String,
},
```

**New variant follows same pattern:**
```rust
#[error("Forge conversion failed for model '{model_id}': {reason}")]
ForgeConversion {
    model_id: String,
    reason: String,
},
```

---

### Python Forge files (all new -- no codebase analogs)

**No existing Python code in this repo.** All Python files use patterns from RESEARCH.md:

- **`forge/src/forge/main.py`**: FastAPI app with lifespan pattern, health endpoints
- **`forge/src/forge/api.py`**: FastAPI router with `/convert` POST endpoint
- **`forge/src/forge/converter.py`**: `optimum.exporters.onnx.main_export` + two-stage validation
- **`forge/src/forge/storage.py`**: boto3 `upload_file` with `TransferConfig`
- **`forge/src/forge/queue.py`**: `asyncio.Semaphore(1)` + `defaultdict(asyncio.Lock)` per model_id
- **`forge/src/forge/models.py`**: Pydantic `BaseModel` for `ConvertRequest`, `ConvertResponse`, `ConversionMetadata`
- **`forge/src/forge/config.py`**: Pydantic `BaseSettings` with env var loading (mirrors Rust config pattern conceptually)

## Shared Patterns

### Error Handling (Rust)
**Source:** `crates/hephaestus-resolve/src/error.rs` (full file)
**Apply to:** All Rust modified files
- `thiserror::Error` derive on `ResolveError` enum
- Structured variants with named fields (`model_id`, `reason`)
- Display messages include the model ID for debugging

### Config from Environment (Rust)
**Source:** `crates/hephaestus/src/config.rs` lines 24-99
**Apply to:** New `forge_timeout_secs` field
- `#[serde(default = "default_fn")]` for optional fields with defaults
- Standalone `fn default_*() -> Type` functions
- Test helper constructs Config directly (not through envy)

### Deep Module / Single-Method Trait (Rust)
**Source:** `crates/hephaestus-resolve/src/forge.rs` lines 18-28
**Apply to:** `HttpForgeClient` implementation
- `ForgeClient` trait has exactly one method (`convert`)
- `#[cfg_attr(test, mockall::automock)]` for testability
- Implementation hides all HTTP complexity behind `convert()`

### CPU-bound Work in Async Context (Python)
**Source:** RESEARCH.md pitfall 1
**Apply to:** `forge/src/forge/converter.py`, `forge/src/forge/queue.py`
- Wrap `main_export()` in `asyncio.to_thread()` to avoid blocking the event loop
- Sequential semaphore ensures only one conversion at a time

## No Analog Found

| File | Role | Data Flow | Reason |
|------|------|-----------|--------|
| `forge/src/forge/main.py` | controller | request-response | First Python code in repo; use FastAPI lifespan pattern from RESEARCH.md |
| `forge/src/forge/api.py` | controller | request-response | First Python code in repo; use FastAPI router pattern from RESEARCH.md |
| `forge/src/forge/converter.py` | service | transform | First Python code in repo; use optimum main_export pattern from RESEARCH.md |
| `forge/src/forge/storage.py` | service | file-I/O | First Python code in repo; use boto3 pattern from RESEARCH.md |
| `forge/src/forge/queue.py` | service | event-driven | First Python code in repo; use asyncio pattern from RESEARCH.md |
| `forge/src/forge/models.py` | model | request-response | First Python code in repo; use Pydantic BaseModel from RESEARCH.md |
| `forge/src/forge/config.py` | config | -- | First Python code in repo; use Pydantic BaseSettings |
| `forge/pyproject.toml` | config | -- | First Python project in repo |
| `forge/Dockerfile` | config | -- | First Python Dockerfile in repo |
| `forge/tests/*` | test | -- | First Python tests in repo; use pytest + httpx + moto from RESEARCH.md |

## Metadata

**Analog search scope:** `crates/` (Rust source)
**Files scanned:** 5 Rust source files (the 4 integration points + main.rs)
**Pattern extraction date:** 2026-08-26
