//! Hephaestus binary entry point.
//!
//! Startup sequence: load config from env vars, initialize tracing,
//! construct the classifier pipeline, run a warmup inference pass,
//! flip readiness, and start the HTTP server with graceful shutdown.

mod config;

use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use hephaestus_api::{AppState, build_router};
use hephaestus_core::{ClassifierPipeline, Pipeline};

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // 1. Load typed configuration from environment variables.
    //    Config must be loaded before tracing init so we can use LOG_LEVEL.
    let config = config::Config::from_env()?;

    // 2. Initialize telemetry: structured JSON logging + conditional OTel export (D-11).
    //    Must be called inside the tokio runtime (after #[tokio::main]) because the
    //    OTel batch span processor spawns a background tokio task (Pitfall 1).
    hephaestus_api::telemetry::init(
        &config.log_level,
        config.otel_exporter_otlp_endpoint.as_deref(),
    )?;
    tracing::info!(
        model_id = %config.model_id,
        execution_provider = %config.execution_provider,
        port = config.port,
        request_timeout_secs = config.request_timeout_secs,
        shutdown_timeout_secs = config.shutdown_timeout_secs,
        "configuration loaded"
    );

    // 2b. Install Prometheus metrics recorder (OBSV-01).
    let metrics_handle = hephaestus_api::install_recorder()?;
    tracing::info!("prometheus metrics recorder installed");

    // 3. Resolve and validate model directory (T-01-01).
    let model_dir = config.model_dir()?;

    // 4. Construct the classifier pipeline.
    let pipeline = ClassifierPipeline::new(&model_dir)
        .context("failed to construct classifier pipeline")?;
    tracing::info!("classifier pipeline constructed");

    // 5. Build shared state (readiness starts false per D-05).
    let state = Arc::new(AppState::new(
        pipeline,
        config.model_id.clone(),
        Duration::from_secs(config.request_timeout_secs),
        metrics_handle,
    ));

    // 6. Run warmup inference pass (CORE-03), then flip readiness.
    {
        let warmup_text = config
            .warmup_input
            .as_deref()
            .unwrap_or("This is a warmup inference pass.");
        let mut pipeline = state.lock_pipeline().await;
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
    }
    state.set_ready(true);
    tracing::info!("warmup complete, readiness enabled");

    // 7. Start HTTP server with graceful shutdown.
    let addr = format!("0.0.0.0:{}", config.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .context("failed to bind TCP listener")?;
    tracing::info!(%addr, "listening");

    let app = build_router(state.clone());

    // Spawn drain-timeout watchdog (D-13).
    let shutdown_timeout = Duration::from_secs(config.shutdown_timeout_secs);
    let watchdog_state = state.clone();
    tokio::spawn(async move {
        // Wait until readiness is flipped to false (shutdown signal received).
        loop {
            if !watchdog_state.is_ready() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        // Grace period for in-flight requests to drain.
        tokio::time::sleep(shutdown_timeout).await;
        tracing::warn!(
            timeout_secs = shutdown_timeout.as_secs(),
            "drain timeout exceeded, forcing exit"
        );
        std::process::exit(1);
    });

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal(state.clone()))
        .await
        .context("HTTP server error")?;

    tracing::info!("server shut down");

    // 8. Flush pending OTel spans before exit.
    hephaestus_api::telemetry::shutdown();

    Ok(())
}

/// Wait for a shutdown signal (Ctrl-C or SIGTERM).
///
/// On signal receipt, flips readiness to false so the k8s readiness
/// probe returns 503 and the load balancer stops routing new traffic
/// while in-flight requests drain (D-07).
async fn shutdown_signal(state: Arc<AppState>) {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }

    tracing::info!("shutdown signal received, draining connections");
    state.set_ready(false);
}
