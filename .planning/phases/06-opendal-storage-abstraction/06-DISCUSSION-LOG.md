# Phase 6: OpenDAL Storage Abstraction - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-08-26
**Phase:** 06-opendal-storage-abstraction
**Areas discussed:** Backend configuration model, Forge service alignment, Storage abstraction boundary, Local dev experience

---

## Backend Configuration Model

### How should operators specify the storage backend?

| Option | Description | Selected |
|--------|-------------|----------|
| STORAGE_TYPE + prefix vars | STORAGE_TYPE=s3\|fs\|gcs\|azblob env var selects backend. Backend-specific config uses STORAGE_ prefix. OpenDAL maps naturally via Operator::via_map(). | ✓ |
| Keep S3 vars, add STORAGE_TYPE | Keep S3_BUCKET/S3_PREFIX for backward compat. Add STORAGE_TYPE for non-S3 backends. | |
| Connection-string style | Single STORAGE_URL env var like 's3://bucket/prefix' or 'fs:///var/models'. | |

**User's choice:** STORAGE_TYPE + prefix vars
**Notes:** Clean break from S3-specific naming.

### What happens when STORAGE_TYPE is not set?

| Option | Description | Selected |
|--------|-------------|----------|
| Default to S3 | No STORAGE_TYPE = S3 backend, matching current behavior. | ✓ |
| Require STORAGE_TYPE | Crash with clear error if missing. | |
| Default to disabled | Storage tier skipped entirely. | |

**User's choice:** Default to S3
**Notes:** Backward compatibility for existing deployments.

### Should old S3_BUCKET/S3_PREFIX vars still work as aliases?

| Option | Description | Selected |
|--------|-------------|----------|
| No aliases, clean break | Only STORAGE_* recognized. Internal service with controlled deployments. | ✓ |
| Temporary aliases with deprecation warning | Accept S3_* with startup warning, prefer STORAGE_* when both set. | |

**User's choice:** No aliases, clean break
**Notes:** Internal service — one deployment update is simpler than maintaining legacy code paths.

### How should the key prefix work across backends?

| Option | Description | Selected |
|--------|-------------|----------|
| STORAGE_PREFIX as universal path prefix | Works the same way across all backends — prepended to model_id/filename. | ✓ |
| Backend-specific prefix handling | Each backend interprets prefix differently. | |
| No prefix, baked into root/bucket | Remove prefix concept entirely. | |

**User's choice:** STORAGE_PREFIX as universal path prefix
**Notes:** Consistent layout across all backends.

---

## Forge Service Alignment

### Should the Forge service also support non-S3 backends?

| Option | Description | Selected |
|--------|-------------|----------|
| Forge stays on boto3/S3-only | Forge always runs in AWS. Local-fs dev doesn't use Forge. | |
| Forge gets a Python storage abstraction | Use a Python object-store library to match Hephaestus. | |
| Forge uploads to Hephaestus instead | Forge POSTs files back to Hephaestus, which stores them. | |

**User's choice:** Other — "opendal has a python library as well, use that"
**Notes:** User pointed out OpenDAL has official Python bindings. Both services use the same storage library family.

### Should the Forge use the same STORAGE_* env vars as Hephaestus?

| Option | Description | Selected |
|--------|-------------|----------|
| Same env vars | Both pods share the same storage config in k8s. | ✓ |
| Forge-specific prefix | FORGE_STORAGE_TYPE etc. allows different storage targets. | |

**User's choice:** Same env vars
**Notes:** One set of env vars to manage per namespace.

### Forge conversion queue lock with OpenDAL?

| Option | Description | Selected |
|--------|-------------|----------|
| Keep in-memory lock, single replica | OpenDAL only changes upload mechanism, not concurrency model. | ✓ |
| You decide | | |

**User's choice:** Keep in-memory lock, single replica

### Should Forge validation step change?

| Option | Description | Selected |
|--------|-------------|----------|
| No changes to validation | Validation operates on local files before upload. Storage-agnostic already. | ✓ |
| You decide | | |

**User's choice:** No changes to validation

### Forge dependency management?

| Option | Description | Selected |
|--------|-------------|----------|
| opendal required, remove boto3 | Clean break. Replace boto3 with opendal in pyproject.toml. | ✓ |
| opendal required, keep boto3 for other uses | Keep boto3 if used elsewhere. | |
| You decide | | |

**User's choice:** opendal required, remove boto3

---

## Storage Abstraction Boundary

### How should the storage abstraction be structured in Rust?

| Option | Description | Selected |
|--------|-------------|----------|
| Use OpenDAL Operator directly | No Hephaestus-specific trait. OpenDAL IS the abstraction. Resolver holds Option\<Operator\>. | ✓ |
| Hephaestus storage trait wrapping OpenDAL | Define ModelStorage trait. Adds testability via mockall. | |
| Keep free functions, swap internals | Rename functions, internally use OpenDAL. | |

**User's choice:** Use OpenDAL Operator directly

### Atomic temp-dir-then-rename download pattern?

| Option | Description | Selected |
|--------|-------------|----------|
| Keep the pattern, use Operator::read() | Same atomic pattern, source bytes from OpenDAL. | ✓ |
| You decide | | |

**User's choice:** Keep the pattern, use Operator::read()

### Should s3.rs be renamed or replaced?

| Option | Description | Selected |
|--------|-------------|----------|
| Replace s3.rs with storage.rs | Clean rename reflecting backend-agnostic nature. | ✓ |
| Keep s3.rs, refactor in place | Less churn but misleading filename. | |
| You decide | | |

**User's choice:** Replace s3.rs with storage.rs

### Testing approach?

| Option | Description | Selected |
|--------|-------------|----------|
| OpenDAL memory backend for unit tests | Real OpenDAL code path with in-memory storage. No mocks needed. | ✓ |
| You decide | | |

**User's choice:** OpenDAL memory backend for unit tests

---

## Local Dev Experience

### How should the local filesystem backend work?

| Option | Description | Selected |
|--------|-------------|----------|
| STORAGE_TYPE=fs + STORAGE_ROOT | Models cached at {root}/{prefix}/{model_id}/. Same path layout. | ✓ |
| Auto-detect local when no cloud config | Automatically fall back to local filesystem. | |
| You decide | | |

**User's choice:** STORAGE_TYPE=fs + STORAGE_ROOT

### How does fs backend interact with HuggingFace cache tier?

| Option | Description | Selected |
|--------|-------------|----------|
| Storage tier replaces S3 tier only | 3-tier chain stays: Storage → HuggingFace → Forge. | ✓ |
| You decide | | |

**User's choice:** Storage tier replaces S3 tier only

### Should STORAGE_ROOT be required when STORAGE_TYPE=fs?

| Option | Description | Selected |
|--------|-------------|----------|
| Required, no default | Explicit is better. Avoids hidden directories. | ✓ |
| Default to ~/.cache/hephaestus | Convenient but may surprise users. | |

**User's choice:** Required, no default

### Should there be a 'disabled' storage mode?

| Option | Description | Selected |
|--------|-------------|----------|
| STORAGE_TYPE unset = S3, no explicit disabled | Rely on missing config to skip tier. | |
| Add STORAGE_TYPE=none | Explicit 'none' value skips storage tier entirely. | ✓ |

**User's choice:** Add STORAGE_TYPE=none
**Notes:** Clearer intent than relying on missing config values.

---

## Claude's Discretion

- OpenDAL Operator construction and error mapping details
- How STORAGE_* env vars map to OpenDAL's HashMap config
- Retry logic adaptation for OpenDAL
- aws-sdk-s3/aws-config dependency removal
- Python opendal Operator construction in Forge
- Config struct field changes

## Deferred Ideas

None — discussion stayed within phase scope.
