//! Core inference pipeline for Hephaestus ONNX runtime.
//!
//! This crate defines the [`Pipeline`] trait contract and profile
//! implementations ([`ClassifierPipeline`], [`EmbeddingsPipeline`],
//! [`Seq2SeqPipeline`], [`TokenClassifierPipeline`]).
//! Callers interact only through `prepare()` and `execute()` -- all
//! internal tokenization, tensor construction, and ONNX inference
//! details are hidden behind the trait boundary.
//!
//! Profile detection via [`detect_profile`] auto-selects the correct
//! pipeline based on the model's `config.json`.

pub mod ep;
pub mod error;
pub mod pipeline;
pub mod profile;
pub(crate) mod postprocess;

pub use ep::ExecutionProvider;
pub use error::CoreError;
pub use pipeline::{
    ClassifierOutput, ClassifierPipeline, EmbeddingsPipeline, Entity,
    Pipeline, PipelineKind, PreparedInput, Seq2SeqPipeline,
    TokenClassifierPipeline,
};
pub use profile::{ModelProfile, detect_profile};
