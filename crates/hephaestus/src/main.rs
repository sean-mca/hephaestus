//! Hephaestus binary entry point.
//!
//! Startup sequence: load config from env vars, construct the
//! classifier pipeline, run a warmup inference pass, report ready.

mod config;

use anyhow::Context;
use hephaestus_core::{ClassifierPipeline, Pipeline};

fn main() -> Result<(), anyhow::Error> {
    // 1. Load typed configuration from environment variables.
    //    Config must be loaded before tracing init so we can use LOG_LEVEL.
    let config = config::Config::from_env()?;

    // 2. Initialize structured JSON logging.
    //    RUST_LOG takes precedence (from_default_env); if unset, fall back
    //    to the LOG_LEVEL env var captured in config (D-12).
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&config.log_level));
    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .json()
        .init();
    tracing::info!(
        model_id = %config.model_id,
        execution_provider = %config.execution_provider,
        "configuration loaded"
    );

    // 3. Resolve and validate model directory (T-01-01).
    let model_dir = config.model_dir()?;

    // 4. Construct the classifier pipeline.
    let mut pipeline = ClassifierPipeline::new(&model_dir)
        .context("failed to construct classifier pipeline")?;
    tracing::info!("classifier pipeline constructed");

    // 5. Run warmup inference pass (CORE-03).
    let warmup_text = config
        .warmup_input
        .as_deref()
        .unwrap_or("This is a warmup inference pass.");
    let prepared = pipeline
        .prepare(warmup_text.to_string())
        .context("warmup: failed to prepare input")?;
    let output = pipeline
        .execute(prepared)
        .context("warmup: failed to run inference")?;
    tracing::info!(
        label = %output.label,
        score = output.score,
        "warmup inference complete"
    );

    // 6. Report ready (Phase 2 adds HTTP server start here).
    tracing::info!("hephaestus ready");

    Ok(())
}
