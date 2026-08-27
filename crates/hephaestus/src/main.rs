//! Hephaestus binary entry point.
//!
//! Startup sequence: load config from env vars, initialize tracing,
//! detect model profile, construct the appropriate pipeline, run a
//! warmup inference pass, flip readiness, and start the HTTP server
//! with graceful shutdown.

mod config;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use hephaestus_api::{AppState, Batcher, batcher_loop, build_router};
use hephaestus_core::{
    ClassifierPipeline, EmbeddingsPipeline, ExecutionProvider, ModelProfile, PipelineKind,
    Seq2SeqPipeline, TokenClassifierPipeline, detect_profile,
};
use hephaestus_resolve::{HttpForgeClient, ModelResolver};

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // 1. Load typed configuration from environment variables.
    //    Config must be loaded before tracing init so we can use LOG_LEVEL.
    let config = config::Config::from_env()?;
    config.validate()?;
    let ep: ExecutionProvider = config.parsed_execution_provider()?;

    // 2. Initialize telemetry: structured JSON logging + conditional OTel export (D-11).
    //    Must be called inside the tokio runtime (after #[tokio::main]) because the
    //    OTel batch span processor spawns a background tokio task (Pitfall 1).
    hephaestus_api::telemetry::init(
        &config.log_level,
        config.otel_exporter_otlp_endpoint.as_deref(),
    )?;
    tracing::info!(
        model_id = %config.model_id,
        execution_provider = %ep,
        port = config.port,
        request_timeout_secs = config.request_timeout_secs,
        shutdown_timeout_secs = config.shutdown_timeout_secs,
        storage_type = %config.storage_type,
        storage_bucket = ?config.storage_bucket,
        storage_prefix = ?config.storage_prefix,
        forge_url = ?config.forge_url,
        forge_timeout_secs = config.forge_timeout_secs,
        model_profile = ?config.model_profile,
        "configuration loaded"
    );

    // 2b. Install Prometheus metrics recorder (OBSV-01).
    let metrics_handle = hephaestus_api::install_recorder()?;
    tracing::info!("prometheus metrics recorder installed");

    // 2c. Build OpenDAL storage operator from config (D-01, D-02, D-05).
    let operator = if config.storage_type == "none" {
        None
    } else {
        let mut cfg = HashMap::new();
        if let Some(ref bucket) = config.storage_bucket {
            cfg.insert("bucket".to_string(), bucket.clone());
        }
        if let Some(ref region) = config.storage_region {
            cfg.insert("region".to_string(), region.clone());
        }
        // D-04: STORAGE_PREFIX/STORAGE_ROOT -> OpenDAL "root" config.
        // For fs: root is STORAGE_ROOT, optionally joined with STORAGE_PREFIX.
        // For cloud backends: STORAGE_PREFIX becomes "/{prefix}" root.
        if config.storage_type == "fs" {
            // validate() ensures storage_root is Some for fs.
            let root = config.storage_root.as_deref().unwrap();
            match config.storage_prefix.as_deref() {
                Some(prefix) => cfg.insert("root".to_string(), format!("{root}/{prefix}")),
                None => cfg.insert("root".to_string(), root.to_string()),
            };
        } else if let Some(ref prefix) = config.storage_prefix {
            cfg.insert("root".to_string(), format!("/{prefix}"));
        }

        let op = opendal::Operator::via_iter(config.storage_type.as_str(), cfg.into_iter())
            .context("failed to build storage operator")?
            .layer(opendal::layers::RetryLayer::new().with_max_times(3));
        Some(op)
    };
    tracing::info!(storage_type = %config.storage_type, "storage operator constructed");

    // 3. Resolve model directory: local override (MODEL_PATH) or automatic resolution.
    let model_dir = if config.model_path.is_some() {
        // Local path override -- preserves backward compatibility.
        config.model_dir()?
    } else {
        // Automatic resolution: storage cache -> HuggingFace -> Forge (RSLV-05).
        // When FORGE_URL is set, use HttpForgeClient; otherwise StubForgeClient.
        // The two branches produce different generic types, so we resolve
        // inside each branch and return the PathBuf.
        if let Some(ref forge_url) = config.forge_url {
            let forge_client = HttpForgeClient::new(forge_url, config.forge_timeout_secs)
                .context("failed to create Forge HTTP client")?;
            let resolver = ModelResolver::new_with_client(
                operator.clone(),
                forge_client,
            )
            .await
            .context("failed to construct model resolver")?;

            resolver
                .resolve(&config.model_id)
                .await
                .context("failed to resolve model")?
        } else {
            let resolver = ModelResolver::new_with_stub(
                operator.clone(),
            )
            .await
            .context("failed to construct model resolver")?;

            resolver
                .resolve(&config.model_id)
                .await
                .context("failed to resolve model")?
        }
    };
    tracing::info!(
        model_id = %config.model_id,
        model_dir = %model_dir.display(),
        "model directory resolved"
    );

    // 3b. Detect model profile from config.json (D-01, D-02).
    let config_json_path = model_dir.join("config.json");
    let config_json_text = std::fs::read_to_string(&config_json_path)
        .context("failed to read config.json from model directory")?;
    let model_config: serde_json::Value = serde_json::from_str(&config_json_text)
        .context("failed to parse config.json")?;
    let profile = detect_profile(&model_config, config.model_profile.as_deref())
        .context("failed to detect model profile")?;
    tracing::info!(
        model_id = %config.model_id,
        profile = ?profile,
        "model profile detected"
    );

    // 4. Construct the appropriate pipeline based on detected profile (D-03).
    let pipeline_kind = match profile {
        ModelProfile::Classifier => {
            let pipeline = ClassifierPipeline::new(&model_dir, &ep)
                .context("failed to construct classifier pipeline")?;
            tracing::info!("classifier pipeline constructed");
            PipelineKind::Classifier(pipeline)
        }
        ModelProfile::Embeddings => {
            let pipeline = EmbeddingsPipeline::new(&model_dir, &ep)
                .context("failed to construct embeddings pipeline")?;
            tracing::info!("embeddings pipeline constructed");
            PipelineKind::Embeddings(pipeline)
        }
        ModelProfile::Seq2Seq => {
            let pipeline = Seq2SeqPipeline::new(&model_dir, &ep)
                .context("failed to construct seq2seq pipeline")?;
            tracing::info!("seq2seq pipeline constructed");
            PipelineKind::Seq2Seq(pipeline)
        }
        ModelProfile::TokenClassifier => {
            let pipeline = TokenClassifierPipeline::new(&model_dir, &ep)
                .context("failed to construct token classifier pipeline")?;
            tracing::info!("token classifier pipeline constructed");
            PipelineKind::TokenClassifier(pipeline)
        }
    };

    // 5. Build shared state with optional batcher (D-07).
    let batcher_handle = if config.batch_enabled {
        let (batcher, receiver) = Batcher::new(config.batch_max_size as usize);
        Some((batcher, receiver))
    } else {
        None
    };

    let (batcher_opt, batcher_rx) = match batcher_handle {
        Some((batcher, receiver)) => (Some(batcher), Some(receiver)),
        None => (None, None),
    };

    let state = Arc::new(AppState::new(
        pipeline_kind,
        config.model_id.clone(),
        Duration::from_secs(config.request_timeout_secs),
        metrics_handle,
        batcher_opt,
    ));

    // 5b. Spawn batcher background task if batching is enabled (D-06).
    if let Some(receiver) = batcher_rx {
        let batcher_state = state.clone();
        let max_batch_size = config.batch_max_size as usize;
        let max_wait = Duration::from_millis(config.batch_max_wait_ms);
        tokio::spawn(batcher_loop(receiver, batcher_state, max_batch_size, max_wait));
        tracing::info!(
            batch_max_size = config.batch_max_size,
            batch_max_wait_ms = config.batch_max_wait_ms,
            "dynamic batching enabled"
        );
    } else {
        tracing::info!("dynamic batching disabled");
    }

    // 6. Run warmup inference pass (CORE-03), then flip readiness.
    //    Warmup is a performance optimization (pre-warms caches), not a
    //    correctness gate. Failure logs a warning but does not crash the pod.
    {
        let warmup_text = config
            .warmup_input
            .as_deref()
            .unwrap_or("This is a warmup inference pass.");
        // Read lock for prepare (tokenization), write lock for execute (inference).
        // Mirrors the handler's read/write split pattern (SC-02).
        let prepared = {
            let pipeline = state.read_pipeline().await;
            pipeline.prepare(warmup_text.to_string())
        };
        match prepared {
            Ok(prepared) => {
                let mut pipeline = state.write_pipeline().await;
                match pipeline.execute(prepared) {
                    Ok(_output) => {
                        tracing::info!(
                            model_id = %config.model_id,
                            "warmup inference complete"
                        );
                    }
                    Err(e) => {
                        tracing::warn!(
                            model_id = %config.model_id,
                            error = %e,
                            "warmup inference failed, continuing without warmup"
                        );
                    }
                }
            }
            Err(e) => {
                tracing::warn!(
                    model_id = %config.model_id,
                    error = %e,
                    "warmup prepare failed, continuing without warmup"
                );
            }
        }
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
    //
    // Uses a Notify instead of std::process::exit(1) so destructors
    // and OTel shutdown run cleanly when the drain timeout expires.
    let force_shutdown = Arc::new(tokio::sync::Notify::new());
    let shutdown_timeout = Duration::from_secs(config.shutdown_timeout_secs);
    let watchdog_state = state.clone();
    let watchdog_notify = force_shutdown.clone();
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
            "drain timeout exceeded, forcing server shutdown"
        );
        watchdog_notify.notify_one();
    });

    // Graceful shutdown waits for either the OS signal or the watchdog timeout.
    let server_notify = force_shutdown.clone();
    let server_state = state.clone();
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            tokio::select! {
                () = shutdown_signal(server_state) => {},
                () = server_notify.notified() => {},
            }
        })
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
