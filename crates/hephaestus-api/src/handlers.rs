//! HTTP request handlers for inference and health probes.
//!
//! All handlers receive shared [`AppState`] via axum's
//! [`State`](axum::extract::State) extractor.

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use hephaestus_core::Pipeline;
use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::state::AppState;

/// JSON request body for the inference endpoint (D-01).
#[derive(Debug, Deserialize)]
pub struct InferRequest {
    /// The text to classify.
    pub text: String,
}

/// JSON response body for the inference endpoint (D-02).
#[derive(Debug, Serialize)]
pub struct InferResponse {
    /// The predicted label (e.g., "POSITIVE").
    pub label: String,
    /// The confidence score in the range [0.0, 1.0].
    pub score: f32,
    /// The model identifier from configuration.
    pub model_id: String,
    /// Request latency in milliseconds.
    pub latency_ms: u64,
}

/// POST /infer -- run text classification inference.
///
/// Validates readiness, acquires the pipeline lock, runs tokenization
/// and inference, and returns a JSON classification result.
///
/// # Errors
///
/// Returns [`ApiError`] variants mapped to HTTP status codes per D-03:
/// - 503 if the service is not ready
/// - 400 if the request text is empty
/// - 422 if tokenization fails
/// - 504 if inference times out (D-14)
/// - 500 if inference fails
#[tracing::instrument(skip(state))]
pub async fn infer(
    State(state): State<Arc<AppState>>,
    Json(req): Json<InferRequest>,
) -> Result<Json<InferResponse>, ApiError> {
    // Gate on readiness (D-05).
    if !state.ready.load(Ordering::SeqCst) {
        return Err(ApiError::NotReady);
    }

    // Validate input (T-02-01).
    if req.text.is_empty() {
        return Err(ApiError::BadRequest("text field must not be empty".to_string()));
    }

    let start = Instant::now();

    // Acquire pipeline lock and run inference.
    let mut pipeline = state.pipeline.lock().await;
    let prepared = pipeline.prepare(req.text)?;
    let output = pipeline.execute(prepared)?;

    let latency_ms = start.elapsed().as_millis() as u64;

    Ok(Json(InferResponse {
        label: output.label,
        score: output.score,
        model_id: state.model_id.clone(),
        latency_ms,
    }))
}

/// GET /healthz/live -- liveness probe (D-05, D-06).
///
/// Always returns 200 OK with service metadata. Used as the
/// Kubernetes liveness probe -- the pod is alive if the process
/// is running and can respond to HTTP.
pub async fn liveness(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "model_id": state.model_id,
        "uptime_s": state.start_time.elapsed().as_secs(),
    }))
}

/// GET /healthz/ready -- readiness probe (D-05, D-06, D-07).
///
/// Returns 200 after warmup completes; 503 before warmup or after
/// SIGTERM flips readiness to false.
pub async fn readiness(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    if state.ready.load(Ordering::SeqCst) {
        (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "ok",
                "model_id": state.model_id,
                "uptime_s": state.start_time.elapsed().as_secs(),
            })),
        )
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "status": "not_ready",
                "model_id": state.model_id,
            })),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infer_request_deserializes_from_json() {
        // Arrange
        let json = r#"{"text": "hello world"}"#;

        // Act
        let req: InferRequest = serde_json::from_str(json).expect("should deserialize");

        // Assert
        assert_eq!(req.text, "hello world");
    }

    #[test]
    fn infer_response_serializes_with_all_fields() {
        // Arrange
        let resp = InferResponse {
            label: "POSITIVE".to_string(),
            score: 0.95,
            model_id: "test-model".to_string(),
            latency_ms: 12,
        };

        // Act
        let json = serde_json::to_value(&resp).expect("should serialize");

        // Assert
        assert_eq!(json["label"], "POSITIVE");
        let score = json["score"].as_f64().expect("score should be a number");
        assert!((score - 0.95).abs() < 1e-6, "score should be ~0.95, got {score}");
        assert_eq!(json["model_id"], "test-model");
        assert_eq!(json["latency_ms"], 12);
    }
}
