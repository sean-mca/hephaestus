//! Core inference pipeline trait and classifier implementation.

use crate::error::CoreError;
use ort::session::Session;
use tokenizers::Tokenizer;

/// Output from a classifier inference pass.
///
/// Contains the top predicted label and its confidence score.
#[derive(Debug, Clone)]
pub struct ClassifierOutput {
    /// The predicted label (e.g., "POSITIVE", "NEGATIVE").
    pub label: String,
    /// The confidence score in the range [0.0, 1.0].
    pub score: f32,
}

/// Prepared input ready for batch collection or immediate execution.
///
/// This type is opaque to callers outside the crate -- construct it
/// via [`Pipeline::prepare`] and pass it to [`Pipeline::execute`].
/// Fields are not publicly accessible.
#[allow(dead_code)] // Fields used in Plan 02 implementation.
pub struct PreparedInput {
    pub(crate) input_ids: Vec<i64>,
    pub(crate) attention_mask: Vec<i64>,
    pub(crate) sequence_length: usize,
}

/// Core inference pipeline trait.
///
/// Each model profile (classifier, embeddings, etc.) implements this trait.
/// Follows the Ousterhout deep module pattern: two methods hide tokenization,
/// tensor construction, ONNX inference, and post-processing complexity.
///
/// The two-step API (prepare then execute) enables future batching --
/// collect prepared inputs, then execute as a batch (Phase 4).
pub trait Pipeline {
    /// The raw input type accepted by this pipeline (e.g., `String` for text).
    type Input;
    /// The prepared representation after tokenization/preprocessing.
    type Prepared;
    /// The output produced by inference and post-processing.
    type Output;

    /// Tokenize and prepare input for inference.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError`] if tokenization or input preparation fails.
    fn prepare(&self, input: Self::Input) -> Result<Self::Prepared, CoreError>;

    /// Run inference on prepared input and return results.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError`] if inference or post-processing fails.
    fn execute(&mut self, prepared: Self::Prepared) -> Result<Self::Output, CoreError>;
}

/// Text classification pipeline backed by an ONNX model.
///
/// Loads an ONNX classifier model and its associated tokenizer,
/// then runs inference with softmax post-processing to produce
/// a label and confidence score.
#[allow(dead_code)] // Fields used in Plan 02 implementation.
pub struct ClassifierPipeline {
    session: Session,
    tokenizer: Tokenizer,
    id2label: Vec<String>,
}

impl ClassifierPipeline {
    /// Construct a new classifier pipeline from a model directory.
    ///
    /// The directory must contain:
    /// - An ONNX model file (`onnx/model.onnx` or `model.onnx`)
    /// - `tokenizer.json`
    /// - `config.json` with an `id2label` mapping
    ///
    /// # Errors
    ///
    /// Returns [`CoreError`] if any required file is missing or invalid.
    pub fn new(_model_dir: &std::path::Path) -> Result<Self, CoreError> {
        todo!("Plan 02 implements")
    }
}

impl Pipeline for ClassifierPipeline {
    type Input = String;
    type Prepared = PreparedInput;
    type Output = ClassifierOutput;

    fn prepare(&self, _input: String) -> Result<PreparedInput, CoreError> {
        todo!("Plan 02 implements")
    }

    fn execute(&mut self, _prepared: PreparedInput) -> Result<ClassifierOutput, CoreError> {
        todo!("Plan 02 implements")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockall::mock;

    mock! {
        /// Mock pipeline for unit testing consumers of the Pipeline trait.
        pub Pipeline {}

        impl Pipeline for Pipeline {
            type Input = String;
            type Prepared = PreparedInput;
            type Output = ClassifierOutput;

            fn prepare(&self, input: String) -> Result<PreparedInput, CoreError>;
            fn execute(&mut self, prepared: PreparedInput) -> Result<ClassifierOutput, CoreError>;
        }
    }
}
