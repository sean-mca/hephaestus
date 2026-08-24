# Phase 2: HTTP Serving and Observability - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-08-23
**Phase:** 2-HTTP Serving and Observability
**Areas discussed:** Request/response shape, Health probe behavior, Metrics granularity, Shutdown and timeout policy

---

## Request/Response Shape

### Request Body Format

| Option | Description | Selected |
|--------|-------------|----------|
| Minimal flat JSON | `{"text": "input string"}` — single field, simplest possible | ✓ |
| Structured with options | `{"text": "...", "top_k": 3, "threshold": 0.5}` — allows callers to request multiple labels | |
| You decide | Let Claude pick the most appropriate format | |

**User's choice:** Minimal flat JSON
**Notes:** Matches one-model-per-pod design.

### Response Format

| Option | Description | Selected |
|--------|-------------|----------|
| Label + score only | `{"label": "POSITIVE", "score": 0.95}` | |
| Add model metadata | `{"label": "...", "score": 0.95, "model_id": "...", "latency_ms": 12}` | ✓ |
| You decide | Let Claude decide | |

**User's choice:** Add model metadata

### Error Response Format

| Option | Description | Selected |
|--------|-------------|----------|
| Simple JSON error | `{"error": "description"}` with HTTP status codes | |
| Structured with code | `{"error": {"code": "TOKENIZATION_FAILED", "message": "..."}}` | ✓ |
| You decide | Let Claude pick the error format | |

**User's choice:** Structured with code

### HTTP Crate Ownership

| Option | Description | Selected |
|--------|-------------|----------|
| In the binary crate | Add axum routes directly in crates/hephaestus/ | |
| New hephaestus-api crate | Separate crate for HTTP handlers, routes, middleware | ✓ |
| You decide | Let Claude decide | |

**User's choice:** New hephaestus-api crate

---

## Health Probe Behavior

### Readiness Gate

| Option | Description | Selected |
|--------|-------------|----------|
| Ready after warmup | Readiness 200 only after warmup inference pass succeeds | ✓ |
| Ready after model load | Gates on Session construction, warmup runs in background | |
| You decide | Let Claude pick | |

**User's choice:** Ready after warmup

### Health Endpoint Data

| Option | Description | Selected |
|--------|-------------|----------|
| Status codes only | 200 OK with empty body or `{"status": "ok"}` | |
| Include diagnostics | 200 with `{"status": "ok", "model_id": "...", "uptime_s": 3600}` | ✓ |
| You decide | Let Claude decide | |

**User's choice:** Include diagnostics

### Readiness During Shutdown

| Option | Description | Selected |
|--------|-------------|----------|
| Yes, flip readiness | On SIGTERM, readiness returns 503. K8s stops routing new requests. | ✓ |
| No, just stop listening | Close listener socket on SIGTERM | |
| You decide | Let Claude decide | |

**User's choice:** Yes, flip readiness

---

## Metrics Granularity

### Metric Depth

| Option | Description | Selected |
|--------|-------------|----------|
| Total latency only | One histogram for end-to-end request latency | |
| Per-stage breakdown | Separate histograms for tokenization, inference, postprocessing | ✓ |
| You decide | Let Claude pick | |

**User's choice:** Per-stage breakdown
**Notes:** User wants a deep-module style timer abstraction — a shared timing utility that each pipeline stage calls through the same API, hiding metrics plumbing. "Kinda like a John Ousterhout style class."

### Model ID Label

| Option | Description | Selected |
|--------|-------------|----------|
| Yes, model_id label | Every metric includes model_id | ✓ |
| No model_id label | Model identity from pod metadata/Prometheus relabeling | |
| You decide | Let Claude decide | |

**User's choice:** Yes, model_id label

### OTel Tracing Strategy

| Option | Description | Selected |
|--------|-------------|----------|
| Full OTel with conditional export | Wire up full OTel pipeline; OTLP exporter activates when OTEL_EXPORTER_OTLP_ENDPOINT is set; structured JSON logs always work | ✓ |

**User's choice:** Full OTel with conditional activation
**Notes:** User clarified: "can we do full otel but have it log what it can, without hacky code, for now?" — resolved as conditional OTel layer based on env var presence, no feature flags or if/else in hot paths.

---

## Shutdown and Timeout Policy

### Request Timeout Default

| Option | Description | Selected |
|--------|-------------|----------|
| 30 seconds | Conservative default, configurable via env var | ✓ |
| 10 seconds | Tighter default, fail fast | |
| You decide | Let Claude pick | |

**User's choice:** 30 seconds

### Graceful Drain Duration

| Option | Description | Selected |
|--------|-------------|----------|
| 30 seconds | Matches request timeout, aligns with k8s default terminationGracePeriodSeconds | ✓ |
| 15 seconds | Shorter drain, pods recycle faster | |
| You decide | Let Claude pick | |

**User's choice:** 30 seconds

### Timeout Error Response

| Option | Description | Selected |
|--------|-------------|----------|
| 408 Request Timeout | Standard timeout status | |
| 504 Gateway Timeout | More accurate — server itself timed out | ✓ |
| You decide | Let Claude pick | |

**User's choice:** 504 with INFERENCE_TIMEOUT error code

### Timeout Configurability

| Option | Description | Selected |
|--------|-------------|----------|
| Env var configurable | REQUEST_TIMEOUT_SECS and SHUTDOWN_TIMEOUT_SECS with defaults of 30 | ✓ |
| Hardcoded defaults | 30s for both, no override | |
| You decide | Let Claude decide | |

**User's choice:** Env var configurable

---

## Claude's Discretion

No areas deferred to Claude's discretion.

## Deferred Ideas

None — discussion stayed within phase scope.
