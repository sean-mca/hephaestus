---
phase: quick
plan: 01
type: execute
wave: 1
depends_on: []
files_modified:
  - Cargo.toml
  - crates/hephaestus-core/Cargo.toml
  - crates/hephaestus-core/src/ep.rs
  - crates/hephaestus-core/src/lib.rs
  - crates/hephaestus-core/src/pipeline.rs
  - crates/hephaestus/Cargo.toml
  - crates/hephaestus/src/config.rs
  - crates/hephaestus/src/main.rs
autonomous: true
requirements: []
must_haves:
  truths:
    - "EXECUTION_PROVIDER=cuda starts session with CUDA EP registered (when compiled with cuda feature)"
    - "EXECUTION_PROVIDER=cpu (default) always works regardless of features"
    - "Missing feature at compile time produces a clear startup error, not a silent fallback"
    - "GPU EP runtime unavailability falls back to CPU gracefully (ort handles this)"
  artifacts:
    - crates/hephaestus-core/src/ep.rs
  key_links:
    - "Config.execution_provider string -> ExecutionProvider enum -> ort SessionBuilder.with_execution_providers()"
    - "Cargo features: hephaestus/cuda -> hephaestus-core/cuda -> ort/cuda"
---

<objective>
Wire the EXECUTION_PROVIDER config value through to the ort session builder so that GPU execution providers (CUDA, TensorRT, CoreML) are actually registered when requested. Currently the env var is accepted and logged but never used -- sessions always run on CPU.

Purpose: Enable GPU-accelerated inference when the binary is compiled with the appropriate feature flag and the hardware is available.
Output: ExecutionProvider enum in hephaestus-core, cargo feature gates, and full wiring from config through pipeline constructors to session creation.
</objective>

<execution_context>
@$HOME/.claude/gsd-core/workflows/execute-plan.md
@$HOME/.claude/gsd-core/templates/summary.md
</execution_context>

<context>
@.planning/PROJECT.md
@.planning/ROADMAP.md
@.planning/STATE.md
@crates/hephaestus-core/src/pipeline.rs
@crates/hephaestus-core/src/lib.rs
@crates/hephaestus/src/config.rs
@crates/hephaestus/src/main.rs
@crates/hephaestus-core/Cargo.toml
@crates/hephaestus/Cargo.toml
@Cargo.toml
</context>

<tasks>

<task type="auto">
  <name>Task 1: Add cargo features and create ExecutionProvider enum</name>
  <files>
    crates/hephaestus-core/Cargo.toml,
    crates/hephaestus/Cargo.toml,
    crates/hephaestus-core/src/ep.rs,
    crates/hephaestus-core/src/lib.rs
  </files>
  <action>
    1. In crates/hephaestus-core/Cargo.toml, add a [features] section with three features:
       - cuda = ["ort/cuda"]
       - tensorrt = ["ort/tensorrt"]
       - coreml = ["ort/coreml"]

    2. In crates/hephaestus/Cargo.toml, add a [features] section forwarding to hephaestus-core:
       - cuda = ["hephaestus-core/cuda"]
       - tensorrt = ["hephaestus-core/tensorrt"]
       - coreml = ["hephaestus-core/coreml"]

    3. Create crates/hephaestus-core/src/ep.rs with:

       - A four-variant enum ExecutionProvider: Cpu, Cuda, TensorRt, CoreMl.
       - Derive Debug, Clone, Copy, PartialEq, Eq.
       - Implement std::fmt::Display (lowercase: "cpu", "cuda", "tensorrt", "coreml").
       - Implement std::str::FromStr that accepts case-insensitive values "cpu", "cuda", "tensorrt", "coreml". Return CoreError::Config for unrecognized values listing the accepted set.
       - A method to_ort_providers(&self) -> Result of Vec of ort::execution_providers::ExecutionProviderDispatch, CoreError.
         - Cpu variant: return empty vec (ort uses CPU by default when no EPs registered).
         - Cuda variant: behind cfg(feature = "cuda"), construct ort::execution_providers::CUDAExecutionProvider::default().build() and return in a vec. When feature not compiled in, return CoreError::Config with message naming the missing cargo feature.
         - TensorRt variant: same pattern with cfg(feature = "tensorrt") and TensorRTExecutionProvider.
         - CoreMl variant: same pattern with cfg(feature = "coreml") and CoreMLExecutionProvider.

    4. In crates/hephaestus-core/src/lib.rs:
       - Add `pub mod ep;`
       - Add `pub use ep::ExecutionProvider;` to the re-exports.

    Note: CoreError needs a Config variant if it does not already have one. Check error.rs -- if missing, add `Config(String)` with Display impl "configuration error: {0}". If a suitable variant already exists, use that instead.
  </action>
  <verify>
    <automated>cd /Users/seanmcauliffe/Repos/minerva/blacksmith && cargo check --workspace 2>&1 | tail -5</automated>
  </verify>
  <done>ExecutionProvider enum exists with FromStr and to_ort_providers. Cargo features defined. Workspace compiles without features enabled (CPU path only).</done>
</task>

<task type="auto">
  <name>Task 2: Wire ExecutionProvider through session builder, pipelines, config, and main</name>
  <files>
    crates/hephaestus-core/src/pipeline.rs,
    crates/hephaestus/src/config.rs,
    crates/hephaestus/src/main.rs
  </files>
  <action>
    1. In crates/hephaestus-core/src/pipeline.rs:
       - Import ExecutionProvider from crate::ep.
       - Update load_session_and_tokenizer signature to accept ep: &amp;ExecutionProvider as second parameter.
       - After creating Session::builder() and setting optimization level, call .with_execution_providers(ep.to_ort_providers()?) before .commit_from_file(). The to_ort_providers call should happen before the builder chain so the error can propagate. Chain: Session::builder()?.with_optimization_level(Level3)?.with_execution_providers(providers)?.commit_from_file(&amp;model_path)?
       - Update all four pipeline constructors (ClassifierPipeline::new, EmbeddingsPipeline::new, Seq2SeqPipeline::new, TokenClassifierPipeline::new) to accept ep: &amp;ExecutionProvider and pass it through to load_session_and_tokenizer.

    2. In crates/hephaestus/src/config.rs:
       - Add use hephaestus_core::ExecutionProvider.
       - Add a method parsed_execution_provider(&amp;self) -> Result of ExecutionProvider, anyhow::Error that calls self.execution_provider.parse::&lt;ExecutionProvider&gt;() and wraps the error with anyhow context.
       - In validate(), add a check that parsed_execution_provider() succeeds (fail fast at startup with a clear message naming allowed values).
       - Update the existing test helper config_with_model_path to keep execution_provider as "cpu" (no change needed, it already does this).
       - Add a test that validates parsing of all four EP strings and rejection of an invalid one.

    3. In crates/hephaestus/src/main.rs:
       - After config.validate(), call config.parsed_execution_provider() and bind to a local `ep` variable.
       - Log the parsed EP in the existing configuration-loaded tracing::info (replace %config.execution_provider with ?ep or %ep).
       - Pass &amp;ep to each pipeline constructor: ClassifierPipeline::new(&amp;model_dir, &amp;ep), EmbeddingsPipeline::new(&amp;model_dir, &amp;ep), Seq2SeqPipeline::new(&amp;model_dir, &amp;ep), TokenClassifierPipeline::new(&amp;model_dir, &amp;ep).
  </action>
  <verify>
    <automated>cd /Users/seanmcauliffe/Repos/minerva/blacksmith && cargo test --workspace 2>&1 | tail -20</automated>
  </verify>
  <done>EXECUTION_PROVIDER env var is parsed into typed enum, validated at startup, and passed through to the ort session builder. All existing tests pass. CPU default path works without any cargo features enabled.</done>
</task>

</tasks>

<threat_model>
## Trust Boundaries

| Boundary | Description |
|----------|-------------|
| env var -> config | EXECUTION_PROVIDER comes from environment (pod spec) |

## STRIDE Threat Register

| Threat ID | Category | Component | Severity | Disposition | Mitigation |
|-----------|----------|-----------|----------|-------------|------------|
| T-EP-01 | Tampering | EXECUTION_PROVIDER env var | low | mitigate | Strict allowlist in FromStr (cpu, cuda, tensorrt, coreml only); reject anything else at startup |
| T-EP-02 | Denial of Service | GPU EP without hardware | low | accept | ort falls back to CPU gracefully at runtime; no crash risk |
</threat_model>

<verification>
cargo test --workspace passes with no new failures.
cargo check --workspace compiles on default features (CPU only).
</verification>

<success_criteria>
- ExecutionProvider enum with Cpu/Cuda/TensorRt/CoreMl variants exists in hephaestus-core
- Cargo features cuda/tensorrt/coreml gate ort EP features
- Session builder calls with_execution_providers when non-CPU EP requested
- Config validation rejects unknown EP values at startup
- All existing tests pass unchanged
- Binary compiled without GPU features still works with EXECUTION_PROVIDER=cpu (default)
</success_criteria>

<output>
Create `.planning/quick/260826-ren-wire-execution-provider-config-to-ort-se/260826-ren-SUMMARY.md` when done
</output>
