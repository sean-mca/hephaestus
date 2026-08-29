//! Fused single-pass seq2seq pipeline.

use std::path::Path;

use ort::session::Session;
use tokenizers::Tokenizer;

use crate::ep::ExecutionProvider;
use crate::error::CoreError;

use super::{
    check_outputs_nonempty, load_session_and_tokenizer, run_onnx_inference, tokenize_text,
    Pipeline, PipelineOutput, PreparedInput,
};

/// Fused single-pass seq2seq pipeline backed by an ONNX model (D-10).
///
/// Supports models exported as a single fused ONNX graph (e.g., via
/// Optimum with beam search baked in). Runs inference to produce
/// output token IDs, then decodes them back to text via the tokenizer.
/// No auto-regressive decode loop -- single forward pass only.
pub struct Seq2SeqPipeline {
    pub(crate) session: Session,
    pub(crate) tokenizer: Tokenizer,
}

impl Seq2SeqPipeline {
    /// Construct a new seq2seq pipeline from a model directory.
    ///
    /// The directory must contain:
    /// - An ONNX model file (`onnx/model.onnx` or `model.onnx`)
    /// - `tokenizer.json`
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

impl Pipeline for Seq2SeqPipeline {
    type Input = String;
    type Prepared = PreparedInput;
    type Output = String;

    fn prepare(&self, input: String) -> Result<PreparedInput, CoreError> {
        tokenize_text(&self.tokenizer, input)
    }

    fn execute(&mut self, prepared: PreparedInput) -> Result<String, CoreError> {
        let outputs = run_onnx_inference(&mut self.session, &prepared)?;

        // WR-05: Guard against models with zero output tensors.
        check_outputs_nonempty(&outputs)?;

        // Fused seq2seq models output generated token IDs.
        // Try extracting as i64 first (most common); fall back to f32 and round.
        // WR-02: Use checked u32 conversion instead of bare `as u32` casts.
        let output_ids: Vec<u32> =
            if let Ok(tensor) = outputs[0].try_extract_tensor::<i64>() {
                let (_, data) = tensor;
                data.iter()
                    .map(|&id| {
                        u32::try_from(id).map_err(|_| {
                            CoreError::Inference(format!("negative token ID {id} in seq2seq output"))
                        })
                    })
                    .collect::<Result<Vec<u32>, CoreError>>()?
            } else if let Ok(tensor) = outputs[0].try_extract_tensor::<f32>() {
                let (_, data) = tensor;
                data.iter()
                    .map(|&v| {
                        let rounded = v.round();
                        if rounded < 0.0 || rounded > u32::MAX as f32 {
                            return Err(CoreError::Inference(format!(
                                "token ID {v} out of u32 range in seq2seq output"
                            )));
                        }
                        Ok(rounded as u32)
                    })
                    .collect::<Result<Vec<u32>, CoreError>>()?
            } else {
                return Err(CoreError::Inference(
                    "seq2seq output tensor is neither i64 nor f32".to_string(),
                ));
            };

        // Decode token IDs back to text, skipping special tokens.
        self.tokenizer
            .decode(&output_ids, true)
            .map_err(|e| CoreError::Inference(e.to_string()))
    }
}

/// Seq2Seq batch post-processing: extract token IDs and decode per sample.
/// WR-02: Uses checked u32 conversion instead of bare `as u32` casts.
pub(super) fn batch_postprocess_seq2seq(
    outputs: ort::session::SessionOutputs<'_>,
    batch_size: usize,
    tokenizer: &Tokenizer,
) -> Vec<Result<PipelineOutput, CoreError>> {
    // WR-05: Guard against models with zero output tensors.
    if let Err(e) = check_outputs_nonempty(&outputs) {
        return (0..batch_size)
            .map(|_| Err(CoreError::Inference(e.to_string())))
            .collect();
    }

    // Try i64 first, fall back to f32.
    if let Ok(tensor) = outputs[0].try_extract_tensor::<i64>() {
        let (shape, data) = tensor;
        let seq_len = if shape.len() >= 2 {
            shape[shape.len() - 1] as usize
        } else {
            data.len() / batch_size
        };
        return (0..batch_size)
            .map(|i| {
                let sample = &data[i * seq_len..(i + 1) * seq_len];
                let ids: Vec<u32> = sample
                    .iter()
                    .map(|&v| {
                        u32::try_from(v).map_err(|_| {
                            CoreError::Inference(format!(
                                "negative token ID {v} in seq2seq output"
                            ))
                        })
                    })
                    .collect::<Result<Vec<u32>, CoreError>>()?;
                match tokenizer.decode(&ids, true) {
                    Ok(text) => Ok(PipelineOutput::Seq2Seq(text)),
                    Err(e) => Err(CoreError::Inference(e.to_string())),
                }
            })
            .collect();
    }

    if let Ok(tensor) = outputs[0].try_extract_tensor::<f32>() {
        let (shape, data) = tensor;
        let seq_len = if shape.len() >= 2 {
            shape[shape.len() - 1] as usize
        } else {
            data.len() / batch_size
        };
        return (0..batch_size)
            .map(|i| {
                let sample = &data[i * seq_len..(i + 1) * seq_len];
                let ids: Vec<u32> = sample
                    .iter()
                    .map(|&v| {
                        let rounded = v.round();
                        if rounded < 0.0 || rounded > u32::MAX as f32 {
                            return Err(CoreError::Inference(format!(
                                "token ID {v} out of u32 range in seq2seq output"
                            )));
                        }
                        Ok(rounded as u32)
                    })
                    .collect::<Result<Vec<u32>, CoreError>>()?;
                match tokenizer.decode(&ids, true) {
                    Ok(text) => Ok(PipelineOutput::Seq2Seq(text)),
                    Err(e) => Err(CoreError::Inference(e.to_string())),
                }
            })
            .collect();
    }

    (0..batch_size)
        .map(|_| Err(CoreError::Inference("seq2seq output tensor is neither i64 nor f32".into())))
        .collect()
}
