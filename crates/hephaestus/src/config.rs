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

    /// Forge conversion service timeout in seconds (default: 600, env `FORGE_TIMEOUT_SECS`).
    #[serde(default = "default_forge_timeout_secs")]
    pub forge_timeout_secs: u64,

    /// Optional model profile override (env `MODEL_PROFILE`, D-02).
    /// When set, takes precedence over auto-detection from config.json.
    /// Accepted values: `classifier`, `embeddings`, `seq2seq`, `token_classifier`.
    #[serde(default)]
    pub model_profile: Option<String>,

    /// Enable dynamic request batching (env `BATCH_ENABLED`, D-07, BTCH-02).
    /// When false (default), requests flow through the direct path with zero overhead.
    #[serde(default)]
    pub batch_enabled: bool,

    /// Maximum number of requests to collect in a single batch (env `BATCH_MAX_SIZE`, D-09, BTCH-03).
    /// Defaults to 8. Values > 64 or < 1 are rejected at startup.
    #[serde(default = "default_batch_max_size")]
    pub batch_max_size: u32,

    /// Maximum time in milliseconds to wait for a full batch before executing (env `BATCH_MAX_WAIT_MS`, D-09, BTCH-03).
    /// Defaults to 50ms.
    #[serde(default = "default_batch_max_wait_ms")]
    pub batch_max_wait_ms: u64,
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

fn default_batch_max_size() -> u32 {
    8
}

fn default_batch_max_wait_ms() -> u64 {
    50
}

fn default_forge_timeout_secs() -> u64 {
    600
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

    /// Validate configuration values before resource allocation.
    ///
    /// When `batch_enabled` is true, validates that `batch_max_size`
    /// is within [1, 64] and that `batch_max_wait_ms` does not exceed
    /// `request_timeout_secs * 1000`. When batching is disabled,
    /// batch-related validation is skipped entirely.
    ///
    /// # Errors
    ///
    /// Returns an error if any configuration value is out of range.
    pub fn validate(&self) -> Result<(), anyhow::Error> {
        if self.batch_enabled {
            if self.batch_max_size < 1 || self.batch_max_size > 64 {
                bail!(
                    "batch_max_size must be between 1 and 64 (got {})",
                    self.batch_max_size,
                );
            }
            let timeout_ms = self.request_timeout_secs * 1000;
            if self.batch_max_wait_ms >= timeout_ms {
                bail!(
                    "batch_max_wait_ms ({}) must be less than request_timeout_secs * 1000 ({})",
                    self.batch_max_wait_ms,
                    timeout_ms,
                );
            }
        }
        Ok(())
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
            forge_timeout_secs: 600,
            model_profile: None,
            batch_enabled: false,
            batch_max_size: 8,
            batch_max_wait_ms: 50,
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

    #[test]
    fn test_batch_config_defaults() {
        // Arrange
        let config = config_with_model_path(None);

        // Assert -- batch fields should have their defaults
        assert!(!config.batch_enabled, "batch_enabled should default to false");
        assert_eq!(config.batch_max_size, 8, "batch_max_size should default to 8");
        assert_eq!(config.batch_max_wait_ms, 50, "batch_max_wait_ms should default to 50");
    }

    #[test]
    fn test_batch_config_custom_values() {
        // Arrange
        let mut config = config_with_model_path(None);
        config.batch_enabled = true;
        config.batch_max_size = 16;
        config.batch_max_wait_ms = 100;

        // Assert
        assert!(config.batch_enabled);
        assert_eq!(config.batch_max_size, 16);
        assert_eq!(config.batch_max_wait_ms, 100);
    }

    #[test]
    fn test_validate_rejects_zero_batch_size() {
        // Arrange
        let mut config = config_with_model_path(None);
        config.batch_enabled = true;
        config.batch_max_size = 0;

        // Act
        let result = config.validate();

        // Assert
        assert!(result.is_err(), "batch_max_size=0 should be rejected");
        let msg = format!("{}", result.unwrap_err());
        assert!(
            msg.contains("between 1 and 64"),
            "error should mention valid range: {msg}"
        );
    }

    #[test]
    fn test_validate_rejects_large_batch_size() {
        // Arrange
        let mut config = config_with_model_path(None);
        config.batch_enabled = true;
        config.batch_max_size = 65;

        // Act
        let result = config.validate();

        // Assert
        assert!(result.is_err(), "batch_max_size=65 should be rejected");
        let msg = format!("{}", result.unwrap_err());
        assert!(
            msg.contains("between 1 and 64"),
            "error should mention valid range: {msg}"
        );
    }

    #[test]
    fn test_validate_accepts_valid_batch_size() {
        // Arrange
        let mut config = config_with_model_path(None);
        config.batch_enabled = true;
        config.batch_max_size = 32;

        // Act
        let result = config.validate();

        // Assert
        assert!(result.is_ok(), "batch_max_size=32 should be accepted");
    }

    #[test]
    fn test_validate_skips_when_batching_disabled() {
        // Arrange -- invalid batch_max_size but batching disabled
        let mut config = config_with_model_path(None);
        config.batch_enabled = false;
        config.batch_max_size = 0;

        // Act
        let result = config.validate();

        // Assert -- should pass because batch validation is skipped
        assert!(result.is_ok(), "validation should skip batch checks when batching disabled");
    }

    #[test]
    fn test_forge_timeout_default() {
        let config = config_with_model_path(None);
        assert_eq!(
            config.forge_timeout_secs, 600,
            "forge_timeout_secs should default to 600"
        );
    }

    #[test]
    fn test_validate_rejects_wait_exceeding_timeout() {
        // Arrange -- batch_max_wait_ms > request_timeout_secs * 1000
        let mut config = config_with_model_path(None);
        config.batch_enabled = true;
        config.batch_max_size = 8;
        config.request_timeout_secs = 5;
        config.batch_max_wait_ms = 6000; // 6s > 5s timeout

        // Act
        let result = config.validate();

        // Assert
        assert!(result.is_err(), "batch_max_wait_ms exceeding timeout should be rejected");
    }
}
