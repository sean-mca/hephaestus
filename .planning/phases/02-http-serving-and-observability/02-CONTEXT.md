# Phase 2: HTTP Serving and Observability - Context

**Gathered:** 2026-08-23
**Status:** Ready for planning

<domain>
## Phase Boundary

Turn the current standalone Rust binary into a deployable HTTP service with health probes, Prometheus metrics, structured JSON logging, and OpenTelemetry distributed tracing. After this phase, Hephaestus can be deployed as a Kubernetes pod, accept HTTP inference requests, and be monitored in production. No gRPC (v2), no model resolution, no additional profiles.

</domain>

<decisions>
## Implementation Decisions

### Request/Response Shape
- **D-01:** Minimal flat JSON request body: `{"text": "input string"}`. Single field matches the one-model-per-pod design.
- **D-02:** Response includes model metadata: `{"label": "POSITIVE", "score": 0.95, "model_id": "distilbert-...", "latency_ms": 12}`. Aids debugging without requiring external correlation.
- **D-03:** Structured error responses with machine-parseable codes: `{"error": {"code": "TOKENIZATION_FAILED", "message": "..."}}`. HTTP status codes: 400 (bad request), 422 (unprocessable), 500 (internal), 503 (not ready), 504 (timeout).
- **D-04:** New `hephaestus-api` crate owns the HTTP layer (axum routes, handlers, middleware). Separates serving concerns from the binary crate.

### Health Probe Behavior
- **D-05:** Readiness probe returns 200 only after the warmup inference pass succeeds. Liveness probe returns 200 immediately on startup.
- **D-06:** Health endpoints include diagnostics: `{"status": "ok", "model_id": "...", "uptime_s": 3600}`. K8s ignores the body but it's useful for operators hitting the endpoint directly.
- **D-07:** On SIGTERM, readiness flips to 503 immediately so k8s stops routing new traffic while in-flight requests drain.

### Metrics and Observability
- **D-08:** Per-stage timing breakdown: separate histograms for tokenization, inference, and postprocessing latency. Total request latency as a summary metric.
- **D-09:** Deep-module style timer abstraction — a shared timing utility that each pipeline stage calls through the same interface, hiding the metrics recording plumbing. Callers should never interact with the metrics crate directly.
- **D-10:** All metrics carry a `model_id` label. One model per pod means one label value per pod — cardinality is controlled.
- **D-11:** Full OTel wiring with conditional activation. The OTLP exporter layer registers only when `OTEL_EXPORTER_OTLP_ENDPOINT` is set. Without it, structured JSON logs still capture span/trace context via tracing-subscriber. No feature flags or if/else in hot paths — just a layered subscriber with an optional OTel layer.

### Shutdown and Timeout Policy
- **D-12:** Request timeout default: 30 seconds. Configurable via `REQUEST_TIMEOUT_SECS` env var.
- **D-13:** Graceful shutdown drain: 30 seconds. Configurable via `SHUTDOWN_TIMEOUT_SECS` env var. Matches the request timeout so any in-flight request finishes within one timeout window.
- **D-14:** Timeout responses: HTTP 504 Gateway Timeout with `{"error": {"code": "INFERENCE_TIMEOUT", "message": "..."}}`.
- **D-15:** New env vars added to the Config struct via envy, following the same pattern as Phase 1 (D-11, D-12): `PORT`, `REQUEST_TIMEOUT_SECS`, `SHUTDOWN_TIMEOUT_SECS`, `OTEL_EXPORTER_OTLP_ENDPOINT`.

### Claude's Discretion
No areas deferred to Claude's discretion — all decisions made explicitly.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Project Context
- `.planning/PROJECT.md` — Core value, constraints, key decisions
- `.planning/REQUIREMENTS.md` — Full v1 requirement list with traceability (Phase 2 requirements: API-01 through API-04, CORE-04, OBSV-01 through OBSV-03)
- `.planning/ROADMAP.md` — Phase 2 goal, success criteria, dependency chain

### Phase 1 Context
- `.planning/phases/01-core-inference-engine/01-CONTEXT.md` — Workspace layout (D-01 through D-03), Pipeline trait design (D-04 through D-07), config pattern (D-11 through D-13)

### Coding Rules
- `rules/` — Full directory of Rust coding rules. All code must comply.

### Existing Code (Phase 1 output)
- `crates/hephaestus/src/main.rs` — Current synchronous entry point; Phase 2 converts to async tokio and adds HTTP server start at line 56
- `crates/hephaestus/src/config.rs` — Config struct loaded via envy; Phase 2 extends with new env vars
- `crates/hephaestus-core/src/pipeline.rs` — Pipeline trait and ClassifierPipeline; Phase 2 wraps this behind HTTP handlers
- `crates/hephaestus-core/src/error.rs` — CoreError type; Phase 2 maps these to HTTP error responses
- `Cargo.toml` — Workspace deps; tracing and tokio already declared

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `ClassifierPipeline` and `Pipeline` trait: HTTP handlers wrap `prepare()` + `execute()` calls
- `Config` struct with envy: extend with new fields for port, timeouts, OTel endpoint
- `CoreError` enum: map variants to HTTP status codes and structured error responses
- tracing-subscriber with JSON format and env-filter: extend with optional OTel layer

### Established Patterns
- Config from env vars via envy (D-11) — all new config follows this pattern
- thiserror for library errors, anyhow for binary-level errors
- Pipeline trait with prepare/execute two-step API
- Structured JSON logging via tracing-subscriber

### Integration Points
- `main.rs` line 56: "Phase 2 adds HTTP server start here" — replace the current synchronous flow with async tokio runtime + axum server
- `Cargo.toml` workspace deps: add axum, tower, metrics, metrics-exporter-prometheus, opentelemetry crates
- New `hephaestus-api` crate joins the workspace alongside existing crates

</code_context>

<specifics>
## Specific Ideas

- Timer abstraction should follow Ousterhout deep-module principle: a single API that records timing for any pipeline stage, hiding histogram registration, label attachment, and metrics crate internals. Every stage calls the same interface.

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

</deferred>

---

*Phase: 2-HTTP Serving and Observability*
*Context gathered: 2026-08-23*
