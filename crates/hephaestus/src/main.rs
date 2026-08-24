//! Hephaestus binary entry point.
//!
//! Startup sequence: load config from env vars, initialize tracing,
//! construct the classifier pipeline, run a warmup inference pass,
//! flip readiness, and start the HTTP server with graceful shutdown.

mod config;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Context;
use hephaestus_api::{AppState, build_router};
use hephaestus_core::{ClassifierPipeline, Pipeline};
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
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
        port = config.port,
        request_timeout_secs = config.request_timeout_secs,
        shutdown_timeout_secs = config.shutdown_timeout_secs,
        "configuration loaded"
    );

    // 3. Resolve and validate model directory (T-01-01).
    let model_dir = config.model_dir()?;

    // 4. Construct the classifier pipeline.
    let pipeline = ClassifierPipeline::new(&model_dir)
        .context("failed to construct classifier pipeline")?;
    tracing::info!("classifier pipeline constructed");

    // 5. Build shared state (readiness starts false per D-05).
    let state = Arc::new(AppState {
        pipeline: Mutex::new(pipeline),
        ready: AtomicBool::new(false),
        model_id: config.model_id.clone(),
        start_time: Instant::now(),
        request_timeout: Duration::from_secs(config.request_timeout_secs),
    });

    // 6. Run warmup inference pass (CORE-03), then flip readiness.
    {
        let warmup_text = config
            .warmup_input
            .as_deref()
            .unwrap_or("This is a warmup inference pass.");
        let mut pipeline = state.pipeline.lock().await;
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
    state.ready.store(true, Ordering::SeqCst);
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
            if !watchdog_state.ready.load(Ordering::SeqCst) {
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
    state.ready.store(false, Ordering::SeqCst);
}
