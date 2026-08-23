//! Core inference pipeline for Hephaestus ONNX runtime.
//!
//! This crate defines the [`Pipeline`] trait contract and profile
//! implementations (starting with [`ClassifierPipeline`]). Callers
//! interact only through `prepare()` and `execute()` -- all internal
//! tokenization, tensor construction, and ONNX inference details are
//! hidden behind the trait boundary.

pub mod error;
pub mod pipeline;
pub(crate) mod postprocess;

pub use error::CoreError;
pub use pipeline::{ClassifierOutput, ClassifierPipeline, Pipeline, PreparedInput};
