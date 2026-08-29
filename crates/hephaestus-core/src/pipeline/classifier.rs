//! Text classification pipeline.

use std::path::Path;

use ort::session::Session;
use tokenizers::Tokenizer;

use crate::ep::ExecutionProvider;
use crate::error::CoreError;
use crate::postprocess;

use super::{
    check_outputs_nonempty, extract_id2label, load_session_and_tokenizer, run_onnx_inference,
    tokenize_text, ClassifierOutput, Pipeline, PipelineOutput, PreparedInput,
};

/// Text classification pipeline backed by an ONNX model.
///
/// Loads an ONNX classifier model and its associated tokenizer,
/// then runs inference with softmax post-processing to produce
/// a label and confidence score.
pub struct ClassifierPipeline {
    pub(crate) session: Session,
    pub(crate) tokenizer: Tokenizer,
    pub(crate) id2label: Vec<String>,
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
    /// Returns [`CoreError`] if any required file is missing or invalid,
    /// or if the tokenizer outputs are incompatible with the model inputs.
    pub fn new(model_dir: &Path, ep: &ExecutionProvider) -> Result<Self, CoreError> {
        let (session, tokenizer) = load_session_and_tokenizer(model_dir, ep)?;

        // Load id2label from config.json.
        let config_path = model_dir.join("config.json");
        let config_text = std::fs::read_to_string(&config_path)?;
        let config: serde_json::Value = serde_json::from_str(&config_text)?;
        let id2label = extract_id2label(&config)?;

        Ok(Self {
            session,
            tokenizer,
            id2label,
        })
    }
}

impl Pipeline for ClassifierPipeline {
    type Input = String;
    type Prepared = PreparedInput;
    type Output = ClassifierOutput;

    fn prepare(&self, input: String) -> Result<PreparedInput, CoreError> {
        tokenize_text(&self.tokenizer, input)
    }

    fn execute(&mut self, prepared: PreparedInput) -> Result<ClassifierOutput, CoreError> {
        let outputs = run_onnx_inference(&mut self.session, &prepared)?;

        // WR-05: Guard against models with zero output tensors.
        check_outputs_nonempty(&outputs)?;

        // Extract logits tensor.
        let logits = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| CoreError::Inference(e.to_string()))?;
        let (_, logits_data) = logits;

        // Apply softmax.
        let probs = postprocess::softmax(logits_data)?;

        // Get top prediction.
        let (idx, score) = postprocess::argmax_with_score(&probs)?;

        // Map index to label.
        let label = self
            .id2label
            .get(idx)
            .ok_or_else(|| {
                CoreError::Inference(format!(
                    "argmax index {idx} out of range for id2label (len {})",
                    self.id2label.len(),
                ))
            })?
            .clone();

        Ok(ClassifierOutput { label, score })
    }
}

/// Classifier batch post-processing: softmax + argmax per sample.
pub(super) fn batch_postprocess_classifier(
    outputs: ort::session::SessionOutputs<'_>,
    batch_size: usize,
    id2label: &[String],
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
    let num_labels = if shape.len() == 2 { shape[1] as usize } else { 0 };
    if num_labels == 0 {
        return (0..batch_size)
            .map(|_| Err(CoreError::Inference("unexpected classifier output shape".into())))
            .collect();
    }

    (0..batch_size)
        .map(|i| {
            let sample_logits = &data[i * num_labels..(i + 1) * num_labels];
            let probs = postprocess::softmax(sample_logits)?;
            let (idx, score) = postprocess::argmax_with_score(&probs)?;
            // CR-04: Return error on out-of-range label index (not empty string).
            let label = id2label.get(idx).ok_or_else(|| {
                CoreError::Inference(format!(
                    "argmax index {idx} out of range for id2label (len {})",
                    id2label.len(),
                ))
            })?.clone();
            Ok(PipelineOutput::Classifier { label, score })
        })
        .collect()
}
