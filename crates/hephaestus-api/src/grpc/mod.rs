//! gRPC serving layer for the Hephaestus inference API.
//!
//! This module provides the tonic-based gRPC service implementations
//! that sit alongside the REST/HTTP layer. The [`GrpcInferenceService`]
//! implements the generated `InferenceService` trait from `hephaestus-proto`,
//! reusing the same [`AppState`](crate::state::AppState) and pipeline
//! locking patterns as the HTTP handlers.

pub mod inference;

pub use inference::GrpcInferenceService;
