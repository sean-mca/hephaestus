//! Core inference pipeline trait, classifier, and embeddings implementations.

use std::path::Path;

use ndarray::Array2;
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::TensorRef;
use tokenizers::Tokenizer;

use crate::error::CoreError;
use crate::postprocess;

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
#[cfg_attr(test, mockall::automock(
    type Input = String;
    type Prepared = PreparedInput;
    type Output = ClassifierOutput;
))]
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

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Load an ONNX session from a model directory.
///
/// Resolves the model file with `onnx/` subdirectory fallback, loads
/// with Level3 optimization, loads the tokenizer, and validates inputs.
fn load_session_and_tokenizer(
    model_dir: &Path,
) -> Result<(Session, Tokenizer), CoreError> {
    // 1. Resolve model file with onnx/ subdirectory fallback.
    let onnx_subdir = model_dir.join("onnx/model.onnx");
    let flat_path = model_dir.join("model.onnx");
    let model_path = if onnx_subdir.exists() {
        onnx_subdir
    } else if flat_path.exists() {
        flat_path
    } else {
        return Err(CoreError::ModelLoad(format!(
            "ONNX model not found; tried '{}' and '{}'",
            onnx_subdir.display(),
            flat_path.display(),
        )));
    };

    // 2. Load ONNX session (ort v2 -- no Environment).
    let session = Session::builder()
        .map_err(|e| CoreError::ModelLoad(e.to_string()))?
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .map_err(|e| CoreError::ModelLoad(e.to_string()))?
        .commit_from_file(&model_path)
        .map_err(|e| CoreError::ModelLoad(e.to_string()))?;

    // 3. Load tokenizer.
    let tokenizer_path = model_dir.join("tokenizer.json");
    let mut tokenizer = Tokenizer::from_file(&tokenizer_path)
        .map_err(|e| CoreError::Tokenization(e.to_string()))?;

    // 4. Configure truncation to mitigate T-01-02 DoS threat.
    tokenizer
        .with_truncation(Some(tokenizers::TruncationParams {
            max_length: 512,
            ..Default::default()
        }))
        .map_err(|e| CoreError::Tokenization(e.to_string()))?;

    // 5. Validate tokenizer-model compatibility (TOKN-03).
    let model_input_names: Vec<String> = session
        .inputs()
        .iter()
        .map(|input| input.name().to_string())
        .collect();
    let model_input_name_refs: Vec<&str> =
        model_input_names.iter().map(String::as_str).collect();
    let required_inputs = ["input_ids", "attention_mask"];
    for required in &required_inputs {
        if !model_input_name_refs.contains(required) {
            return Err(CoreError::ModelValidation(format!(
                "model does not accept input '{required}'; model inputs are: {model_input_name_refs:?}",
            )));
        }
    }

    Ok((session, tokenizer))
}

/// Tokenize text into prepared input (shared by all text-based pipelines).
fn tokenize_text(
    tokenizer: &Tokenizer,
    input: String,
) -> Result<PreparedInput, CoreError> {
    let encoding = tokenizer
        .encode(input.as_str(), true)
        .map_err(|e| CoreError::Tokenization(e.to_string()))?;

    let input_ids: Vec<i64> = encoding.get_ids().iter().map(|&id| i64::from(id)).collect();
    let attention_mask: Vec<i64> = encoding
        .get_attention_mask()
        .iter()
        .map(|&m| i64::from(m))
        .collect();
    let sequence_length = encoding.len();

    Ok(PreparedInput {
        input_ids,
        attention_mask,
        sequence_length,
    })
}

/// Run ONNX inference with input_ids and attention_mask tensors.
///
/// Returns the raw output values from the session.
fn run_onnx_inference<'a>(
    session: &'a mut Session,
    prepared: &'a PreparedInput,
) -> Result<ort::session::SessionOutputs<'a>, CoreError> {
    let seq_len = prepared.sequence_length;

    let input_ids_array =
        Array2::from_shape_vec((1, seq_len), prepared.input_ids.clone())
            .map_err(|e| CoreError::Inference(e.to_string()))?;
    let attention_mask_array =
        Array2::from_shape_vec((1, seq_len), prepared.attention_mask.clone())
            .map_err(|e| CoreError::Inference(e.to_string()))?;

    let input_ids_tensor = TensorRef::from_array_view(input_ids_array.view())
        .map_err(|e| CoreError::Inference(e.to_string()))?;
    let attention_mask_tensor =
        TensorRef::from_array_view(attention_mask_array.view())
            .map_err(|e| CoreError::Inference(e.to_string()))?;

    session
        .run(ort::inputs![
            "input_ids" => input_ids_tensor,
            "attention_mask" => attention_mask_tensor,
        ])
        .map_err(|e| CoreError::Inference(e.to_string()))
}

// ---------------------------------------------------------------------------
// ClassifierPipeline
// ---------------------------------------------------------------------------

/// Text classification pipeline backed by an ONNX model.
///
/// Loads an ONNX classifier model and its associated tokenizer,
/// then runs inference with softmax post-processing to produce
/// a label and confidence score.
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
    /// Returns [`CoreError`] if any required file is missing or invalid,
    /// or if the tokenizer outputs are incompatible with the model inputs.
    pub fn new(model_dir: &Path) -> Result<Self, CoreError> {
        let (session, tokenizer) = load_session_and_tokenizer(model_dir)?;

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

        // Extract logits tensor.
        let logits = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| CoreError::Inference(e.to_string()))?;
        let (_, logits_data) = logits;

        // Apply softmax.
        let probs = postprocess::softmax(logits_data);

        // Get top prediction.
        let (idx, score) = postprocess::argmax_with_score(&probs);

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

// ---------------------------------------------------------------------------
// EmbeddingsPipeline
// ---------------------------------------------------------------------------

/// Sentence/document embeddings pipeline backed by an ONNX model.
///
/// Loads an ONNX encoder model and its tokenizer, runs inference,
/// applies mean pooling over token hidden states (weighted by the
/// attention mask), and L2-normalizes the result to produce a unit
/// embedding vector.
pub struct EmbeddingsPipeline {
    session: Session,
    tokenizer: Tokenizer,
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
    pub fn new(model_dir: &Path) -> Result<Self, CoreError> {
        let (session, tokenizer) = load_session_and_tokenizer(model_dir)?;
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
        let attention_mask = prepared.attention_mask.clone();
        let outputs = run_onnx_inference(&mut self.session, &prepared)?;

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
        let mut pooled = postprocess::mean_pool(data, &attention_mask, hidden_dim);

        // L2 normalize to unit vector.
        postprocess::l2_normalize(&mut pooled);

        Ok(pooled)
    }
}

// ---------------------------------------------------------------------------
// PipelineKind enum dispatch (D-03)
// ---------------------------------------------------------------------------

/// Enum dispatch wrapper for all pipeline types (D-03).
///
/// `AppState` holds `Mutex<PipelineKind>` instead of a concrete pipeline.
/// The handler matches on the variant to dispatch prepare/execute calls.
/// No trait objects, no dynamic dispatch overhead.
pub enum PipelineKind {
    /// Text classification pipeline.
    Classifier(ClassifierPipeline),
    /// Sentence/document embeddings pipeline.
    Embeddings(EmbeddingsPipeline),
}

impl PipelineKind {
    /// Prepare input for any profile. All profiles accept text input.
    pub fn prepare(&self, input: String) -> Result<PreparedInput, CoreError> {
        match self {
            Self::Classifier(p) => p.prepare(input),
            Self::Embeddings(p) => p.prepare(input),
        }
    }

    /// Execute single inference and return model-determined output as JSON value (D-05).
    pub fn execute(
        &mut self,
        prepared: PreparedInput,
    ) -> Result<serde_json::Value, CoreError> {
        match self {
            Self::Classifier(p) => {
                let out = p.execute(prepared)?;
                Ok(serde_json::json!({
                    "label": out.label,
                    "score": out.score,
                }))
            }
            Self::Embeddings(p) => {
                let out = p.execute(prepared)?;
                Ok(serde_json::json!({
                    "embedding": out,
                }))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extract `id2label` mapping from a parsed `config.json` value.
///
/// The `id2label` field is a JSON object with string keys ("0", "1", ...)
/// mapping to label strings. Returns a `Vec<String>` ordered by numeric key.
fn extract_id2label(config: &serde_json::Value) -> Result<Vec<String>, CoreError> {
    let id2label_obj = config
        .get("id2label")
        .and_then(|v| v.as_object())
        .ok_or_else(|| {
            CoreError::ModelValidation(
                "config.json missing 'id2label' object".to_string(),
            )
        })?;

    let mut entries: Vec<(usize, String)> = Vec::with_capacity(id2label_obj.len());
    for (key, value) in id2label_obj {
        let idx: usize = key.parse().map_err(|_| {
            CoreError::ModelValidation(format!(
                "id2label key '{key}' is not a valid numeric index",
            ))
        })?;
        let label = value.as_str().ok_or_else(|| {
            CoreError::ModelValidation(format!(
                "id2label value for key '{key}' is not a string",
            ))
        })?;
        entries.push((idx, label.to_string()));
    }

    entries.sort_by_key(|(idx, _)| *idx);
    Ok(entries.into_iter().map(|(_, label)| label).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_id2label_parsing() {
        // Arrange
        let config: serde_json::Value = serde_json::json!({
            "id2label": {
                "0": "NEGATIVE",
                "1": "POSITIVE"
            }
        });

        // Act
        let labels = extract_id2label(&config).expect("should parse id2label");

        // Assert
        assert_eq!(labels, vec!["NEGATIVE", "POSITIVE"]);
    }

    #[test]
    fn test_id2label_ordering() {
        // Arrange -- keys intentionally out of order
        let config: serde_json::Value = serde_json::json!({
            "id2label": {
                "2": "NEUTRAL",
                "0": "NEGATIVE",
                "1": "POSITIVE"
            }
        });

        // Act
        let labels = extract_id2label(&config).expect("should parse id2label");

        // Assert -- must be sorted by numeric key
        assert_eq!(labels, vec!["NEGATIVE", "POSITIVE", "NEUTRAL"]);
    }

    #[test]
    fn test_id2label_missing() {
        // Arrange
        let config: serde_json::Value = serde_json::json!({
            "model_type": "distilbert"
        });

        // Act
        let result = extract_id2label(&config);

        // Assert
        assert!(result.is_err());
    }

    #[test]
    fn test_mock_pipeline_prepare_execute() {
        // Arrange
        let mut mock = MockPipeline::new();

        mock.expect_prepare()
            .returning(|_input: String| {
                Ok(PreparedInput {
                    input_ids: vec![101, 2023, 102],
                    attention_mask: vec![1, 1, 1],
                    sequence_length: 3,
                })
            });

        mock.expect_execute()
            .returning(|_prepared: PreparedInput| {
                Ok(ClassifierOutput {
                    label: "POSITIVE".to_string(),
                    score: 0.95,
                })
            });

        // Act
        let prepared = mock
            .prepare("test input".to_string())
            .expect("prepare should succeed");
        let output = mock.execute(prepared).expect("execute should succeed");

        // Assert
        assert_eq!(output.label, "POSITIVE");
        assert!((output.score - 0.95).abs() < 1e-6);
    }

    #[test]
    fn test_pipeline_kind_variant_sizes() {
        // Verify PipelineKind doesn't trigger clippy large_enum_variant.
        // Both variants hold Session + Tokenizer + optional metadata,
        // so sizes should be comparable.
        let classifier_size = std::mem::size_of::<ClassifierPipeline>();
        let embeddings_size = std::mem::size_of::<EmbeddingsPipeline>();

        // Neither should be more than 3x the other.
        let ratio = if classifier_size > embeddings_size {
            classifier_size as f64 / embeddings_size as f64
        } else {
            embeddings_size as f64 / classifier_size as f64
        };

        assert!(
            ratio < 3.0,
            "variant size ratio {ratio:.1} exceeds 3x -- consider boxing the larger variant"
        );
    }
}
