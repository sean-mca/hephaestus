//! Error types for the model resolution pipeline.

use thiserror::Error;

/// Errors produced by model resolution operations.
#[derive(Error, Debug)]
pub enum ResolveError {
    /// Model ID failed validation (T-03-01).
    ///
    /// Returned when the model ID contains characters outside the allowed
    /// set (alphanumeric, hyphen, underscore, forward-slash, period) or
    /// contains path traversal segments (`..`).
    #[error(
        "invalid model ID '{model_id}': only alphanumeric, hyphen, underscore, \
         forward-slash, and period characters are allowed; '..' segments are forbidden"
    )]
    InvalidModelId {
        /// The model ID that failed validation.
        model_id: String,
    },

    /// S3 operation failed.
    #[error("S3 error: {0}")]
    S3(String),

    /// HuggingFace download failed.
    #[error("HuggingFace error: {0}")]
    HuggingFace(String),

    /// Model exists on HuggingFace but has no ONNX export (D-04).
    ///
    /// The model repository was found but contains neither `onnx/model.onnx`
    /// nor `model.onnx`. Use the Forge service to convert, or choose a
    /// model with a pre-exported ONNX variant.
    #[error(
        "model '{model_id}' has no ONNX export and Forge is not configured"
    )]
    NoOnnxExport {
        /// The model ID that lacks an ONNX export.
        model_id: String,
    },

    /// Forge conversion service is not configured (D-10).
    ///
    /// Returned when the resolution chain reaches the Forge tier but
    /// `FORGE_URL` was not set. Set the `FORGE_URL` environment variable
    /// to enable model conversion.
    #[error(
        "Forge is not configured for model '{model_id}': set FORGE_URL to enable conversion"
    )]
    ForgeUnavailable {
        /// The model ID that requires Forge conversion.
        model_id: String,
    },

    /// Forge conversion request failed.
    ///
    /// Returned when the HTTP request to the Forge service fails, returns
    /// a non-success status, or returns an unparseable response body.
    #[error("Forge conversion failed for model '{model_id}': {reason}")]
    ForgeConversion {
        /// The model ID that failed conversion.
        model_id: String,
        /// Human-readable reason for the failure.
        reason: String,
    },

    /// Filesystem I/O error.
    #[error("i/o error")]
    Io(#[from] std::io::Error),
}
