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
}

fn default_ep() -> String {
    "cpu".to_string()
}

fn default_log_level() -> String {
    "info".to_string()
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
