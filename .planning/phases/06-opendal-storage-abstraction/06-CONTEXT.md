# Phase 6: OpenDAL Storage Abstraction - Context

**Gathered:** 2026-08-26
**Status:** Ready for planning

<domain>
## Phase Boundary

Replace all direct `aws-sdk-s3` usage in `hephaestus-resolve` and all `boto3` usage in the Forge with Apache OpenDAL, enabling multi-cloud and local-filesystem storage backends via runtime configuration. The resolver's `s3.rs` becomes `storage.rs` using OpenDAL's `Operator` directly. The Forge's `storage.py` switches from `boto3` to the `opendal` Python package. Both services share the same `STORAGE_*` env var configuration. The 3-tier resolution chain (Storage → HuggingFace → Forge) and the atomic download pattern are preserved. No new model profiles, no API changes, no changes to the Forge's conversion or validation logic.

</domain>

<decisions>
## Implementation Decisions

### Backend Configuration Model
- **D-01:** `STORAGE_TYPE` + `STORAGE_*` prefix env vars for backend configuration. `STORAGE_TYPE=s3|fs|gcs|azblob|none` selects the backend. Backend-specific config uses `STORAGE_` prefix: `STORAGE_BUCKET`, `STORAGE_REGION`, `STORAGE_ROOT`, etc. Maps directly to OpenDAL's `Operator::via_map()`.
- **D-02:** Default to S3 when `STORAGE_TYPE` is unset. Existing deployments continue working without config changes (but env var names change from `S3_*` to `STORAGE_*`).
- **D-03:** No aliases for old `S3_BUCKET`/`S3_PREFIX` env vars. Clean break — update k8s manifests in one pass. Internal service with controlled deployments.
- **D-04:** `STORAGE_PREFIX` as universal path prefix across all backends. On S3 it becomes a key prefix, on filesystem it becomes a subdirectory, on GCS a path prefix. Same layout everywhere: `{prefix}/{model_id}/{filename}`.
- **D-05:** `STORAGE_TYPE=none` explicitly disables the storage tier. Resolution starts at HuggingFace when set.

### Forge Service Alignment
- **D-06:** Forge switches from `boto3` to OpenDAL Python bindings (`opendal` package). Both Rust and Python services use the same storage abstraction library.
- **D-07:** Forge uses the same `STORAGE_*` env vars as Hephaestus. One set of storage config per namespace in k8s.
- **D-08:** Keep in-memory conversion lock, single Forge replica. OpenDAL only changes the upload mechanism, not the concurrency model.
- **D-09:** No changes to Forge validation logic (onnx.checker + dummy inference). Validation operates on local files before upload — storage-agnostic already.
- **D-10:** `opendal` replaces `boto3` as a required dependency in `pyproject.toml`. Clean removal of boto3, no fallback.

### Storage Abstraction Boundary
- **D-11:** Use OpenDAL's `Operator` directly in resolver code. No Hephaestus-specific storage trait — OpenDAL IS the abstraction. `Operator` already provides `read()`/`write()`/`list()`/`stat()`.
- **D-12:** Keep the atomic temp-dir-then-rename download pattern (Phase 3 D-06). Source bytes from `Operator::read()` instead of S3 `GetObject`. Pattern is backend-agnostic.
- **D-13:** Replace `s3.rs` with `storage.rs`. Clean rename reflecting backend-agnostic nature. Update `mod.rs`/`lib.rs` imports.
- **D-14:** OpenDAL `Memory` backend for unit tests. No mocks needed — real OpenDAL code path with in-memory storage. Tests write files to memory operator, then test download/upload logic.

### Local Dev Experience
- **D-15:** `STORAGE_TYPE=fs` + `STORAGE_ROOT=/path/to/models` for local filesystem backend. Models cached at `{root}/{prefix}/{model_id}/`. Same path layout as other backends.
- **D-16:** Storage tier replaces S3 tier only. Resolution chain stays 3-tier: Storage (OpenDAL) → HuggingFace → Forge. With fs backend, Tier 1 reads from a local directory. HF download + cache-back still writes to the same directory.
- **D-17:** `STORAGE_ROOT` is required when `STORAGE_TYPE=fs`. No default path — operators/devs always know exactly where models go.

### Claude's Discretion
- OpenDAL `Operator` construction and error mapping details
- How `STORAGE_*` env vars map to OpenDAL's `HashMap<String, String>` config
- Retry logic adaptation (OpenDAL may have built-in retry layers)
- `aws-sdk-s3`, `aws-config` dependency removal from workspace `Cargo.toml`
- Python `opendal` Operator construction in Forge `storage.py`
- Config struct field changes (replacing `s3_bucket`/`s3_prefix` with `storage_type`/`storage_bucket`/`storage_prefix`/`storage_root`)

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Project Context
- `.planning/PROJECT.md` — Core value, constraints, key decisions, model resolution flow description
- `.planning/REQUIREMENTS.md` — Full v1 requirement list; Phase 6 adds STOR-01
- `.planning/ROADMAP.md` — Phase 6 goal, success criteria, dependency chain

### Prior Phase Context
- `.planning/phases/03-model-resolution/03-CONTEXT.md` — S3 cache layout (D-01), atomic download pattern (D-06), resolution fallback chain (D-04/D-05), cache-back strategy (D-12/D-13/D-14), ForgeClient trait contract (D-08/D-09/D-10/D-11)
- `.planning/phases/05-forge-conversion-service/05-CONTEXT.md` — Forge project structure (D-11/D-12/D-13/D-14), conversion flow (D-01 through D-04), validation steps (D-05/D-06/D-07), Python deps approach (D-14)

### Coding Rules
- `rules/` — Full directory of Rust coding rules. All code must comply.

### Existing Rust Code (storage integration points)
- `crates/hephaestus-resolve/src/s3.rs` — Current S3 operations: `download_model_from_s3()`, `upload_model_to_s3()`, `download_s3_file()`, `format_s3_key()`. THIS FILE GETS REPLACED by `storage.rs`.
- `crates/hephaestus-resolve/src/resolver.rs` — `ModelResolver` holds `s3_client: Option<aws_sdk_s3::Client>`, `s3_bucket`, `s3_prefix`. These fields change to `operator: Option<opendal::Operator>`.
- `crates/hephaestus/src/config.rs` — Config struct with `s3_bucket`, `s3_prefix` fields. Replace with `storage_type`, `storage_bucket`, `storage_prefix`, `storage_root`.
- `crates/hephaestus/src/main.rs` — Binary entry point. Wires storage config into `ModelResolver::new()`. Changes to construct OpenDAL `Operator` from env vars.
- `crates/hephaestus-resolve/Cargo.toml` — Dependencies: remove `aws-sdk-s3`, `aws-config`; add `opendal`.

### Existing Python Code (Forge storage)
- `forge/src/forge/storage.py` — `upload_to_s3()` using `boto3`. Replace with OpenDAL-based upload function.
- `forge/pyproject.toml` — Dependencies: remove `boto3`; add `opendal`.
- `forge/tests/conftest.py` — Test fixtures that may reference boto3/S3 mocking. Update to use OpenDAL memory backend or test helpers.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `with_retry()` in `resolver.rs`: generic async retry with exponential backoff. Backend-agnostic — works with any `Result` return type. Reuse for OpenDAL operations.
- `format_s3_key()` logic: key format `{prefix}/{model_id}/{filename}` stays the same. Rename to `format_storage_path()` in `storage.rs`.
- `ModelResolver<F: ForgeClient>` generic pattern: already generic over Forge client. Add OpenDAL `Operator` as a field alongside the Forge client.
- `validate_model_id()`: model ID validation is storage-agnostic. No changes needed.

### Established Patterns
- Config from env vars via envy — new `STORAGE_*` fields follow the same `#[serde(default)]` pattern
- thiserror for library errors in crate boundaries — `ResolveError::S3(String)` becomes `ResolveError::Storage(String)`
- Deep module interfaces: `resolve()` stays as the single entry point hiding all storage complexity
- Atomic operations: temp dir + rename pattern preserved with OpenDAL
- Background cache-back: `spawn_cache_back()` fires a tokio task — change internal S3 calls to OpenDAL `Operator::write()`

### Integration Points
- `resolver.rs:27` — `s3_client: Option<aws_sdk_s3::Client>` → `operator: Option<opendal::Operator>`
- `resolver.rs:28-29` — `s3_bucket: Option<String>`, `s3_prefix: Option<String>` → may be folded into Operator config
- `resolver.rs:99-132` — `new_with_client()` constructs S3 client from AWS config → construct OpenDAL Operator from `STORAGE_*` env vars
- `resolver.rs:150-151` — calls `s3::download_model_from_s3()` → calls `storage::download_model()`
- `resolver.rs:268-271` — `spawn_cache_back()` calls `s3::upload_model_to_s3()` → calls `storage::upload_model()`
- `config.rs:64-71` — `s3_bucket`, `s3_prefix` fields → `storage_type`, `storage_bucket`, `storage_prefix`, `storage_root`
- `main.rs` — wires config into resolver constructor → pass Operator instead of S3 client
- `Cargo.toml` workspace deps — remove `aws-sdk-s3`, `aws-config`; add `opendal`
- `forge/storage.py` — `boto3.client("s3")` → `opendal.Operator("s3", ...)` or `opendal.Operator("fs", ...)`

</code_context>

<specifics>
## Specific Ideas

- Both services (Rust Hephaestus and Python Forge) use OpenDAL's bindings for their respective languages, ensuring the same backend abstraction works everywhere. The Python `opendal` package is the official binding from the Apache OpenDAL project.
- The key/path layout `{prefix}/{model_id}/{filename}` is preserved unchanged. OpenDAL paths work the same way across backends — `s3://bucket/prefix/org/model/model.onnx` maps to `Operator::read("prefix/org/model/model.onnx")`.
- `STORAGE_TYPE=none` provides a clean way to run without any storage tier (HF-only or HF+Forge), replacing the implicit "no S3_BUCKET means no S3" behavior.

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

</deferred>

---

*Phase: 6-OpenDAL Storage Abstraction*
*Context gathered: 2026-08-26*
