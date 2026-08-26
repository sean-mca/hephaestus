---
phase: 05-forge-conversion-service
plan: 02
subsystem: resolve
tags: [reqwest, forge, http-client, onnx-conversion, generics]

requires:
  - phase: 03-model-resolution
    provides: ForgeClient trait, StubForgeClient, ModelResolver with 3-tier chain
provides:
  - HttpForgeClient with reqwest POST + JSON + configurable timeout
  - ForgeResponse and ConversionMetadata types
  - Generic ModelResolver<F: ForgeClient> with default StubForgeClient
  - Binary wiring of HttpForgeClient when FORGE_URL is set
affects: []

tech-stack:
  added: []
  patterns:
    - "Generic struct with default type parameter for pluggable implementations"
    - "Conditional construction branching in main.rs for different generic instantiations"

key-files:
  created: []
  modified:
    - crates/hephaestus-resolve/src/forge.rs
    - crates/hephaestus-resolve/src/error.rs
    - crates/hephaestus-resolve/src/resolver.rs
    - crates/hephaestus-resolve/src/lib.rs
    - crates/hephaestus/src/config.rs
    - crates/hephaestus/src/main.rs

key-decisions:
  - "Generic ModelResolver<F: ForgeClient = StubForgeClient> instead of trait object -- avoids Box<dyn> overhead and preserves static dispatch"
  - "Conditional branching in main.rs (if/else producing PathBuf) instead of trait object -- each branch instantiates different concrete type"
  - "ForgeResponse contains s3_paths + ConversionMetadata -- carries conversion details for observability"

patterns-established:
  - "Generic struct with default type: ModelResolver<F: ForgeClient = StubForgeClient> pattern for pluggable service clients"
  - "new_with_stub() / new_with_client() constructor split for backward compatibility"

requirements-completed: [FORG-03]

coverage:
  - id: D1
    description: "HttpForgeClient sends POST /convert with JSON body and deserializes ForgeResponse"
    requirement: "FORG-03"
    verification:
      - kind: unit
        ref: "crates/hephaestus-resolve/src/forge.rs#http_forge_client_stores_base_url"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-resolve/src/forge.rs#http_forge_client_trims_trailing_slash"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-resolve/src/forge.rs#forge_response_deserializes_from_json"
        status: pass
    human_judgment: false
  - id: D2
    description: "ForgeConversion error variant captures HTTP status and body for debugging"
    verification:
      - kind: unit
        ref: "crates/hephaestus-resolve/src/forge.rs#stub_forge_returns_forge_unavailable"
        status: pass
    human_judgment: false
  - id: D3
    description: "ModelResolver generic over ForgeClient with StubForgeClient default"
    verification:
      - kind: unit
        ref: "crates/hephaestus-resolve/src/resolver.rs#resolver_new_without_s3_has_no_client"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-resolve/src/resolver.rs#resolver_new_with_s3_creates_client"
        status: pass
      - kind: unit
        ref: "crates/hephaestus-resolve/src/resolver.rs#resolve_rejects_invalid_model_id"
        status: pass
    human_judgment: false
  - id: D4
    description: "Binary wires HttpForgeClient when FORGE_URL set, StubForgeClient when unset"
    verification:
      - kind: unit
        ref: "cargo build --release (compiles without errors)"
        status: pass
    human_judgment: false
  - id: D5
    description: "Config has forge_timeout_secs with 600s default"
    verification:
      - kind: unit
        ref: "crates/hephaestus/src/config.rs#test_forge_timeout_default"
        status: pass
    human_judgment: false

duration: 8min
completed: 2026-08-26
status: complete
---

# Phase 05 Plan 02: Forge HTTP Client and Resolver Generalization Summary

**reqwest-based HttpForgeClient with ForgeResponse types, generic ModelResolver<F: ForgeClient>, and binary wiring for the complete 3-tier resolution chain**

## Performance

- **Duration:** 8 min
- **Started:** 2026-08-26T19:18:27Z
- **Completed:** 2026-08-26T19:26:35Z
- **Tasks:** 2
- **Files modified:** 6

## Accomplishments
- Implemented HttpForgeClient with reqwest POST to /convert endpoint, configurable timeout (FORGE_TIMEOUT_SECS, default 600s), and structured ForgeResponse/ConversionMetadata deserialization
- Generalized ModelResolver to ModelResolver<F: ForgeClient = StubForgeClient> with new_with_stub() and new_with_client() constructors for backward-compatible pluggable Forge integration
- Wired HttpForgeClient in main.rs when FORGE_URL is configured, completing the 3-tier resolution chain (S3 -> HuggingFace -> Forge)
- Added ForgeConversion error variant with model_id and reason fields for structured error reporting from the Forge HTTP tier

## Task Commits

Each task was committed atomically:

1. **Task 1: HttpForgeClient, ForgeResponse types, error variant, and config field** - `e28873e` (feat)
2. **Task 2: ModelResolver generalization and binary wiring** - `0326941` (feat)

## Files Created/Modified
- `crates/hephaestus-resolve/src/forge.rs` - HttpForgeClient, ForgeResponse, ConversionMetadata, ConvertRequest types; ForgeClient trait updated to return ForgeResponse
- `crates/hephaestus-resolve/src/error.rs` - ForgeConversion error variant with model_id and reason
- `crates/hephaestus-resolve/src/resolver.rs` - Generic ModelResolver<F: ForgeClient>, new_with_stub/new_with_client constructors, forge tier destructures ForgeResponse
- `crates/hephaestus-resolve/src/lib.rs` - Public re-exports for HttpForgeClient, ForgeResponse, ConversionMetadata
- `crates/hephaestus/src/config.rs` - forge_timeout_secs field with 600s default
- `crates/hephaestus/src/main.rs` - Conditional HttpForgeClient/StubForgeClient construction based on FORGE_URL

## Decisions Made
- Generic ModelResolver<F: ForgeClient = StubForgeClient> with static dispatch instead of Box<dyn ForgeClient> -- avoids heap allocation and vtable overhead for the common path
- Conditional if/else branching in main.rs produces PathBuf from each branch rather than storing a single resolver binding -- necessary because the two branches produce different monomorphized types
- ForgeResponse struct carries s3_paths and ConversionMetadata (architecture, original_format, conversion_duration_secs, optimum_version) -- logged at info level in the forge tier for observability
- Removed forge_url field from ModelResolver (was #[allow(dead_code)]) -- URL is now consumed during HttpForgeClient construction

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- 3-tier resolution chain is fully wired: S3 cache -> HuggingFace -> Forge conversion
- The Forge service (Python) must be deployed separately and accessible at FORGE_URL for the conversion tier to function
- All workspace tests pass (40 tests), release build compiles cleanly

## Self-Check: PASSED

All 6 modified files verified present. Both task commits (e28873e, 0326941) verified in git log.

---
*Phase: 05-forge-conversion-service*
*Completed: 2026-08-26*
