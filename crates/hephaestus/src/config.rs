//! Typed configuration loaded from environment variables via `envy`.
//!
//! All configuration comes from environment variables (D-11). This is
//! a k8s-only service -- no CLI parser, no config files.

use std::path::{Component, PathBuf};

use anyhow::{Context, bail};
use serde::Deserialize;

/// Runtime configuration deserialized from environment variables.
///
/// # Required
///
/// - `MODEL_ID` -- identifier for the model (e.g., `distilbert-base-uncased-finetuned-sst-2-english`).
///   The binary crashes with a clear error if this is missing (D-13).
///
/// # Optional
///
/// - `MODEL_PATH` -- absolute path to the local directory containing model files.
/// - `EXECUTION_PROVIDER` -- ONNX execution provider (default: `"cpu"`).
/// - `LOG_LEVEL` -- log verbosity (default: `"info"`).
/// - `WARMUP_INPUT` -- custom text for the warmup inference pass.
#[derive(Deserialize, Debug)]
pub struct Config {
    /// Model identifier (required).
    pub model_id: String,

    /// Local directory containing model files (optional).
    #[serde(default)]
    pub model_path: Option<String>,

    /// ONNX execution provider (default: `"cpu"`).
    #[serde(default = "default_ep")]
    pub execution_provider: String,

    /// Log level (default: `"info"`).
    #[serde(default = "default_log_level")]
    pub log_level: String,

    /// Custom warmup inference text (optional).
    #[serde(default)]
    pub warmup_input: Option<String>,

    /// HTTP server listen port (default: 8080, env `PORT`).
    #[serde(default = "default_port")]
    pub port: u16,

    /// Per-request inference timeout in seconds (default: 30, env `REQUEST_TIMEOUT_SECS`).
    #[serde(default = "default_request_timeout_secs")]
    pub request_timeout_secs: u64,

    /// Graceful shutdown drain timeout in seconds (default: 30, env `SHUTDOWN_TIMEOUT_SECS`).
    #[serde(default = "default_shutdown_timeout_secs")]
    pub shutdown_timeout_secs: u64,

    /// OpenTelemetry OTLP exporter endpoint (optional, env `OTEL_EXPORTER_OTLP_ENDPOINT`).
    /// When set, OTel tracing is activated. When absent, only structured JSON logs are emitted.
    ///
    /// Used by `hephaestus_api::telemetry::init` to conditionally activate OTel tracing.
    #[serde(default)]
    pub otel_exporter_otlp_endpoint: Option<String>,

    /// S3 bucket for model cache (optional, env `S3_BUCKET`, D-03).
    /// When set, the resolver checks S3 before HuggingFace.
    #[serde(default)]
    pub s3_bucket: Option<String>,

    /// S3 key prefix for model files (optional, env `S3_PREFIX`).
    /// Prepended to model ID when constructing S3 keys.
    #[serde(default)]
    pub s3_prefix: Option<String>,

    /// Forge conversion service URL (optional, env `FORGE_URL`, D-09).
    /// When set, enables the Forge conversion tier for models without ONNX exports.
    #[serde(default)]
    pub forge_url: Option<String>,
}

fn default_ep() -> String {
    "cpu".to_string()
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_port() -> u16 {
    8080
}

fn default_request_timeout_secs() -> u64 {
    30
}

fn default_shutdown_timeout_secs() -> u64 {
    30
}

impl Config {
    /// Load configuration from environment variables.
    ///
    /// # Errors
    ///
    /// Returns an error if `MODEL_ID` is not set or if any env var
    /// fails to deserialize into the expected type.
    pub fn from_env() -> Result<Self, anyhow::Error> {
        envy::from_env::<Self>().context("failed to load config from environment (MODEL_ID is required)")
    }

    /// Resolve and validate the model directory path.
    ///
    /// Validates that the path is absolute and contains no parent-directory
    /// traversal components (`..`) to mitigate T-01-01 path tampering.
    ///
    /// # Errors
    ///
    /// Returns an error if `MODEL_PATH` is not set, the path is relative,
    /// the path contains `..` components, or the path does not exist.
    pub fn model_dir(&self) -> Result<PathBuf, anyhow::Error> {
        let raw = self
            .model_path
            .as_deref()
            .context("MODEL_PATH is required (model resolution not yet implemented -- Phase 3)")?;

        let path = PathBuf::from(raw);

        // T-01-01: reject relative paths.
        if !path.is_absolute() {
            bail!("MODEL_PATH must be an absolute path, got: {raw}");
        }

        // T-01-01: reject parent-directory traversal.
        for component in path.components() {
            if matches!(component, Component::ParentDir) {
                bail!("MODEL_PATH must not contain '..' components, got: {raw}");
            }
        }

        // Validate that the directory exists.
        if !path.is_dir() {
            bail!("MODEL_PATH does not exist or is not a directory: {raw}");
        }

        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: construct a `Config` with the given model_path and sensible
    /// defaults for all other fields. Avoids going through envy so tests
    /// are deterministic and don't mutate process-wide env vars.
    fn config_with_model_path(model_path: Option<&str>) -> Config {
        Config {
            model_id: "test-model".to_string(),
            model_path: model_path.map(String::from),
            execution_provider: "cpu".to_string(),
            log_level: "info".to_string(),
            warmup_input: None,
            port: 8080,
            request_timeout_secs: 30,
            shutdown_timeout_secs: 30,
            otel_exporter_otlp_endpoint: None,
            s3_bucket: None,
            s3_prefix: None,
            forge_url: None,
        }
    }

    #[test]
    fn from_env_with_defaults_has_correct_defaults() {
        // Arrange -- set only MODEL_ID; rely on serde defaults for the rest.
        // Safety: env var mutation is process-global but acceptable in unit
        // tests that are not run in parallel with other env-dependent tests.
        unsafe { std::env::set_var("MODEL_ID", "test-model") };

        // Act
        let config = Config::from_env().expect("should load config with MODEL_ID set");

        // Assert
        assert_eq!(config.model_id, "test-model");
        assert_eq!(config.execution_provider, "cpu");
        assert_eq!(config.log_level, "info");
        assert!(config.model_path.is_none());
        assert!(config.warmup_input.is_none());

        // Cleanup
        unsafe { std::env::remove_var("MODEL_ID") };
    }

    #[test]
    fn model_dir_returns_error_when_model_path_is_none() {
        // Arrange
        let config = config_with_model_path(None);

        // Act
        let result = config.model_dir();

        // Assert
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("MODEL_PATH"), "error should mention MODEL_PATH: {msg}");
    }

    #[test]
    fn model_dir_rejects_relative_path() {
        // Arrange
        let config = config_with_model_path(Some("relative/path"));

        // Act
        let result = config.model_dir();

        // Assert
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("absolute"), "error should mention 'absolute': {msg}");
    }

    #[test]
    fn model_dir_rejects_parent_traversal() {
        // Arrange
        let config = config_with_model_path(Some("/tmp/models/../secret"));

        // Act
        let result = config.model_dir();

        // Assert
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains(".."), "error should mention '..': {msg}");
    }

    #[test]
    fn model_dir_accepts_valid_absolute_path() {
        // Arrange
        let tmpdir = tempfile::tempdir().expect("should create temp dir");
        let path_str = tmpdir.path().to_str().expect("path should be valid UTF-8");
        let config = config_with_model_path(Some(path_str));

        // Act
        let result = config.model_dir();

        // Assert
        let dir = result.expect("should accept valid absolute path");
        assert_eq!(dir, tmpdir.path());
    }

    #[test]
    fn model_dir_rejects_nonexistent_path() {
        // Arrange
        let config = config_with_model_path(Some("/nonexistent/path/to/model"));

        // Act
        let result = config.model_dir();

        // Assert
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(
            msg.contains("does not exist"),
            "error should mention 'does not exist': {msg}"
        );
    }
}
