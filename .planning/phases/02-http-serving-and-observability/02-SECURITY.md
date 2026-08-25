---
phase: 02
slug: http-serving-and-observability
status: verified
threats_open: 0
asvs_level: 1
created: 2026-08-24
---

# Phase 02 — Security

> Per-phase security contract: threat register, accepted risks, and audit trail.

---

## Trust Boundaries

| Boundary | Description | Data Crossing |
|----------|-------------|---------------|
| HTTP ingress | External HTTP requests to /infer, /healthz/*, /metrics | User-supplied text (POST body), health metadata, operational metrics |
| ONNX Runtime | Rust process to native ONNX Runtime shared library | Tokenized tensor data (int64 arrays) |
| Filesystem | Process to local model directory (MODEL_PATH) | Model files (.onnx, tokenizer.json, config.json) |
| OTel export | Process to OTLP collector endpoint | Trace spans with model_id, latency, status (no request text) |

---

## Threat Register

| Threat ID | Category | Component | Severity | Disposition | Mitigation | Status |
|-----------|----------|-----------|----------|-------------|------------|--------|
| T-02-W0 | — | Test scaffolding | low | accept | No production code in wave 0 | closed |
| T-02-01 | Tampering | POST /infer body | medium | mitigate | Serde typed deserialization via `Json<InferRequest>` + tokenizer truncation at 512 tokens | closed |
| T-02-02 | DoS | POST /infer oversized body | medium | mitigate | axum 0.8 default 2MB body limit (DefaultBodyLimit) | closed |
| T-02-03 | DoS | Runaway inference | high | mitigate | `tokio::time::timeout` at handlers.rs:74 + tokenizer truncation at pipeline.rs:127 | closed |
| T-02-04 | Info Disclosure | Error responses | medium | mitigate | Server errors return generic "internal server error"; detailed error logged server-side only (error.rs:79-81) | closed |
| T-02-05 | Spoofing | Health probe abuse | low | accept | Internal k8s service, no auth required | closed |
| T-02-SC-W1 | Tampering | Cargo dependencies (wave 1) | medium | accept | Packages verified in RESEARCH.md | closed |
| T-02-06 | Info Disclosure | GET /metrics | low | accept | Operational data only (counters, histograms) | closed |
| T-02-07 | Info Disclosure | OTel trace spans | low | accept | `skip(state, req)` on #[instrument]; only `text_len` field exposed, no request text | closed |
| T-02-08 | DoS | /metrics scrape overhead | low | accept | Lightweight PrometheusHandle::render() read | closed |
| T-02-SC-W2 | Tampering | Cargo dependencies (wave 2) | medium | accept | Packages verified in RESEARCH.md | closed |
| T-02-03-01 | Info Disclosure | Structured log content | low | mitigate | `skip(state, req)` at handlers.rs:53; logs emit model_id/latency/status only | closed |
| T-02-03-02 | DoS | TraceLayer overhead | low | accept | Negligible per-request overhead | closed |

*Status: open / closed / open — below high threshold (non-blocking)*
*Severity: critical > high > medium > low — only open threats at or above high count toward threats_open*

---

## Accepted Risks Log

| Risk ID | Threat Ref | Rationale | Accepted By | Date |
|---------|------------|-----------|-------------|------|
| AR-01 | T-02-W0 | Wave 0 is test scaffolding only, no production code | Claude | 2026-08-24 |
| AR-02 | T-02-05 | Health probes are internal k8s endpoints, no auth needed | Claude | 2026-08-24 |
| AR-03 | T-02-SC-W1 | Dependencies verified against crates.io during research phase | Claude | 2026-08-24 |
| AR-04 | T-02-06 | /metrics exposes only operational counters/histograms, no PII | Claude | 2026-08-24 |
| AR-05 | T-02-07 | OTel spans carry model_id and timing only; request text excluded via skip() | Claude | 2026-08-24 |
| AR-06 | T-02-08 | PrometheusHandle::render() is a cheap in-memory read | Claude | 2026-08-24 |
| AR-07 | T-02-SC-W2 | Dependencies verified against crates.io during research phase | Claude | 2026-08-24 |
| AR-08 | T-02-03-02 | TraceLayer adds negligible overhead per request | Claude | 2026-08-24 |

---

## Security Audit Trail

| Audit Date | Threats Total | Closed | Open | Run By |
|------------|---------------|--------|------|--------|
| 2026-08-24 | 13 | 13 | 0 | Claude (L1 grep-depth, short-circuit) |

---

## Sign-Off

- [x] All threats have a disposition (mitigate / accept / transfer)
- [x] Accepted risks documented in Accepted Risks Log
- [x] `threats_open: 0` confirmed
- [x] `status: verified` set in frontmatter

**Approval:** verified 2026-08-24
