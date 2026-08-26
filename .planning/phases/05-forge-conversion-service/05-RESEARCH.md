# Phase 5: Forge Conversion Service - Research

**Researched:** 2026-08-26
**Domain:** Python ML model conversion service + Rust HTTP client integration
**Confidence:** HIGH

## Summary

Phase 5 builds a persistent Python service (the Forge) that converts HuggingFace models to ONNX format via `optimum`, validates the conversion, uploads artifacts to S3, and exposes an HTTP API that Hephaestus calls. On the Rust side, the existing `StubForgeClient` is replaced with a real `reqwest`-based HTTP client, and `ModelResolver` is generalized to accept any `ForgeClient` implementation.

This is a two-language phase: Python for the Forge service (FastAPI + optimum + boto3 + onnxruntime) and Rust for the client changes (reqwest with JSON and timeout). The Python side is a standalone microservice with its own Dockerfile and dependency management (uv + pyproject.toml). The Rust changes are minimal -- replacing a stub with a real HTTP client and adding a timeout config field.

**Primary recommendation:** Use `optimum`'s `main_export` function for programmatic ONNX export, FastAPI with lifespan pattern for the web framework, and `asyncio.Lock` per model_id for concurrent request deduplication. On the Rust side, use `reqwest::Client` with a pre-configured timeout and `.post().json().send().await` pattern.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- D-01: Synchronous long-poll API. Hephaestus POSTs `{"model_id": "org/model"}` and blocks until conversion finishes.
- D-02: Response includes S3 paths plus conversion metadata (architecture, original format, conversion duration, optimum version).
- D-03: Forge downloads PyTorch model from HuggingFace internally. Hephaestus sends only model_id.
- D-04: Configurable HTTP timeout via `FORGE_TIMEOUT_SECS` env var with sensible default (600s).
- D-05: Two-stage validation: `onnx.checker.check_model()` then dummy inference with `onnxruntime`.
- D-06: Validation failure returns error immediately, no automatic retry with different settings.
- D-07: Validate all artifacts before uploading -- model.onnx, tokenizer.json, config.json must exist and be parseable.
- D-08: In-memory lock per model_id. First request converts; subsequent requests block and receive same result.
- D-09: Single Forge replica for v1. In-memory lock sufficient.
- D-10: Sequential conversion queue -- one conversion at a time.
- D-11: Forge lives in `forge/` at the repo root.
- D-12: FastAPI for the web framework.
- D-13: Dockerfile included for the Forge service, no k8s manifests.
- D-14: `uv` + `pyproject.toml` for Python dependency management.

### Claude's Discretion
- FastAPI app structure (routers, middleware, error handling patterns)
- Optimum conversion flags and opset version selection
- Test inference input generation for validation (D-05 dummy inference)
- Rust `ForgeClient` return type struct design (field names, serde derives)
- How `ModelResolver` is generalized (trait object vs generic parameter)
- S3 upload implementation details in Python (boto3 patterns, multipart upload thresholds)

### Deferred Ideas (OUT OF SCOPE)
- Horizontal Forge scaling with distributed locking
- Automatic retry with fallback optimum settings on conversion failure
- Auto-regressive seq2seq decode support in converted models
- Full PyTorch output comparison validation
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| FORG-01 | Forge is a persistent Python service that converts HuggingFace models to ONNX format via `optimum` | Standard stack (FastAPI + optimum), architecture patterns, code examples |
| FORG-02 | Forge uploads converted ONNX files to S3 after conversion | S3 upload patterns (boto3 + TransferConfig), architecture patterns |
| FORG-03 | Forge exposes an API that Hephaestus calls when S3 and HuggingFace both lack ONNX files | API contract design, reqwest client patterns, ForgeClient generalization |
| FORG-04 | Forge validates converted ONNX model integrity before uploading to S3 | Two-stage validation pattern (onnx.checker + onnxruntime inference) |
</phase_requirements>

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| ONNX model conversion | Forge (Python service) | -- | Requires Python ecosystem (optimum, transformers, torch) |
| Conversion validation | Forge (Python service) | -- | Uses Python onnx.checker + onnxruntime |
| S3 upload of converted files | Forge (Python service) | -- | Forge is self-contained; uploads immediately after validation |
| Conversion request dispatch | Hephaestus (Rust binary) | -- | reqwest HTTP client calls Forge API |
| S3 download of converted files | Hephaestus (Rust binary) | -- | Existing resolver S3 tier handles download after Forge reports success |
| Concurrent request deduplication | Forge (Python service) | -- | In-memory asyncio.Lock per model_id |

## Standard Stack

### Core (Python -- Forge Service)
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| fastapi | >=0.115 | Web framework | Async, auto-generated OpenAPI docs, Pydantic validation. Locked by D-12. [ASSUMED] |
| uvicorn | >=0.30 | ASGI server | Standard production server for FastAPI. [ASSUMED] |
| optimum[onnxruntime] | >=2.0 | ONNX export | HuggingFace's official conversion library. `main_export` handles model loading, export, and validation. [CITED: huggingface.co/docs/optimum/en/exporters/onnx/usage_guides/export_a_model] |
| onnx | >=1.16 | Model validation | `onnx.checker.check_model()` validates graph structure. [CITED: onnx.ai/onnx/api/checker.html] |
| onnxruntime | >=1.28 | Inference validation | Runs dummy inference to catch runtime errors graph-only validation misses. [ASSUMED] |
| boto3 | >=1.35 | S3 uploads | Official AWS SDK for Python. Standard for S3 operations. [ASSUMED] |
| pydantic | >=2.0 | Data validation | Request/response models. Ships with FastAPI. [ASSUMED] |

### Core (Rust -- Client Changes)
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| reqwest | 0.13 | HTTP client | Already in workspace. POST with JSON body and configurable timeout. [VERIFIED: crates.io registry] |

### Supporting (Python)
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| structlog | >=24.0 | Structured logging | JSON logs for k8s log aggregation, consistent with Hephaestus patterns |
| transformers | (transitive) | Model loading | Required by optimum for model architecture detection |
| torch | (transitive) | PyTorch weights | Required by optimum for loading source model before export |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| FastAPI | Flask | FastAPI has native async, Pydantic validation, auto-docs. Flask requires extensions for each. D-12 locks FastAPI. |
| optimum main_export | ORTModelForSequenceClassification.from_pretrained(export=True) | main_export is the recommended programmatic API, supports all model types, includes built-in validation |
| boto3 upload_file | aiobotocore | boto3 is simpler for sequential uploads. Forge converts one model at a time (D-10), so async S3 has no benefit. |
| structlog | stdlib logging | structlog produces structured JSON by default, less configuration needed |

**Installation (Python):**
```bash
cd forge/
uv init
uv add "fastapi>=0.115" "uvicorn[standard]>=0.30" "optimum[onnxruntime]>=2.0" "onnx>=1.16" "onnxruntime>=1.28" "boto3>=1.35" "pydantic>=2.0" "structlog>=24.0"
uv add --dev "pytest>=8.0" "httpx>=0.27" "moto[s3]>=5.0"
```

**Version verification:**
- optimum: 2.3.0 (latest on PyPI, verified 2026-08-26)
- fastapi: 0.141.1 (latest on PyPI, verified 2026-08-26)
- uvicorn: 0.52.4 (latest on PyPI, verified 2026-08-26)
- onnxruntime: 1.29.0 (latest on PyPI, verified 2026-08-26)
- onnx: 1.22.0 (latest on PyPI, verified 2026-08-26)
- boto3: 1.43.80 (latest on PyPI, verified 2026-08-26)
- pydantic: 2.13.4 (latest on PyPI, verified 2026-08-26)
- reqwest (Rust): 0.13.4 (latest on crates.io, verified 2026-08-26)

## Package Legitimacy Audit

> All Python packages flagged as SUS by the automated tool due to PyPI not exposing weekly download counts. Manual review confirms all packages are from major organizations with verified GitHub repos and years of production use.

| Package | Registry | Age | Downloads | Source Repo | Verdict | Disposition |
|---------|----------|-----|-----------|-------------|---------|-------------|
| optimum | PyPI | 4+ yrs | millions/mo | github.com/huggingface/optimum | [SUS] | Approved -- HuggingFace official, tooling limitation |
| fastapi | PyPI | 6+ yrs | millions/mo | github.com/fastapi/fastapi | [SUS] | Approved -- industry standard, tooling limitation |
| uvicorn | PyPI | 7+ yrs | millions/mo | github.com/Kludex/uvicorn | [SUS] | Approved -- standard ASGI server, tooling limitation |
| boto3 | PyPI | 10+ yrs | millions/mo | github.com/boto/boto3 | [SUS] | Approved -- AWS official SDK, tooling limitation |
| onnx | PyPI | 7+ yrs | millions/mo | onnx.ai | [SUS] | Approved -- ONNX Foundation official, tooling limitation |
| onnxruntime | PyPI | 6+ yrs | millions/mo | onnxruntime.ai | [SUS] | Approved -- Microsoft official, tooling limitation |
| pydantic | PyPI | 7+ yrs | millions/mo | github.com/pydantic/pydantic | [SUS] | Approved -- industry standard, tooling limitation |
| reqwest | crates.io | 10 yrs | 13M/wk | github.com/seanmonstar/reqwest | [OK] | Approved |

**Packages removed due to [SLOP] verdict:** none
**Packages flagged as suspicious [SUS]:** All Python packages flagged due to PyPI download-count tooling limitation. All are from verified organizations (HuggingFace, AWS, Microsoft, Pydantic, FastAPI). No `checkpoint:human-verify` needed -- these are industry-standard packages.

## Architecture Patterns

### System Architecture Diagram

```
                    Hephaestus Pod (Rust)                          Forge Pod (Python)
               +---------------------------+                +---------------------------+
               |                           |                |                           |
  Inference    |  ModelResolver            |   HTTP POST    |  FastAPI App              |
  Request  --> |    Tier 1: S3 Cache       | -- miss -->    |    /convert endpoint      |
               |    Tier 2: HuggingFace    |   {"model_id"} |      |                    |
               |    Tier 3: Forge Client --+--------------->|      v                    |
               |                           |                |  ConversionQueue          |
               |  On Forge success:        |                |    Lock per model_id      |
               |    Download from S3       |   HTTP 200     |    Sequential exec        |
               |    Load ONNX + serve  <---+----------------+      |                    |
               |                           |   {s3_paths,   |      v                    |
               +---------------------------+    metadata}   |  optimum main_export()    |
                                                            |      |                    |
                                                            |      v                    |
                                                            |  Validation               |
                                                            |    onnx.checker           |
                                                            |    onnxruntime inference  |
                                                            |      |                    |
                                                            |      v                    |
                                                            |  boto3 S3 upload          |
                                                            |                           |
                                                            +---------------------------+
                                                                       |
                                                                       v
                                                              +------------------+
                                                              |  S3 Model Cache  |
                                                              +------------------+
```

### Recommended Project Structure (Forge)
```
forge/
├── pyproject.toml         # uv project config, all dependencies
├── uv.lock                # Lockfile for reproducible builds
├── Dockerfile             # Multi-stage: builder + runtime
├── src/
│   └── forge/
│       ├── __init__.py
│       ├── main.py        # FastAPI app, lifespan, health endpoints
│       ├── api.py         # /convert endpoint router
│       ├── converter.py   # optimum export + validation logic
│       ├── storage.py     # boto3 S3 upload logic
│       ├── queue.py       # Sequential queue + per-model-id locking
│       ├── models.py      # Pydantic request/response models
│       └── config.py      # Settings from env vars
└── tests/
    ├── conftest.py
    ├── test_api.py        # HTTP endpoint tests with httpx
    ├── test_converter.py  # Conversion logic unit tests
    └── test_storage.py    # S3 upload tests with moto
```

### Pattern 1: Sequential Conversion Queue with Per-Model Locking (D-08, D-10)
**What:** An asyncio-based queue that processes one conversion at a time while deduplicating concurrent requests for the same model.
**When to use:** When multiple Hephaestus pods might simultaneously request the same model conversion.
**Example:**
```python
# Source: D-08, D-10 from CONTEXT.md
import asyncio
from collections import defaultdict

class ConversionQueue:
    def __init__(self):
        self._semaphore = asyncio.Semaphore(1)  # D-10: one at a time
        self._locks: dict[str, asyncio.Lock] = defaultdict(asyncio.Lock)
        self._results: dict[str, ConversionResult] = {}

    async def convert(self, model_id: str) -> ConversionResult:
        lock = self._locks[model_id]
        async with lock:
            # D-08: If already converted, return cached result
            if model_id in self._results:
                return self._results[model_id]
            # D-10: Only one conversion at a time
            async with self._semaphore:
                result = await self._do_convert(model_id)
                self._results[model_id] = result
                return result
```

### Pattern 2: Two-Stage Validation (D-05)
**What:** Validate ONNX model first with graph checker, then with a dummy inference pass.
**When to use:** After every `main_export` call, before uploading to S3.
**Example:**
```python
# Source: D-05 from CONTEXT.md, onnx.ai/onnx/api/checker.html
import onnx
import onnxruntime as ort
import numpy as np

def validate_model(model_path: str, tokenizer_path: str) -> None:
    """Two-stage validation: graph check + dummy inference."""
    # Stage 1: Graph structure validation
    model = onnx.load(model_path)
    onnx.checker.check_model(model)

    # Stage 2: Dummy inference pass
    session = ort.InferenceSession(model_path)
    input_names = [inp.name for inp in session.get_inputs()]
    # Generate dummy inputs matching expected shapes
    dummy_inputs = {}
    for inp in session.get_inputs():
        shape = [1 if d is None or isinstance(d, str) else d for d in inp.shape]
        if inp.type == "tensor(int64)":
            dummy_inputs[inp.name] = np.ones(shape, dtype=np.int64)
        else:
            dummy_inputs[inp.name] = np.ones(shape, dtype=np.float32)
    # If this raises, the model is corrupt
    session.run(None, dummy_inputs)
```

### Pattern 3: Rust ForgeClient with reqwest (D-01, D-04)
**What:** Real HTTP client replacing StubForgeClient.
**When to use:** When `FORGE_URL` is configured in the environment.
**Example:**
```rust
// Source: reqwest docs, existing ForgeClient trait pattern
use std::time::Duration;
use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
struct ConvertRequest {
    model_id: String,
}

#[derive(Deserialize)]
pub struct ForgeResponse {
    pub s3_paths: Vec<String>,
    pub metadata: ConversionMetadata,
}

#[derive(Deserialize)]
pub struct ConversionMetadata {
    pub architecture: String,
    pub original_format: String,
    pub conversion_duration_secs: f64,
    pub optimum_version: String,
}

pub struct HttpForgeClient {
    client: Client,
    base_url: String,
}

impl HttpForgeClient {
    pub fn new(base_url: &str, timeout_secs: u64) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .build()
            .expect("failed to build reqwest client");
        Self {
            client,
            base_url: base_url.to_string(),
        }
    }
}

impl ForgeClient for HttpForgeClient {
    async fn convert(&self, model_id: &str) -> Result<Vec<String>, ResolveError> {
        let url = format!("{}/convert", self.base_url);
        let resp = self.client
            .post(&url)
            .json(&ConvertRequest { model_id: model_id.to_string() })
            .send()
            .await
            .map_err(|e| ResolveError::ForgeConversion {
                model_id: model_id.to_string(),
                reason: e.to_string(),
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(ResolveError::ForgeConversion {
                model_id: model_id.to_string(),
                reason: format!("HTTP {status}: {body}"),
            });
        }

        let forge_resp: ForgeResponse = resp.json().await
            .map_err(|e| ResolveError::ForgeConversion {
                model_id: model_id.to_string(),
                reason: format!("invalid response: {e}"),
            })?;

        Ok(forge_resp.s3_paths)
    }
}
```

### Pattern 4: ModelResolver Generalization
**What:** Replace concrete `StubForgeClient` field with generic type parameter bounded by `ForgeClient`.
**When to use:** Enables both stub (no Forge configured) and real client (Forge URL set) at compile time.
**Example:**
```rust
// Source: existing resolver.rs pattern
pub struct ModelResolver<F: ForgeClient = StubForgeClient> {
    cache_dir: PathBuf,
    s3_client: Option<aws_sdk_s3::Client>,
    s3_bucket: Option<String>,
    s3_prefix: Option<String>,
    forge: F,
}
```
**Recommendation:** Use a generic parameter with default type (`StubForgeClient`) rather than `Box<dyn ForgeClient>`. This avoids heap allocation and dynamic dispatch overhead, keeps the existing test code compiling unchanged, and the binary entry point selects the concrete type at build time based on config. The tradeoff is monomorphization, but there are only two implementors (stub + HTTP), so code size impact is negligible.

### Anti-Patterns to Avoid
- **Blocking the event loop during conversion:** `optimum` export is CPU-bound. Run it in a thread pool via `asyncio.to_thread()` or `run_in_executor()` so the FastAPI event loop remains responsive for health checks and concurrent lock waiters.
- **Uploading before validating:** Always run both validation stages (D-05) before any S3 upload. A corrupt model in S3 cache poisons all future pods.
- **Retry on validation failure:** Per D-06, validation failures return immediately. No exponential backoff or alternative settings.
- **Storing conversion results permanently in memory:** The per-model result cache should be bounded or use TTL. For v1 single-replica this is acceptable since restarts clear it, but document the assumption.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| ONNX export | Custom torch.onnx.export wrapper | `optimum.exporters.onnx.main_export` | Handles opset selection, dynamic axes, model-specific configs, tokenizer export, and validation in one call |
| Model graph validation | Manual protobuf parsing | `onnx.checker.check_model()` | Checks IR version, opset imports, node validity, type inference -- hundreds of edge cases |
| S3 multipart upload | Manual chunking | `boto3 upload_file` with `TransferConfig` | Handles multipart threshold, concurrency, retry, and progress tracking |
| HTTP client with retry | Manual loop in Rust | `reqwest::Client` with timeout | Connection pooling, TLS, redirect handling, JSON serde built in |
| Request/response validation | Manual dict checking | Pydantic models with FastAPI | Auto-validation, serialization, OpenAPI schema generation |
| Async locking per key | Custom lock map | `collections.defaultdict(asyncio.Lock)` | Standard library pattern, no external deps needed |

**Key insight:** The conversion pipeline (download model -> export to ONNX -> validate -> upload to S3) is a sequential chain where each step has a well-tested library. The Forge's value is orchestrating these libraries with proper error handling, not reimplementing any of them.

## Common Pitfalls

### Pitfall 1: optimum export blocks the event loop
**What goes wrong:** `main_export()` downloads PyTorch weights and runs ONNX export, which is CPU-intensive (minutes for large models). If called directly in an async handler, the entire FastAPI server becomes unresponsive.
**Why it happens:** FastAPI/uvicorn uses a single-threaded event loop by default for async handlers.
**How to avoid:** Wrap the conversion in `asyncio.to_thread()` so it runs in the default thread pool executor. The sequential semaphore (D-10) ensures only one conversion runs at a time regardless.
**Warning signs:** Health check endpoints stop responding during conversion.

### Pitfall 2: Large model files exceed memory during validation
**What goes wrong:** `onnx.load()` loads the entire model into memory. For models > 2GB, this fails or causes OOM.
**Why it happens:** ONNX protobuf has a 2GB limit on in-memory model representation.
**How to avoid:** Use `onnx.checker.check_model("path/to/model.onnx")` with a file path string instead of loading the model object first. For the inference validation, onnxruntime's `InferenceSession` memory-maps the file, so it handles large models fine.
**Warning signs:** OOM kills during validation of large models.

### Pitfall 3: reqwest timeout includes connection + response time
**What goes wrong:** The `timeout()` on reqwest `Client` is a total timeout covering DNS, connection, TLS handshake, AND the full response body. For long-poll (D-01), the entire conversion must complete within this timeout.
**Why it happens:** reqwest has a single timeout that covers the entire request lifecycle.
**How to avoid:** Set `FORGE_TIMEOUT_SECS` high enough for the largest expected model (default 600s). The Forge should also have its own internal timeout to prevent unbounded conversions, returning a 504 if exceeded.
**Warning signs:** Timeout errors on large model conversions even when the Forge is healthy.

### Pitfall 4: ForgeClient trait return type change breaks existing tests
**What goes wrong:** Changing `ForgeClient::convert()` return type from `Vec<String>` to a new struct breaks the mockall-generated `MockForgeClient` and all existing tests in resolver.rs.
**Why it happens:** mockall re-generates mock implementations based on trait signatures.
**How to avoid:** Update the trait signature, update all mock expectations in tests simultaneously, and use the new `ForgeResponse` struct consistently. The mock should return `Ok(ForgeResponse { s3_paths: vec![...], metadata: ... })`.
**Warning signs:** Compiler errors in test modules after trait change.

### Pitfall 5: Forge uploads to wrong S3 prefix
**What goes wrong:** Forge uploads to `s3://bucket/model_id/model.onnx` but Hephaestus's resolver looks for `s3://bucket/{prefix}/{model_id}/model.onnx`. Resolver returns cache miss despite the file existing.
**Why it happens:** S3 prefix (`S3_PREFIX`) must be consistent between Forge and Hephaestus.
**How to avoid:** The Forge must accept an `S3_PREFIX` env var and prepend it to all uploads, matching exactly what `ModelResolver` expects. Document this contract explicitly.
**Warning signs:** Forge reports success but resolver still reports S3 miss.

### Pitfall 6: tokenizer.json not exported by main_export
**What goes wrong:** `main_export` exports the ONNX model but may not always export `tokenizer.json` in the expected format or location.
**Why it happens:** Some model architectures export the tokenizer differently or require explicit `save_pretrained()` call.
**How to avoid:** After `main_export`, explicitly call `tokenizer.save_pretrained(output_dir)` to ensure `tokenizer.json` is present. Validate its existence in D-07 artifact check.
**Warning signs:** Hephaestus fails to load tokenizer from the converted model directory.

## Code Examples

### Forge convert endpoint (FastAPI)
```python
# Source: D-01, D-02, D-12 from CONTEXT.md
from fastapi import FastAPI, HTTPException
from pydantic import BaseModel
import time

class ConvertRequest(BaseModel):
    model_id: str

class ConversionMetadata(BaseModel):
    architecture: str
    original_format: str
    conversion_duration_secs: float
    optimum_version: str

class ConvertResponse(BaseModel):
    s3_paths: list[str]
    metadata: ConversionMetadata

@app.post("/convert", response_model=ConvertResponse)
async def convert(request: ConvertRequest):
    try:
        result = await queue.convert(request.model_id)
        return result
    except ConversionError as e:
        raise HTTPException(status_code=500, detail=str(e))
```

### optimum main_export usage
```python
# Source: huggingface.co/docs/optimum/en/exporters/onnx/usage_guides/export_a_model
from optimum.exporters.onnx import main_export
from transformers import AutoTokenizer
import tempfile
import os

async def do_convert(model_id: str) -> str:
    """Export model to ONNX. Returns path to output directory."""
    output_dir = tempfile.mkdtemp(prefix="forge-")
    # main_export handles: model download, architecture detection,
    # opset selection, dynamic axes, and ONNX export
    main_export(
        model_name_or_path=model_id,
        output=output_dir,
        task="auto",  # auto-detect task from model architecture
    )
    # Ensure tokenizer is saved alongside model
    tokenizer = AutoTokenizer.from_pretrained(model_id)
    tokenizer.save_pretrained(output_dir)
    return output_dir
```

### boto3 S3 upload with TransferConfig
```python
# Source: docs.aws.amazon.com/boto3/latest/guide/s3.html
import boto3
from boto3.s3.transfer import TransferConfig
import os

def upload_model_to_s3(
    local_dir: str,
    bucket: str,
    prefix: str,
    model_id: str,
) -> list[str]:
    """Upload all model artifacts to S3. Returns list of S3 keys."""
    s3 = boto3.client("s3")
    config = TransferConfig(
        multipart_threshold=100 * 1024 * 1024,  # 100MB
        max_concurrency=4,
    )
    uploaded_keys = []
    for filename in os.listdir(local_dir):
        filepath = os.path.join(local_dir, filename)
        if not os.path.isfile(filepath):
            continue
        s3_key = f"{prefix}/{model_id}/{filename}" if prefix else f"{model_id}/{filename}"
        s3.upload_file(filepath, bucket, s3_key, Config=config)
        uploaded_keys.append(s3_key)
    return uploaded_keys
```

### Rust Config extension (D-04)
```rust
// Source: existing config.rs pattern
/// Forge conversion service timeout in seconds (default: 600, env `FORGE_TIMEOUT_SECS`).
/// Covers the entire conversion request lifecycle including model download,
/// ONNX export, validation, and S3 upload.
#[serde(default = "default_forge_timeout_secs")]
pub forge_timeout_secs: u64,

fn default_forge_timeout_secs() -> u64 {
    600
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `torch.onnx.export()` manual | `optimum.exporters.onnx.main_export()` | optimum 1.0+ (2022) | Handles architecture-specific export configs automatically |
| `from_transformers=True` | `export=True` in ORTModel | optimum 1.7+ | Simplified API, same underlying mechanism |
| `@app.on_event("startup")` | `lifespan` context manager | FastAPI 0.95+ | Cleaner resource management, recommended pattern |
| pip + requirements.txt | uv + pyproject.toml + uv.lock | 2024+ | 10-100x faster installs, reproducible lockfile |
| optimum (monolith) | optimum-onnx (split package) | optimum 2.0+ | Leaner install for ONNX-only usage |

**Deprecated/outdated:**
- `@app.on_event("startup/shutdown")`: Replaced by lifespan context manager in FastAPI 0.95+
- `from_transformers=True`: Still works but `export=True` is the current API
- `pip install`: Works but uv is significantly faster and produces lockfiles natively

## Assumptions Log

> List all claims tagged `[ASSUMED]` in this research.

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | FastAPI >=0.115 supports lifespan pattern | Standard Stack | Low -- lifespan available since 0.95, well-documented |
| A2 | optimum[onnxruntime] pulls in onnxruntime as dependency | Standard Stack | Low -- standard extras pattern for optimum |
| A3 | onnxruntime InferenceSession works for validation of any model main_export produces | Architecture Patterns | Medium -- some models may need specific inputs beyond dummy ones |
| A4 | structlog for Python structured logging | Standard Stack | Low -- can use stdlib logging if preferred, no hard dependency |
| A5 | boto3 >= 1.35 TransferConfig API unchanged | Standard Stack | Low -- stable API for years |
| A6 | uvicorn >= 0.30 compatible with fastapi >= 0.115 | Standard Stack | Low -- standard pairing |

## Open Questions (RESOLVED)

1. **optimum opset version selection** — RESOLVED
   - What we know: `main_export` auto-selects opset based on model architecture. Minimum opset 13 for T5-like models.
   - What's unclear: Whether to pin a specific opset or let optimum auto-detect. Auto-detect is more flexible but less reproducible.
   - Recommendation: Let optimum auto-detect (default behavior). If specific models fail, add an optional `opset` parameter to the Forge API later.
   - Resolution: Plans use `task="auto"` with no explicit opset pin. Optimum auto-detects per model architecture.

2. **Forge internal timeout** — RESOLVED
   - What we know: Hephaestus has `FORGE_TIMEOUT_SECS` (D-04). The Forge itself has no explicit conversion timeout mentioned.
   - What's unclear: Should the Forge also enforce an internal timeout on `main_export`?
   - Recommendation: Add a `CONVERSION_TIMEOUT_SECS` env var in the Forge (default 540s, less than the 600s client default) so the Forge returns a clear timeout error rather than the client timing out ambiguously.
   - Resolution: Plan 05-01 config.py adds `conversion_timeout_secs` (default 540). Plan 05-01 queue.py wraps conversion in `asyncio.wait_for` with this value.

3. **HuggingFace authentication in the Forge** — RESOLVED
   - What we know: Some models require authentication (gated models). Forge downloads from HF internally (D-03).
   - What's unclear: Whether to support `HF_TOKEN` env var for gated model access.
   - Recommendation: Support `HF_TOKEN` as an optional env var. `optimum` and `transformers` respect it automatically when set.
   - Resolution: Plan 05-01 config.py adds `hf_token: Optional[str]` field. Optimum/transformers pick it up from the environment automatically.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| Python 3.13 | Forge service | Yes | 3.13.13 | Any Python >= 3.11 works |
| uv | Forge dependency management | Yes | 0.9.13 | pip + requirements.txt (not recommended) |
| Docker | Forge Dockerfile build | Yes | 29.6.1 | -- |
| pip | Fallback package management | Yes | 26.0.1 | -- |

**Missing dependencies with no fallback:** None
**Missing dependencies with fallback:** None

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework (Rust) | cargo test (built-in) |
| Framework (Python) | pytest >= 8.0 |
| Config file (Rust) | None needed |
| Config file (Python) | `forge/pyproject.toml` [tool.pytest.ini_options] |
| Quick run (Rust) | `cargo test -p hephaestus-resolve` |
| Quick run (Python) | `cd forge && uv run pytest tests/ -x` |
| Full suite | `cargo test --workspace && cd forge && uv run pytest` |

### Phase Requirements -> Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| FORG-01 | Forge converts HF model to ONNX via optimum | integration | `cd forge && uv run pytest tests/test_converter.py -x` | Wave 0 |
| FORG-02 | Forge uploads converted files to S3 | unit (moto) | `cd forge && uv run pytest tests/test_storage.py -x` | Wave 0 |
| FORG-03 | Forge exposes API that Hephaestus calls | unit + integration | `cd forge && uv run pytest tests/test_api.py -x` | Wave 0 |
| FORG-04 | Forge validates ONNX integrity before upload | unit | `cd forge && uv run pytest tests/test_converter.py::test_validation -x` | Wave 0 |
| -- | Rust ForgeClient sends correct HTTP request | unit (mockall) | `cargo test -p hephaestus-resolve forge` | Wave 0 |
| -- | ModelResolver uses real client when configured | unit | `cargo test -p hephaestus-resolve resolver` | Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test -p hephaestus-resolve` (Rust) + `cd forge && uv run pytest -x` (Python)
- **Per wave merge:** `cargo test --workspace` + full pytest suite
- **Phase gate:** Both suites green + end-to-end test (Hephaestus -> Forge -> S3 -> inference)

### Wave 0 Gaps
- [ ] `forge/tests/conftest.py` -- shared fixtures (mock S3, test model IDs)
- [ ] `forge/tests/test_api.py` -- covers FORG-03
- [ ] `forge/tests/test_converter.py` -- covers FORG-01, FORG-04
- [ ] `forge/tests/test_storage.py` -- covers FORG-02
- [ ] `forge/pyproject.toml` -- pytest config, dependencies
- [ ] Framework install: `uv sync` in forge directory

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | No | Internal service-to-service only |
| V3 Session Management | No | Stateless API |
| V4 Access Control | No | No multi-tenant, internal only |
| V5 Input Validation | Yes | Pydantic model validation on model_id format |
| V6 Cryptography | No | No secrets handled beyond env vars |

### Known Threat Patterns for this stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Arbitrary model download (SSRF-like) | Tampering | Validate model_id format matches `org/model` pattern; reject URLs, paths |
| Large model OOM (resource exhaustion) | DoS | Sequential queue (D-10) limits to one conversion; pod memory limits in k8s |
| S3 path injection | Tampering | Validate model_id has no `..` or special chars (reuse resolver's validate_model_id logic) |
| Dependency confusion in optimum export | Tampering | Pin all dependencies via uv.lock; use only public PyPI |
| Timeout bypass (unbounded conversion) | DoS | FORGE_TIMEOUT_SECS on client; internal CONVERSION_TIMEOUT_SECS on server |

## Sources

### Primary (HIGH confidence)
- Existing codebase: `crates/hephaestus-resolve/src/forge.rs` -- ForgeClient trait contract
- Existing codebase: `crates/hephaestus-resolve/src/resolver.rs` -- ModelResolver structure
- Existing codebase: `crates/hephaestus/src/config.rs` -- Config pattern with envy
- PyPI registry -- verified all package versions exist (2026-08-26)
- crates.io registry -- reqwest 0.13.4 verified (2026-08-26)

### Secondary (MEDIUM confidence)
- [HuggingFace optimum ONNX export docs](https://huggingface.co/docs/optimum/en/exporters/onnx/usage_guides/export_a_model) -- main_export usage
- [ONNX checker API docs](https://onnx.ai/onnx/api/checker.html) -- check_model function
- [AWS boto3 S3 transfer guide](https://docs.aws.amazon.com/boto3/latest/guide/s3.html) -- TransferConfig usage

### Tertiary (LOW confidence)
- WebSearch results for uv workflow, FastAPI lifespan pattern, reqwest usage

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - all packages verified on registries, well-established ecosystem choices
- Architecture: HIGH - patterns derived from locked decisions in CONTEXT.md and existing codebase
- Pitfalls: HIGH - based on direct knowledge of the libraries and confirmed API behaviors
- Security: MEDIUM - standard patterns for internal services, no novel threat vectors

**Research date:** 2026-08-26
**Valid until:** 2026-09-26 (stable ecosystem, no fast-moving dependencies)
