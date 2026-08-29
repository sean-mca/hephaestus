//! Sentence/document embeddings pipeline.

use std::path::Path;

use ndarray::Array2;
use ort::session::Session;
use tokenizers::Tokenizer;

use crate::ep::ExecutionProvider;
use crate::error::CoreError;
use crate::postprocess;

use super::{
    check_outputs_nonempty, load_session_and_tokenizer, run_onnx_inference, tokenize_text,
    Pipeline, PipelineOutput, PreparedInput,
};

/// Sentence/document embeddings pipeline backed by an ONNX model.
///
/// Loads an ONNX encoder model and its tokenizer, runs inference,
/// applies mean pooling over token hidden states (weighted by the
/// attention mask), and L2-normalizes the result to produce a unit
/// embedding vector.
pub struct EmbeddingsPipeline {
    pub(crate) session: Session,
    pub(crate) tokenizer: Tokenizer,
}

impl EmbeddingsPipeline {
    /// Construct a new embeddings pipeline from a model directory.
    ///
    /// The directory must contain:
    /// - An ONNX model file (`onnx/model.onnx` or `model.onnx`)
    /// - `tokenizer.json`
    ///
    /// No `id2label` or other classification metadata is needed.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError`] if any required file is missing or invalid,
    /// or if the tokenizer outputs are incompatible with the model inputs.
    pub fn new(model_dir: &Path, ep: &ExecutionProvider) -> Result<Self, CoreError> {
        let (session, tokenizer) = load_session_and_tokenizer(model_dir, ep)?;
        Ok(Self { session, tokenizer })
    }
}

impl Pipeline for EmbeddingsPipeline {
    type Input = String;
    type Prepared = PreparedInput;
    type Output = Vec<f32>;

    fn prepare(&self, input: String) -> Result<PreparedInput, CoreError> {
        tokenize_text(&self.tokenizer, input)
    }

    fn execute(&mut self, prepared: PreparedInput) -> Result<Vec<f32>, CoreError> {
        let outputs = run_onnx_inference(&mut self.session, &prepared)?;

        // WR-05: Guard against models with zero output tensors.
        check_outputs_nonempty(&outputs)?;

        // Extract output tensor -- shape (1, seq_len, hidden_dim).
        let tensor = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| CoreError::Inference(e.to_string()))?;
        let (shape, data) = tensor;

        // Determine hidden_dim from shape (Shape derefs to [i64]).
        let hidden_dim = if shape.len() == 3 {
            shape[2] as usize
        } else {
            return Err(CoreError::Inference(format!(
                "expected 3D output tensor (batch, seq_len, hidden_dim), got {}-D shape",
                shape.len()
            )));
        };

        // Mean pool over token dimension using attention mask.
        let mut pooled = postprocess::mean_pool(data, &prepared.attention_mask, hidden_dim)?;

        // L2 normalize to unit vector.
        postprocess::l2_normalize(&mut pooled);

        Ok(pooled)
    }
}

/// Embeddings batch post-processing: mean pool + L2 normalize per sample.
pub(super) fn batch_postprocess_embeddings(
    outputs: ort::session::SessionOutputs<'_>,
    batch_size: usize,
    max_seq_len: usize,
    attention_mask_array: &Array2<i64>,
) -> Vec<Result<PipelineOutput, CoreError>> {
    // WR-05: Guard against models with zero output tensors.
    if let Err(e) = check_outputs_nonempty(&outputs) {
        return (0..batch_size)
            .map(|_| Err(CoreError::Inference(e.to_string())))
            .collect();
    }
    let tensor = match outputs[0].try_extract_tensor::<f32>() {
        Ok(t) => t,
        Err(e) => {
            let msg = e.to_string();
            return (0..batch_size)
                .map(|_| Err(CoreError::Inference(msg.clone())))
                .collect();
        }
    };
    let (shape, data) = tensor;
    let hidden_dim = if shape.len() == 3 { shape[2] as usize } else { 0 };
    if hidden_dim == 0 {
        return (0..batch_size)
            .map(|_| Err(CoreError::Inference("unexpected embeddings output shape".into())))
            .collect();
    }

    (0..batch_size)
        .map(|i| {
            let sample_start = i * max_seq_len * hidden_dim;
            let sample_end = sample_start + max_seq_len * hidden_dim;
            let sample_data = &data[sample_start..sample_end];
            let sample_mask = attention_mask_array.row(i);
            let mask_vec: Vec<i64> = sample_mask.to_vec();
            let mut pooled = postprocess::mean_pool(sample_data, &mask_vec, hidden_dim)?;
            postprocess::l2_normalize(&mut pooled);
            Ok(PipelineOutput::Embeddings(pooled))
        })
        .collect()
}
