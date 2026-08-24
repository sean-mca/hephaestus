//! Telemetry initialization with conditional OpenTelemetry export (D-11).
//!
//! [`init`] builds a layered tracing subscriber: a JSON formatting layer
//! for structured logs (OBSV-02), an environment filter, and an optional
//! OpenTelemetry layer that activates only when an OTLP endpoint is provided.
//!
//! [`shutdown`] flushes any pending OTel spans before process exit.
//!
//! # Important
//!
//! [`init`] **must** be called inside the tokio runtime (after `#[tokio::main]`)
//! because the OTel batch span processor spawns a tokio task (Pitfall 1).

use std::sync::OnceLock;

use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::trace::SdkTracerProvider;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

/// Global handle to the OTel tracer provider, used for shutdown.
static TRACER_PROVIDER: OnceLock<SdkTracerProvider> = OnceLock::new();

/// Initialize the tracing subscriber with structured JSON logging and
/// conditional OpenTelemetry export.
///
/// - `log_level`: default filter level (e.g., `"info"`). Overridden by
///   `RUST_LOG` env var if set.
/// - `otel_endpoint`: when `Some`, activates OTLP span export to the
///   given endpoint via gRPC (tonic). When `None`, only JSON logs are emitted.
///
/// # Errors
///
/// Returns an error if the OTLP span exporter fails to build or the
/// tracing subscriber fails to initialize.
pub fn init(log_level: &str, otel_endpoint: Option<&str>) -> Result<(), anyhow::Error> {
    // JSON fmt layer for structured logging (OBSV-02).
    let fmt_layer = tracing_subscriber::fmt::layer()
        .json()
        .with_target(true);

    // EnvFilter: RUST_LOG takes precedence; fall back to log_level parameter.
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(log_level));

    // Conditional OTel layer per D-11: Option<Layer> implements Layer,
    // passing through when None -- no feature flags or if/else in hot paths.
    let otel_layer = if let Some(endpoint) = otel_endpoint {
        let exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_tonic()
            .with_endpoint(endpoint)
            .build()
            .map_err(|e| anyhow::anyhow!("failed to build OTLP span exporter: {e}"))?;

        let provider = SdkTracerProvider::builder()
            .with_batch_exporter(exporter)
            .build();

        let tracer = provider.tracer("hephaestus");

        // Store provider globally so shutdown() can flush it.
        let _ = TRACER_PROVIDER.set(provider);

        let layer = tracing_opentelemetry::OpenTelemetryLayer::new(tracer);

        Some(layer)
    } else {
        None
    };

    tracing_subscriber::Registry::default()
        .with(env_filter)
        .with(fmt_layer)
        .with(otel_layer)
        .init();

    // Log after subscriber is installed so messages actually appear.
    if otel_endpoint.is_some() {
        tracing::info!("OpenTelemetry OTLP export enabled");
    } else {
        tracing::info!("OpenTelemetry export disabled (OTEL_EXPORTER_OTLP_ENDPOINT not set)");
    }

    Ok(())
}

/// Flush pending OTel spans before process exit.
///
/// Call this after the HTTP server stops and before the process exits
/// to ensure all buffered spans are exported. No-op if OTel was not enabled.
pub fn shutdown() {
    if let Some(provider) = TRACER_PROVIDER.get()
        && let Err(e) = provider.shutdown()
    {
        tracing::warn!(error = %e, "failed to shut down OTel tracer provider");
    }
}

#[cfg(test)]
mod tests {
    // Note: telemetry::init installs a global subscriber, so tests that
    // call it must not run in parallel with other init() calls. The
    // integration test in tests/tracing.rs covers this; here we only
    // verify that the module compiles and shutdown does not panic.

    #[test]
    fn shutdown_without_init_does_not_panic() {
        // Calling shutdown before init should be a no-op, not a panic.
        super::shutdown();
    }
}
