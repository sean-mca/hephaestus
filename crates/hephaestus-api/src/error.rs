//! API error types and HTTP response mapping.
//!
//! [`ApiError`] wraps [`CoreError`](hephaestus_core::CoreError) and maps
//! each variant to a structured JSON error response with the appropriate
//! HTTP status code per D-03.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use hephaestus_core::CoreError;
use thiserror::Error;

/// Errors produced by the HTTP serving layer.
///
/// Each variant maps to a specific HTTP status code and a machine-parseable
/// error code in the JSON response body (D-03).
#[derive(Error, Debug)]
pub enum ApiError {
    /// Tokenization failed (maps to 422 Unprocessable Entity).
    #[error("tokenization failed: {0}")]
    Tokenization(String),

    /// ONNX inference failed (maps to 500 Internal Server Error).
    #[error("inference failed: {0}")]
    Inference(String),

    /// Model load or validation error (maps to 500 Internal Server Error).
    #[error("model error: {0}")]
    Model(String),

    /// Request-level inference timeout exceeded (maps to 504 Gateway Timeout, D-14).
    #[error("inference timeout")]
    Timeout,

    /// Service not ready -- warmup not complete or shutdown in progress (maps to 503).
    #[error("service not ready")]
    NotReady,

    /// Client sent an invalid request (maps to 400 Bad Request).
    #[error("bad request: {0}")]
    BadRequest(String),

    /// Catch-all internal error (maps to 500 Internal Server Error).
    #[error("internal error: {0}")]
    Internal(String),
}

impl From<CoreError> for ApiError {
    fn from(err: CoreError) -> Self {
        match err {
            CoreError::Tokenization(msg) => Self::Tokenization(msg),
            CoreError::Inference(msg) => Self::Inference(msg),
            CoreError::ModelLoad(msg) | CoreError::ModelValidation(msg) => Self::Model(msg),
            CoreError::Config(msg) => Self::Internal(msg),
            CoreError::Io(e) => Self::Internal(e.to_string()),
            CoreError::JsonParse(e) => Self::Internal(e.to_string()),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            Self::Tokenization(_) => (StatusCode::UNPROCESSABLE_ENTITY, "TOKENIZATION_FAILED"),
            Self::Inference(_) => (StatusCode::INTERNAL_SERVER_ERROR, "INFERENCE_FAILED"),
            Self::Model(_) => (StatusCode::INTERNAL_SERVER_ERROR, "MODEL_ERROR"),
            Self::Timeout => (StatusCode::GATEWAY_TIMEOUT, "INFERENCE_TIMEOUT"),
            Self::NotReady => (StatusCode::SERVICE_UNAVAILABLE, "NOT_READY"),
            Self::BadRequest(_) => (StatusCode::BAD_REQUEST, "BAD_REQUEST"),
            Self::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR"),
        };

        let body = serde_json::json!({
            "error": {
                "code": code,
                "message": self.to_string(),
            }
        });

        (status, axum::Json(body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;

    #[tokio::test]
    async fn api_error_tokenization_maps_to_422() {
        // Arrange
        let err = ApiError::Tokenization("bad token".to_string());

        // Act
        let response = err.into_response();

        // Assert
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = to_bytes(response.into_body(), 1024).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"]["code"], "TOKENIZATION_FAILED");
    }

    #[tokio::test]
    async fn api_error_timeout_maps_to_504() {
        // Arrange
        let err = ApiError::Timeout;

        // Act
        let response = err.into_response();

        // Assert
        assert_eq!(response.status(), StatusCode::GATEWAY_TIMEOUT);
        let body = to_bytes(response.into_body(), 1024).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"]["code"], "INFERENCE_TIMEOUT");
    }

    #[tokio::test]
    async fn api_error_not_ready_maps_to_503() {
        // Arrange
        let err = ApiError::NotReady;

        // Act
        let response = err.into_response();

        // Assert
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let body = to_bytes(response.into_body(), 1024).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"]["code"], "NOT_READY");
    }

    #[tokio::test]
    async fn core_error_converts_to_api_error() {
        // Arrange
        let core_err = CoreError::Tokenization("test error".to_string());

        // Act
        let api_err: ApiError = core_err.into();

        // Assert
        assert!(matches!(api_err, ApiError::Tokenization(_)));
    }
}
