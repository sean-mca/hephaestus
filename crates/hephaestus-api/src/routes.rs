//! axum router construction for the Hephaestus HTTP API.
//!
//! The [`build_router`] function mounts all HTTP endpoints and
//! attaches shared [`AppState`] as axum state.

use std::sync::Arc;

use axum::routing::{get, post};
use axum::Router;
use tower_http::trace::TraceLayer;

use crate::handlers;
use crate::metrics;
use crate::state::AppState;

/// Construct the axum router with all HTTP endpoints.
///
/// Mounts:
/// - `POST /infer` -- text classification inference (D-01)
/// - `GET /healthz/live` -- liveness probe (D-05)
/// - `GET /healthz/ready` -- readiness probe (D-05)
/// - `GET /metrics` -- Prometheus scrape endpoint (OBSV-01)
pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/infer", post(handlers::infer))
        .route("/healthz/live", get(handlers::liveness))
        .route("/healthz/ready", get(handlers::readiness))
        .route("/metrics", get(metrics::metrics_handler))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
