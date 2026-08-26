//! Core inference pipeline for Hephaestus ONNX runtime.
//!
//! This crate defines the [`Pipeline`] trait contract and profile
//! implementations ([`ClassifierPipeline`], [`EmbeddingsPipeline`]).
//! Callers interact only through `prepare()` and `execute()` -- all
//! internal tokenization, tensor construction, and ONNX inference
//! details are hidden behind the trait boundary.
//!
//! Profile detection via [`detect_profile`] auto-selects the correct
//! pipeline based on the model's `config.json`.

pub mod error;
pub mod pipeline;
pub mod profile;
pub(crate) mod postprocess;

pub use error::CoreError;
pub use pipeline::{
    ClassifierOutput, ClassifierPipeline, EmbeddingsPipeline, Pipeline,
    PipelineKind, PreparedInput,
};
pub use profile::{ModelProfile, detect_profile};
