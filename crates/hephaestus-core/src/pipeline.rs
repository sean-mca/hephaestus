//! Core inference pipeline trait, classifier, and embeddings implementations.

use std::path::Path;

use ndarray::Array2;
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::TensorRef;
use tokenizers::Tokenizer;

use serde::Serialize;

use crate::ep::ExecutionProvider;
use crate::error::CoreError;
use crate::postprocess;

/// A named entity span extracted from token classification output.
///
/// Contains the surface text, entity label, confidence score, and
/// character offsets in the original input string.
#[derive(Debug, Clone, Serialize)]
pub struct Entity {
    /// The surface text of the entity (original word, not subword).
    pub word: String,
    /// The entity label (e.g., "PER", "LOC", "ORG").
    pub entity: String,
    /// The confidence score from the model's softmax output.
    pub score: f32,
    /// Start character offset in the original input.
    pub start: usize,
    /// End character offset in the original input (exclusive).
    pub end: usize,
}

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

/// Input accepted by [`PipelineKind::prepare`] (D-01).
///
/// Text-based pipelines consume `Text(String)`; ASR pipelines
/// consume `Audio(Vec<f32>)`. The `From<String>` impl enables
/// backward-compatible calls -- existing callers that pass a `String`
/// continue to work without modification.
pub enum InferenceInput {
    /// Text input for classifiers, embeddings, seq2seq, and token classifiers.
    Text(String),
    /// Raw audio samples (mono, f32, model-specific sample rate) for ASR.
    Audio(Vec<f32>),
}

impl From<String> for InferenceInput {
    fn from(text: String) -> Self {
        Self::Text(text)
    }
}

/// Prepared audio features ready for ONNX inference.
///
/// Holds mel spectrogram (or reshaped raw waveform) and optionally
/// the original samples for CTC models that need waveform length.
pub struct PreparedAudio {
    /// Mel spectrogram or reshaped waveform tensor (time_steps x features).
    pub(crate) features: Array2<f32>,
    /// Original raw samples, preserved for CTC models that need waveform length.
    /// Retained for future CTC length-dependent processing.
    #[allow(dead_code)]
    pub(crate) raw_samples: Option<Vec<f32>>,
}

impl PreparedAudio {
    /// Construct a `PreparedAudio` for testing purposes.
    ///
    /// Mirrors `PreparedInput::new_for_test` -- bypasses the `pub(crate)`
    /// restriction so downstream crates can create test instances.
    pub fn new_for_test(features: Array2<f32>, raw_samples: Option<Vec<f32>>) -> Self {
        Self { features, raw_samples }
    }
}

/// Generic prepared data wrapping text or audio preprocessing output.
///
/// Returned by [`PipelineKind::prepare`], consumed by
/// [`PipelineKind::execute`] and [`PipelineKind::execute_batch`].
pub enum PreparedData {
    /// Tokenized text input.
    Text(PreparedInput),
    /// Preprocessed audio features.
    Audio(PreparedAudio),
}

impl PreparedData {
    /// Extract the inner [`PreparedInput`] if this is a `Text` variant.
    ///
    /// Returns `None` for `Audio`. Used by the batcher path to unwrap
    /// text inputs for batch tensor construction.
    pub fn into_text(self) -> Option<PreparedInput> {
        match self {
            Self::Text(t) => Some(t),
            Self::Audio(_) => None,
        }
    }
}

/// Typed output from [`PipelineKind::execute`] (D-02).
///
/// Replaces raw `serde_json::Value` with compile-time-checked variants
/// for each model profile. The [`to_json`](PipelineOutput::to_json)
/// method converts to JSON for REST/gRPC responses.
pub enum PipelineOutput {
    /// Classifier result with predicted label and confidence score.
    Classifier { label: String, score: f32 },
    /// Embedding vector (L2-normalized, unit length).
    Embeddings(Vec<f32>),
    /// Generated text from a fused seq2seq model.
    Seq2Seq(String),
    /// Token-level entity spans from NER/POS models.
    TokenClassifier(Vec<Entity>),
    /// Transcribed text from an ASR model.
    Asr(String),
}

impl PipelineOutput {
    /// Convert typed output to a JSON value for HTTP/gRPC responses.
    ///
    /// Each variant produces the same JSON shape that the old
    /// `serde_json::json!` calls in `PipelineKind::execute` produced,
    /// ensuring backward compatibility with existing API consumers.
    pub fn to_json(&self) -> serde_json::Value {
        match self {
            Self::Classifier { label, score } => {
                serde_json::json!({ "label": label, "score": score })
            }
            Self::Embeddings(vec) => {
                serde_json::json!({ "embedding": vec })
            }
            Self::Seq2Seq(text) => {
                serde_json::json!({ "generated_text": text })
            }
            Self::TokenClassifier(entities) => {
                serde_json::json!({ "entities": entities })
            }
            Self::Asr(text) => {
                serde_json::json!({ "text": text })
            }
        }
    }
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
    /// Optional tokenizer encoding preserved for token classification.
    ///
    /// Set to `Some` by `TokenClassifierPipeline::prepare()` so that
    /// `execute()` can access word IDs and offsets for BIO span merging.
    /// All other pipelines set this to `None`.
    pub(crate) encoding: Option<tokenizers::Encoding>,
}

impl PreparedInput {
    /// Construct a `PreparedInput` for testing purposes.
    ///
    /// This bypasses the `pub(crate)` restriction on fields so that
    /// tests in downstream crates (e.g., `hephaestus-api`) can create
    /// instances without a real tokenizer. Not intended for production use.
    pub fn new_for_test(
        input_ids: Vec<i64>,
        attention_mask: Vec<i64>,
        sequence_length: usize,
    ) -> Self {
        Self {
            input_ids,
            attention_mask,
            sequence_length,
            encoding: None,
        }
    }
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

/// Check that session outputs are non-empty before tensor extraction (WR-05).
///
/// Returns [`CoreError::Inference`] if the model produced zero output tensors,
/// which would otherwise cause a panic via direct `outputs[0]` indexing.
fn check_outputs_nonempty(outputs: &ort::session::SessionOutputs<'_>) -> Result<(), CoreError> {
    if outputs.len() == 0 {
        return Err(CoreError::Inference("model produced no output tensors".into()));
    }
    Ok(())
}

/// Load an ONNX session from a model directory.
///
/// Resolves the model file with `onnx/` subdirectory fallback, loads
/// with Level3 optimization, registers the requested execution
/// providers, loads the tokenizer, and validates inputs.
fn load_session_and_tokenizer(
    model_dir: &Path,
    ep: &ExecutionProvider,
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

    // 2. Build execution provider list (may fail if feature not compiled).
    let providers = ep.to_ort_providers()?;

    // 3. Load ONNX session (ort v2 -- no Environment).
    let mut builder = Session::builder()
        .map_err(|e| CoreError::ModelLoad(e.to_string()))?
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .map_err(|e| CoreError::ModelLoad(e.to_string()))?;

    if !providers.is_empty() {
        builder = builder
            .with_execution_providers(providers)
            .map_err(|e| CoreError::ModelLoad(e.to_string()))?;
    }

    let session = builder
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
        encoding: None,
    })
}

/// Check whether an ONNX session declares a `token_type_ids` input.
///
/// BERT-family models require this input (a zeros tensor of the same
/// shape as `input_ids`); DistilBERT-family models omit it. The check
/// lets us conditionally provide it for backward compatibility.
fn session_expects_token_type_ids(session: &Session) -> bool {
    session
        .inputs()
        .iter()
        .any(|input| input.name() == "token_type_ids")
}

/// Run ONNX inference with input_ids, attention_mask, and optional
/// token_type_ids tensors.
///
/// Conditionally includes a `token_type_ids` zeros tensor when the
/// session expects it (BERT-family models). DistilBERT models that
/// omit `token_type_ids` from their inputs are handled without it.
fn run_onnx_inference<'a>(
    session: &'a mut Session,
    prepared: &'a PreparedInput,
) -> Result<ort::session::SessionOutputs<'a>, CoreError> {
    let seq_len = prepared.sequence_length;
    let needs_token_type_ids = session_expects_token_type_ids(session);

    let input_ids_array =
        Array2::from_shape_vec((1, seq_len), prepared.input_ids.clone())
            .map_err(|e| CoreError::Inference(e.to_string()))?;
    let attention_mask_array =
        Array2::from_shape_vec((1, seq_len), prepared.attention_mask.clone())
            .map_err(|e| CoreError::Inference(e.to_string()))?;
    let token_type_ids_array = Array2::<i64>::zeros((1, seq_len));

    let input_ids_tensor = TensorRef::from_array_view(input_ids_array.view())
        .map_err(|e| CoreError::Inference(e.to_string()))?;
    let attention_mask_tensor =
        TensorRef::from_array_view(attention_mask_array.view())
            .map_err(|e| CoreError::Inference(e.to_string()))?;

    if needs_token_type_ids {
        let token_type_ids_tensor =
            TensorRef::from_array_view(token_type_ids_array.view())
                .map_err(|e| CoreError::Inference(e.to_string()))?;
        session
            .run(ort::inputs![
                "input_ids" => input_ids_tensor,
                "attention_mask" => attention_mask_tensor,
                "token_type_ids" => token_type_ids_tensor,
            ])
            .map_err(|e| CoreError::Inference(e.to_string()))
    } else {
        session
            .run(ort::inputs![
                "input_ids" => input_ids_tensor,
                "attention_mask" => attention_mask_tensor,
            ])
            .map_err(|e| CoreError::Inference(e.to_string()))
    }
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
        let attention_mask = prepared.attention_mask.clone();
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
        let mut pooled = postprocess::mean_pool(data, &attention_mask, hidden_dim)?;

        // L2 normalize to unit vector.
        postprocess::l2_normalize(&mut pooled);

        Ok(pooled)
    }
}

// ---------------------------------------------------------------------------
// Seq2SeqPipeline
// ---------------------------------------------------------------------------

/// Fused single-pass seq2seq pipeline backed by an ONNX model (D-10).
///
/// Supports models exported as a single fused ONNX graph (e.g., via
/// Optimum with beam search baked in). Runs inference to produce
/// output token IDs, then decodes them back to text via the tokenizer.
/// No auto-regressive decode loop -- single forward pass only.
pub struct Seq2SeqPipeline {
    session: Session,
    tokenizer: Tokenizer,
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

// ---------------------------------------------------------------------------
// TokenClassifierPipeline
// ---------------------------------------------------------------------------

/// Token classification pipeline for NER, POS tagging, and similar tasks.
///
/// Loads an ONNX token classification model and its tokenizer, runs
/// inference to produce per-token logits, applies argmax to get
/// predicted labels, then merges subword tokens into word-level
/// entity spans using BIO tag conventions.
pub struct TokenClassifierPipeline {
    session: Session,
    tokenizer: Tokenizer,
    id2label: Vec<String>,
}

impl TokenClassifierPipeline {
    /// Construct a new token classification pipeline from a model directory.
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

impl Pipeline for TokenClassifierPipeline {
    type Input = String;
    type Prepared = PreparedInput;
    type Output = Vec<Entity>;

    fn prepare(&self, input: String) -> Result<PreparedInput, CoreError> {
        let encoding = self
            .tokenizer
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
            encoding: Some(encoding),
        })
    }

    fn execute(
        &mut self,
        prepared: PreparedInput,
    ) -> Result<Vec<Entity>, CoreError> {
        let encoding = prepared.encoding.as_ref().ok_or_else(|| {
            CoreError::Inference(
                "TokenClassifierPipeline requires encoding in PreparedInput".to_string(),
            )
        })?;

        let num_tokens = prepared.sequence_length;
        let outputs = run_onnx_inference(&mut self.session, &prepared)?;

        // WR-05: Guard against models with zero output tensors.
        check_outputs_nonempty(&outputs)?;

        // Extract logits tensor -- shape (1, num_tokens, num_labels).
        let tensor = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| CoreError::Inference(e.to_string()))?;
        let (shape, data) = tensor;

        let num_labels = if shape.len() == 3 {
            shape[2] as usize
        } else {
            return Err(CoreError::Inference(format!(
                "expected 3D output tensor (batch, seq_len, num_labels), got {}-D shape",
                shape.len()
            )));
        };

        // Per-token softmax + argmax to get predicted label indices and probability scores.
        let predictions = postprocess::softmax_argmax_per_token(data, num_tokens, num_labels)?;

        // Merge subword tokens into word-level entity spans.
        let mut entities =
            postprocess::merge_subword_entities(&predictions, encoding, &self.id2label);

        for entity in &mut entities {
            // Decode the token IDs corresponding to this entity's char span.
            // Since we have the full encoding, find tokens whose offsets overlap.
            let token_ids: Vec<u32> = encoding
                .get_ids()
                .iter()
                .zip(encoding.get_offsets())
                .filter(|(_, (start, end))| *start < entity.end && *end > entity.start)
                .map(|(&id, _)| id)
                .collect();

            entity.word = self
                .tokenizer
                .decode(&token_ids, true)
                .unwrap_or_default();
        }

        Ok(entities)
    }
}

// ---------------------------------------------------------------------------
// AsrPipeline
// ---------------------------------------------------------------------------

/// Internal mode discriminator for the ASR pipeline.
///
/// CTC models use a single ONNX session with greedy decode.
/// EncoderDecoder models (Whisper) use separate encoder and decoder sessions
/// with autoregressive token generation.
enum AsrMode {
    /// CTC model (wav2vec2, HuBERT): single session, greedy decode.
    Ctc,
    /// Encoder-decoder model (Whisper): separate encoder + decoder sessions.
    EncoderDecoder,
}

/// Automatic speech recognition pipeline for CTC and Whisper models.
///
/// Follows the Ousterhout deep-module pattern: a single `prepare()` /
/// `execute()` interface hides the complexity of feature extraction,
/// ONNX inference, and decoding (CTC greedy or Whisper autoregressive).
pub struct AsrPipeline {
    /// Primary session: the CTC model or the Whisper encoder.
    encoder_session: Session,

    /// Optional decoder session for Whisper encoder-decoder models.
    decoder_session: Option<Session>,

    /// Optional tokenizer for Whisper (uses tokenizer.json for decode).
    tokenizer: Option<Tokenizer>,

    /// Vocabulary for CTC models (loaded from vocab.json).
    vocab: Vec<String>,

    /// CTC blank token index (typically 0 for wav2vec2).
    blank_id: usize,

    /// Whether this is a CTC or encoder-decoder model.
    mode: AsrMode,

    /// Feature extractor type: "mel" (Whisper) or "none" (raw waveform for CTC).
    feature_extractor: String,

    /// Number of mel filter banks (Whisper: 80 or 128).
    n_mels: usize,

    /// FFT window size (Whisper: 400).
    n_fft: usize,

    /// STFT hop length (Whisper: 160).
    hop_length: usize,

    /// Decoder start token ID for Whisper autoregressive generation.
    decoder_start_token_id: usize,

    /// End-of-sequence token ID for Whisper.
    eos_token_id: usize,

    /// Maximum number of decoder tokens (Whisper: 448).
    max_target_positions: usize,

    /// Name of the decoder input_ids input (varies: "input_ids" or "decoder_input_ids").
    decoder_input_name: String,

    /// Name of the encoder hidden states input to the decoder
    /// (varies: "encoder_hidden_states", "encoder_output", etc.).
    encoder_hidden_name: String,
}

impl AsrPipeline {
    /// Construct a new ASR pipeline from a model directory.
    ///
    /// Reads `config.json` to detect whether this is a CTC model or
    /// Whisper encoder-decoder model, then loads the appropriate ONNX
    /// session(s) and vocabulary/tokenizer.
    ///
    /// # Arguments
    ///
    /// * `model_dir` -- Path to the directory containing model files.
    /// * `ep` -- Execution provider for ONNX Runtime.
    /// * `feature_extractor` -- `"mel"` for Whisper, `"none"` for CTC.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError`] if required files are missing or the model
    /// architecture is not a recognized ASR type.
    pub fn new(
        model_dir: &Path,
        ep: &ExecutionProvider,
        feature_extractor: &str,
    ) -> Result<Self, CoreError> {
        // Read config.json to detect model type.
        let config_path = model_dir.join("config.json");
        let config_text = std::fs::read_to_string(&config_path)?;
        let config: serde_json::Value = serde_json::from_str(&config_text)?;

        let archs = config
            .get("architectures")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let is_ctc = archs.iter().any(|a| a.ends_with("ForCTC"));
        let is_whisper = archs.iter().any(|a| a == "WhisperForConditionalGeneration");

        if is_ctc {
            Self::new_ctc(model_dir, ep, feature_extractor)
        } else if is_whisper {
            Self::new_whisper(model_dir, ep, feature_extractor, &config)
        } else {
            Err(CoreError::ModelLoad(format!(
                "ASR pipeline requires ForCTC or WhisperForConditionalGeneration architecture, got: {archs:?}"
            )))
        }
    }

    /// Build a CTC-mode ASR pipeline.
    fn new_ctc(
        model_dir: &Path,
        ep: &ExecutionProvider,
        feature_extractor: &str,
    ) -> Result<Self, CoreError> {
        // Resolve ONNX model file with onnx/ subdirectory fallback.
        let onnx_subdir = model_dir.join("onnx/model.onnx");
        let flat_path = model_dir.join("model.onnx");
        let model_path = if onnx_subdir.exists() {
            onnx_subdir
        } else if flat_path.exists() {
            flat_path
        } else {
            return Err(CoreError::ModelLoad(format!(
                "ONNX model not found; tried '{}' and '{}'",
                model_dir.join("onnx/model.onnx").display(),
                flat_path.display(),
            )));
        };

        let providers = ep.to_ort_providers()?;
        let mut builder = Session::builder()
            .map_err(|e| CoreError::ModelLoad(e.to_string()))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| CoreError::ModelLoad(e.to_string()))?;
        if !providers.is_empty() {
            builder = builder
                .with_execution_providers(providers)
                .map_err(|e| CoreError::ModelLoad(e.to_string()))?;
        }
        let session = builder
            .commit_from_file(&model_path)
            .map_err(|e| CoreError::ModelLoad(e.to_string()))?;

        // T-11-07: Validate that the model has "input_values" input.
        let input_names: Vec<String> = session
            .inputs()
            .iter()
            .map(|i| i.name().to_string())
            .collect();
        if !input_names.iter().any(|n| n == "input_values") {
            return Err(CoreError::ModelValidation(format!(
                "CTC model must have 'input_values' input; got: {input_names:?}"
            )));
        }

        // Load vocab.json: map of token -> id.
        let vocab_path = model_dir.join("vocab.json");
        let vocab_text = std::fs::read_to_string(&vocab_path).map_err(|e| {
            CoreError::ModelLoad(format!("failed to read vocab.json: {e}"))
        })?;
        let vocab_map: std::collections::HashMap<String, usize> =
            serde_json::from_str(&vocab_text)?;

        // Invert to Vec<String> indexed by id.
        let max_id = vocab_map.values().copied().max().unwrap_or(0);
        if max_id > 1_000_000 {
            return Err(CoreError::ModelLoad(format!(
                "vocab.json max token ID {max_id} exceeds safety limit of 1,000,000"
            )));
        }
        let mut vocab = vec![String::new(); max_id + 1];
        for (token, id) in &vocab_map {
            vocab[*id] = token.clone();
        }

        // Determine blank_id: check for "<pad>" first, then "|", default 0.
        let blank_id = vocab_map
            .get("<pad>")
            .or_else(|| vocab_map.get("|"))
            .copied()
            .unwrap_or(0);

        Ok(Self {
            encoder_session: session,
            decoder_session: None,
            tokenizer: None,
            vocab,
            blank_id,
            mode: AsrMode::Ctc,
            feature_extractor: feature_extractor.to_string(),
            n_mels: 80,
            n_fft: 400,
            hop_length: 160,
            decoder_start_token_id: 0,
            eos_token_id: 0,
            max_target_positions: 0,
            decoder_input_name: String::new(),
            encoder_hidden_name: String::new(),
        })
    }

    /// Build a Whisper encoder-decoder ASR pipeline.
    fn new_whisper(
        model_dir: &Path,
        ep: &ExecutionProvider,
        feature_extractor: &str,
        config: &serde_json::Value,
    ) -> Result<Self, CoreError> {
        let onnx_dir = model_dir.join("onnx");

        // Load encoder session.
        let encoder_path = onnx_dir.join("encoder_model.onnx");
        if !encoder_path.exists() {
            return Err(CoreError::ModelLoad(format!(
                "Whisper encoder not found at '{}'",
                encoder_path.display(),
            )));
        }

        let providers = ep.to_ort_providers()?;
        let build_session = |path: &Path| -> Result<Session, CoreError> {
            let mut builder = Session::builder()
                .map_err(|e| CoreError::ModelLoad(e.to_string()))?
                .with_optimization_level(GraphOptimizationLevel::Level3)
                .map_err(|e| CoreError::ModelLoad(e.to_string()))?;
            if !providers.is_empty() {
                builder = builder
                    .with_execution_providers(providers.clone())
                    .map_err(|e| CoreError::ModelLoad(e.to_string()))?;
            }
            builder
                .commit_from_file(path)
                .map_err(|e| CoreError::ModelLoad(e.to_string()))
        };

        let encoder_session = build_session(&encoder_path)?;

        // T-11-07: Validate encoder has "input_features" input.
        let encoder_inputs: Vec<String> = encoder_session
            .inputs()
            .iter()
            .map(|i| i.name().to_string())
            .collect();
        if !encoder_inputs.iter().any(|n| n == "input_features") {
            return Err(CoreError::ModelValidation(format!(
                "Whisper encoder must have 'input_features' input; got: {encoder_inputs:?}"
            )));
        }

        // Load decoder session (try merged first, then standalone).
        let decoder_merged = onnx_dir.join("decoder_model_merged.onnx");
        let decoder_plain = onnx_dir.join("decoder_model.onnx");
        let decoder_path = if decoder_merged.exists() {
            decoder_merged
        } else if decoder_plain.exists() {
            decoder_plain
        } else {
            return Err(CoreError::ModelLoad(
                "Whisper decoder not found (tried decoder_model_merged.onnx and decoder_model.onnx)".into(),
            ));
        };
        let decoder_session = build_session(&decoder_path)?;

        // T-11-07: Validate decoder has "input_ids" or "decoder_input_ids".
        let decoder_inputs: Vec<String> = decoder_session
            .inputs()
            .iter()
            .map(|i| i.name().to_string())
            .collect();
        let decoder_input_name = if decoder_inputs.iter().any(|n| n == "input_ids") {
            "input_ids".to_string()
        } else if decoder_inputs.iter().any(|n| n == "decoder_input_ids") {
            "decoder_input_ids".to_string()
        } else {
            return Err(CoreError::ModelValidation(format!(
                "Whisper decoder must have 'input_ids' or 'decoder_input_ids' input; got: {decoder_inputs:?}"
            )));
        };

        // Validate encoder hidden states input name (varies across ONNX exports).
        let encoder_hidden_name =
            if decoder_inputs.iter().any(|n| n == "encoder_hidden_states") {
                "encoder_hidden_states".to_string()
            } else if decoder_inputs.iter().any(|n| n == "encoder_output") {
                "encoder_output".to_string()
            } else if decoder_inputs
                .iter()
                .any(|n| n == "encoder_last_hidden_state")
            {
                "encoder_last_hidden_state".to_string()
            } else {
                return Err(CoreError::ModelValidation(format!(
                    "Whisper decoder must have 'encoder_hidden_states', 'encoder_output', \
                     or 'encoder_last_hidden_state' input; got: {decoder_inputs:?}"
                )));
            };

        // Load tokenizer.
        let tokenizer_path = model_dir.join("tokenizer.json");
        let tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| CoreError::Tokenization(e.to_string()))?;

        // Read preprocessor config for mel parameters.
        let preprocessor_path = model_dir.join("preprocessor_config.json");
        let (n_mels, n_fft, hop_length) = if preprocessor_path.exists() {
            let text = std::fs::read_to_string(&preprocessor_path)?;
            let preproc: serde_json::Value = serde_json::from_str(&text)?;
            let n_mels = preproc
                .get("feature_size")
                .and_then(|v| v.as_u64())
                .unwrap_or(80) as usize;
            let n_fft = preproc
                .get("n_fft")
                .and_then(|v| v.as_u64())
                .unwrap_or(400) as usize;
            let hop = preproc
                .get("hop_length")
                .and_then(|v| v.as_u64())
                .unwrap_or(160) as usize;
            (n_mels, n_fft, hop)
        } else {
            (80, 400, 160)
        };

        // Read decoder config.
        let decoder_start_token_id = config
            .get("decoder_start_token_id")
            .and_then(|v| v.as_u64())
            .unwrap_or(50258) as usize;
        let eos_token_id = config
            .get("eos_token_id")
            .and_then(|v| v.as_u64())
            .unwrap_or(50257) as usize;
        let max_target_positions = config
            .get("max_target_positions")
            .and_then(|v| v.as_u64())
            .unwrap_or(448) as usize;

        Ok(Self {
            encoder_session,
            decoder_session: Some(decoder_session),
            tokenizer: Some(tokenizer),
            vocab: Vec::new(),
            blank_id: 0,
            mode: AsrMode::EncoderDecoder,
            feature_extractor: feature_extractor.to_string(),
            n_mels,
            n_fft,
            hop_length,
            decoder_start_token_id,
            eos_token_id,
            max_target_positions,
            decoder_input_name,
            encoder_hidden_name,
        })
    }
}

impl Pipeline for AsrPipeline {
    type Input = Vec<f32>;
    type Prepared = PreparedAudio;
    type Output = String;

    fn prepare(&self, input: Vec<f32>) -> Result<PreparedAudio, CoreError> {
        if self.feature_extractor == "mel" {
            let mel_features = crate::mel::compute_mel_spectrogram(
                &input,
                self.n_fft,
                self.hop_length,
                self.n_mels,
                16000,
            )?;
            Ok(PreparedAudio {
                features: mel_features,
                raw_samples: None,
            })
        } else {
            // Raw waveform: shape [1, num_samples] for CTC models.
            let num_samples = input.len();
            let features = Array2::from_shape_vec((1, num_samples), input.clone())
                .map_err(|e| CoreError::Inference(format!("waveform reshape failed: {e}")))?;
            Ok(PreparedAudio {
                features,
                raw_samples: Some(input),
            })
        }
    }

    fn execute(&mut self, prepared: PreparedAudio) -> Result<String, CoreError> {
        match self.mode {
            AsrMode::Ctc => self.execute_ctc(prepared),
            AsrMode::EncoderDecoder => self.execute_whisper(prepared),
        }
    }
}

impl AsrPipeline {
    /// Execute CTC model inference and greedy decode.
    fn execute_ctc(&mut self, prepared: PreparedAudio) -> Result<String, CoreError> {
        // Build input tensor: [1, num_samples] for raw waveform.
        let features = &prepared.features;
        let tensor = TensorRef::from_array_view(features.view())
            .map_err(|e| CoreError::Inference(e.to_string()))?;

        let outputs = self
            .encoder_session
            .run(ort::inputs!["input_values" => tensor])
            .map_err(|e| CoreError::Inference(e.to_string()))?;

        check_outputs_nonempty(&outputs)?;

        // Extract logits: shape [1, timesteps, vocab_size].
        let logits = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| CoreError::Inference(e.to_string()))?;
        let (shape, data) = logits;

        if shape.len() != 3 {
            return Err(CoreError::Inference(format!(
                "expected 3D CTC output (batch, timesteps, vocab_size), got {}-D",
                shape.len()
            )));
        }
        let num_timesteps = shape[1] as usize;
        let vocab_size = shape[2] as usize;

        Ok(crate::ctc::ctc_greedy_decode(
            data,
            num_timesteps,
            vocab_size,
            &self.vocab,
            self.blank_id,
        ))
    }

    /// Execute Whisper encoder-decoder inference with autoregressive decode.
    fn execute_whisper(&mut self, prepared: PreparedAudio) -> Result<String, CoreError> {
        let mel_features = &prepared.features;
        let (n_mels, num_frames) = mel_features.dim();

        // Build 3D input: [1, n_mels, num_frames].
        let flat: Vec<f32> = mel_features.iter().copied().collect();
        let input_3d = ndarray::Array3::from_shape_vec((1, n_mels, num_frames), flat)
            .map_err(|e| CoreError::Inference(format!("mel tensor reshape failed: {e}")))?;

        let input_tensor = TensorRef::from_array_view(input_3d.view())
            .map_err(|e| CoreError::Inference(e.to_string()))?;

        // Run encoder.
        let encoder_outputs = self
            .encoder_session
            .run(ort::inputs!["input_features" => input_tensor])
            .map_err(|e| CoreError::Inference(e.to_string()))?;
        check_outputs_nonempty(&encoder_outputs)?;

        // Extract encoder hidden state.
        let encoder_hidden = encoder_outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| CoreError::Inference(e.to_string()))?;
        let (enc_shape, enc_data) = encoder_hidden;

        // Reconstruct as a 3D array for repeated decoder input.
        let enc_3d = ndarray::Array3::from_shape_vec(
            (
                enc_shape[0] as usize,
                enc_shape[1] as usize,
                enc_shape[2] as usize,
            ),
            enc_data.to_vec(),
        )
        .map_err(|e| CoreError::Inference(format!("encoder output reshape failed: {e}")))?;

        // Autoregressive decode loop (T-11-08: bounded by max_target_positions).
        let decoder_session = self.decoder_session.as_mut().ok_or_else(|| {
            CoreError::Inference("Whisper pipeline missing decoder session".into())
        })?;

        let mut tokens: Vec<i64> = vec![self.decoder_start_token_id as i64];

        for _ in 0..self.max_target_positions {
            let seq_len = tokens.len();
            let token_array =
                Array2::from_shape_vec((1, seq_len), tokens.clone())
                    .map_err(|e| CoreError::Inference(format!("token tensor failed: {e}")))?;

            let token_tensor = TensorRef::from_array_view(token_array.view())
                .map_err(|e| CoreError::Inference(e.to_string()))?;
            let enc_tensor = TensorRef::from_array_view(enc_3d.view())
                .map_err(|e| CoreError::Inference(e.to_string()))?;

            let decoder_outputs = decoder_session
                .run(ort::inputs![
                    self.decoder_input_name.as_str() => token_tensor,
                    self.encoder_hidden_name.as_str() => enc_tensor,
                ])
                .map_err(|e| CoreError::Inference(e.to_string()))?;
            check_outputs_nonempty(&decoder_outputs)?;

            // Extract logits: [1, seq_len, vocab_size].
            let logits = decoder_outputs[0]
                .try_extract_tensor::<f32>()
                .map_err(|e| CoreError::Inference(e.to_string()))?;
            let (logit_shape, logit_data) = logits;

            if logit_shape.len() != 3 {
                return Err(CoreError::Inference(format!(
                    "expected 3D decoder output (batch, seq, vocab), got {}-D",
                    logit_shape.len()
                )));
            }
            let vocab_size = logit_shape[2] as usize;

            // Take logits for the last position.
            let last_pos_start = (seq_len - 1) * vocab_size;
            let expected_len = last_pos_start + vocab_size;
            if logit_data.len() < expected_len {
                return Err(CoreError::Inference(format!(
                    "decoder logit data too short: expected at least {expected_len} elements, got {}",
                    logit_data.len()
                )));
            }
            let last_pos_logits = &logit_data[last_pos_start..expected_len];

            // Argmax.
            let next_token = last_pos_logits
                .iter()
                .enumerate()
                .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(idx, _)| idx)
                .unwrap_or(self.eos_token_id);

            if next_token == self.eos_token_id {
                break;
            }

            tokens.push(next_token as i64);
        }

        // Decode tokens to text (skip the start token and special tokens).
        let tokenizer = self.tokenizer.as_ref().ok_or_else(|| {
            CoreError::Inference("Whisper pipeline missing tokenizer".into())
        })?;

        let output_ids: Vec<u32> = tokens
            .iter()
            .skip(1) // Skip decoder_start_token_id.
            .map(|&t| {
                u32::try_from(t).map_err(|_| {
                    CoreError::Inference(format!(
                        "invalid token ID {t} in Whisper decoder output"
                    ))
                })
            })
            .collect::<Result<Vec<u32>, CoreError>>()?;

        tokenizer
            .decode(&output_ids, true)
            .map_err(|e| CoreError::Inference(e.to_string()))
    }
}

// ---------------------------------------------------------------------------
// PipelineKind enum dispatch (D-03)
// ---------------------------------------------------------------------------

/// Enum dispatch wrapper for all pipeline types (D-03).
///
/// `AppState` holds `RwLock<PipelineKind>` instead of a concrete pipeline.
/// The handler matches on the variant to dispatch prepare/execute calls.
/// No trait objects, no dynamic dispatch overhead.
pub enum PipelineKind {
    /// Text classification pipeline.
    Classifier(ClassifierPipeline),
    /// Sentence/document embeddings pipeline.
    Embeddings(EmbeddingsPipeline),
    /// Fused single-pass seq2seq pipeline (D-10).
    Seq2Seq(Seq2SeqPipeline),
    /// Token classification pipeline (NER, POS).
    TokenClassifier(TokenClassifierPipeline),
    /// Automatic speech recognition pipeline (CTC or Whisper).
    Asr(AsrPipeline),
}

impl PipelineKind {
    /// Prepare input for any profile (D-01).
    ///
    /// Accepts `String` (backward compat via `From<String>`) or
    /// `InferenceInput` directly. Text pipelines reject audio input
    /// with [`CoreError::InvalidInput`]; the ASR variant will be
    /// added in Plan 11-03.
    pub fn prepare(&self, input: impl Into<InferenceInput>) -> Result<PreparedData, CoreError> {
        let input = input.into();
        match (self, input) {
            (Self::Classifier(p), InferenceInput::Text(text)) => {
                Ok(PreparedData::Text(p.prepare(text)?))
            }
            (Self::Embeddings(p), InferenceInput::Text(text)) => {
                Ok(PreparedData::Text(p.prepare(text)?))
            }
            (Self::Seq2Seq(p), InferenceInput::Text(text)) => {
                Ok(PreparedData::Text(p.prepare(text)?))
            }
            (Self::TokenClassifier(p), InferenceInput::Text(text)) => {
                Ok(PreparedData::Text(p.prepare(text)?))
            }
            (Self::Asr(p), InferenceInput::Audio(audio)) => {
                Ok(PreparedData::Audio(p.prepare(audio)?))
            }
            (Self::Asr(_), InferenceInput::Text(_)) => Err(CoreError::InvalidInput(
                "ASR pipeline requires audio input".to_string(),
            )),
            (_, InferenceInput::Audio(_)) => Err(CoreError::InvalidInput(
                "text pipeline requires text input, got audio".to_string(),
            )),
        }
    }

    /// Execute batched inference for multiple prepared inputs.
    ///
    /// Pads all inputs to the maximum sequence length, constructs batch
    /// tensors, runs a single `session.run()` call, then splits the
    /// output by sample and applies profile-specific post-processing.
    ///
    /// Returns one `Result` per input sample. The method matches on the
    /// variant to access session, tokenizer, and id2label directly,
    /// avoiding borrow conflicts between the mutable session borrow
    /// (needed for `session.run()`) and the immutable reads for
    /// post-processing.
    pub fn execute_batch(
        &mut self,
        batch: Vec<PreparedData>,
    ) -> Vec<Result<PipelineOutput, CoreError>> {
        if batch.is_empty() {
            return Vec::new();
        }

        // Extract PreparedInput from each PreparedData::Text item.
        // Audio items in a text-pipeline batch produce per-item errors.
        let batch_size = batch.len();
        let mut text_batch: Vec<PreparedInput> = Vec::with_capacity(batch_size);
        let mut audio_indices: Vec<usize> = Vec::new();
        for (i, item) in batch.into_iter().enumerate() {
            match item {
                PreparedData::Text(t) => text_batch.push(t),
                PreparedData::Audio(_) => {
                    audio_indices.push(i);
                    // Push a dummy to keep indexing aligned.
                    text_batch.push(PreparedInput::new_for_test(vec![0], vec![0], 1));
                }
            }
        }

        // If all items are audio, short-circuit with errors.
        if audio_indices.len() == batch_size {
            return (0..batch_size)
                .map(|_| {
                    Err(CoreError::InvalidInput(
                        "text pipeline received audio prepared data in batch".to_string(),
                    ))
                })
                .collect();
        }

        let batch = text_batch;
        let max_seq_len = batch.iter().map(|p| p.sequence_length).max().unwrap_or(0);

        // Check if the session expects token_type_ids (BERT vs DistilBERT).
        // ASR models do not use token_type_ids.
        let needs_tti = match self {
            Self::Classifier(p) => session_expects_token_type_ids(&p.session),
            Self::Embeddings(p) => session_expects_token_type_ids(&p.session),
            Self::Seq2Seq(p) => session_expects_token_type_ids(&p.session),
            Self::TokenClassifier(p) => session_expects_token_type_ids(&p.session),
            Self::Asr(_) => false,
        };

        // Pad and stack into batch tensors.
        let (input_ids_array, attention_mask_array) =
            match pad_and_stack(&batch, batch_size, max_seq_len) {
                Ok(arrays) => arrays,
                Err(e) => {
                    return (0..batch_size)
                        .map(|_| Err(CoreError::Inference(e.to_string())))
                        .collect();
                }
            };
        let token_type_ids_array = Array2::<i64>::zeros((batch_size, max_seq_len));

        let input_ids_tensor = match TensorRef::from_array_view(input_ids_array.view()) {
            Ok(t) => t,
            Err(e) => {
                return (0..batch_size)
                    .map(|_| Err(CoreError::Inference(e.to_string())))
                    .collect();
            }
        };
        let attention_mask_tensor = match TensorRef::from_array_view(attention_mask_array.view()) {
            Ok(t) => t,
            Err(e) => {
                return (0..batch_size)
                    .map(|_| Err(CoreError::Inference(e.to_string())))
                    .collect();
            }
        };

        let ort_inputs = if needs_tti {
            let tti_tensor = match TensorRef::from_array_view(token_type_ids_array.view()) {
                Ok(t) => t,
                Err(e) => {
                    return (0..batch_size)
                        .map(|_| Err(CoreError::Inference(e.to_string())))
                        .collect();
                }
            };
            ort::inputs![
                "input_ids" => input_ids_tensor,
                "attention_mask" => attention_mask_tensor,
                "token_type_ids" => tti_tensor,
            ]
        } else {
            ort::inputs![
                "input_ids" => input_ids_tensor,
                "attention_mask" => attention_mask_tensor,
            ]
        };

        // Match on variant to access session + postprocessing resources
        // within the same borrow scope.
        let mut results = match self {
            Self::Classifier(p) => {
                let outputs = match p.session.run(ort_inputs) {
                    Ok(o) => o,
                    Err(e) => {
                        let msg = e.to_string();
                        return (0..batch_size).map(|_| Err(CoreError::Inference(msg.clone()))).collect();
                    }
                };
                batch_postprocess_classifier(outputs, batch_size, &p.id2label)
            }
            Self::Embeddings(p) => {
                let outputs = match p.session.run(ort_inputs) {
                    Ok(o) => o,
                    Err(e) => {
                        let msg = e.to_string();
                        return (0..batch_size).map(|_| Err(CoreError::Inference(msg.clone()))).collect();
                    }
                };
                batch_postprocess_embeddings(outputs, batch_size, max_seq_len, &attention_mask_array)
            }
            Self::Seq2Seq(p) => {
                let outputs = match p.session.run(ort_inputs) {
                    Ok(o) => o,
                    Err(e) => {
                        let msg = e.to_string();
                        return (0..batch_size).map(|_| Err(CoreError::Inference(msg.clone()))).collect();
                    }
                };
                batch_postprocess_seq2seq(outputs, batch_size, &p.tokenizer)
            }
            Self::TokenClassifier(p) => {
                let outputs = match p.session.run(ort_inputs) {
                    Ok(o) => o,
                    Err(e) => {
                        let msg = e.to_string();
                        return (0..batch_size).map(|_| Err(CoreError::Inference(msg.clone()))).collect();
                    }
                };
                batch_postprocess_token_classifier(outputs, batch, batch_size, max_seq_len, &p.id2label, &p.tokenizer)
            }
            Self::Asr(_) => {
                return (0..batch_size)
                    .map(|_| Err(CoreError::InvalidInput(
                        "ASR pipeline does not support batched execution".to_string(),
                    )))
                    .collect();
            }
        };

        // Replace results for audio indices with errors.
        for &idx in &audio_indices {
            if idx < results.len() {
                results[idx] = Err(CoreError::InvalidInput(
                    "text pipeline received audio prepared data in batch".to_string(),
                ));
            }
        }

        results
    }

    /// Execute single inference and return typed output (D-02, D-05).
    ///
    /// Accepts [`PreparedData`] and returns [`PipelineOutput`]. Callers
    /// convert to JSON via [`PipelineOutput::to_json()`] when needed.
    pub fn execute(
        &mut self,
        prepared: PreparedData,
    ) -> Result<PipelineOutput, CoreError> {
        match (self, prepared) {
            (Self::Classifier(p), PreparedData::Text(t)) => {
                let out = p.execute(t)?;
                Ok(PipelineOutput::Classifier {
                    label: out.label,
                    score: out.score,
                })
            }
            (Self::Embeddings(p), PreparedData::Text(t)) => {
                let out = p.execute(t)?;
                Ok(PipelineOutput::Embeddings(out))
            }
            (Self::Seq2Seq(p), PreparedData::Text(t)) => {
                let out = p.execute(t)?;
                Ok(PipelineOutput::Seq2Seq(out))
            }
            (Self::TokenClassifier(p), PreparedData::Text(t)) => {
                let out = p.execute(t)?;
                Ok(PipelineOutput::TokenClassifier(out))
            }
            (Self::Asr(p), PreparedData::Audio(a)) => {
                let out = p.execute(a)?;
                Ok(PipelineOutput::Asr(out))
            }
            (Self::Asr(_), PreparedData::Text(_)) => Err(CoreError::InvalidInput(
                "ASR pipeline requires audio prepared data".to_string(),
            )),
            (_, PreparedData::Audio(_)) => Err(CoreError::InvalidInput(
                "text pipeline received audio prepared data".to_string(),
            )),
        }
    }
}

// ---------------------------------------------------------------------------
// Batch post-processing free functions
// ---------------------------------------------------------------------------
//
// These are free functions (not methods on PipelineKind) to avoid borrow
// conflicts: `session.run()` borrows `self` mutably (via the inner pipeline),
// and the returned `SessionOutputs` holds that borrow. Post-processing needs
// immutable access to tokenizer/id2label from the same inner pipeline, so
// the variant match in `execute_batch` passes these references directly.

/// Pad and stack batch inputs into 2D tensors.
///
/// Pads each sample's `input_ids` and `attention_mask` with zeros to
/// `max_seq_len`, then stacks into `(batch_size, max_seq_len)` arrays.
fn pad_and_stack(
    batch: &[PreparedInput],
    batch_size: usize,
    max_seq_len: usize,
) -> Result<(Array2<i64>, Array2<i64>), CoreError> {
    let mut input_ids_flat = Vec::with_capacity(batch_size * max_seq_len);
    let mut attention_mask_flat = Vec::with_capacity(batch_size * max_seq_len);
    for prepared in batch {
        let pad_len = max_seq_len - prepared.sequence_length;
        input_ids_flat.extend_from_slice(&prepared.input_ids);
        input_ids_flat.extend(std::iter::repeat_n(0i64, pad_len));
        attention_mask_flat.extend_from_slice(&prepared.attention_mask);
        attention_mask_flat.extend(std::iter::repeat_n(0i64, pad_len));
    }

    let input_ids_array = Array2::from_shape_vec((batch_size, max_seq_len), input_ids_flat)
        .map_err(|e| CoreError::Inference(format!("batch tensor shape error: {e}")))?;
    let attention_mask_array =
        Array2::from_shape_vec((batch_size, max_seq_len), attention_mask_flat)
            .map_err(|e| CoreError::Inference(format!("batch tensor shape error: {e}")))?;

    Ok((input_ids_array, attention_mask_array))
}

/// Classifier batch post-processing: softmax + argmax per sample.
fn batch_postprocess_classifier(
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

/// Embeddings batch post-processing: mean pool + L2 normalize per sample.
fn batch_postprocess_embeddings(
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

/// Seq2Seq batch post-processing: extract token IDs and decode per sample.
/// WR-02: Uses checked u32 conversion instead of bare `as u32` casts.
fn batch_postprocess_seq2seq(
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

/// Token classifier batch post-processing: per-token argmax + subword merging per sample.
fn batch_postprocess_token_classifier(
    outputs: ort::session::SessionOutputs<'_>,
    batch: Vec<PreparedInput>,
    batch_size: usize,
    max_seq_len: usize,
    id2label: &[String],
    tokenizer: &Tokenizer,
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
    let num_labels = if shape.len() == 3 { shape[2] as usize } else { 0 };
    if num_labels == 0 {
        return (0..batch_size)
            .map(|_| Err(CoreError::Inference("unexpected token classifier output shape".into())))
            .collect();
    }

    batch
        .into_iter()
        .enumerate()
        .map(|(i, prepared)| {
            let num_tokens = prepared.sequence_length;
            let sample_start = i * max_seq_len * num_labels;
            let sample_data = &data[sample_start..sample_start + num_tokens * num_labels];
            let predictions = postprocess::softmax_argmax_per_token(sample_data, num_tokens, num_labels)?;

            let encoding = match &prepared.encoding {
                Some(enc) => enc,
                None => {
                    return Err(CoreError::Inference(
                        "TokenClassifier batch requires encoding in PreparedInput".into(),
                    ))
                }
            };

            let mut entities =
                postprocess::merge_subword_entities(&predictions, encoding, id2label);

            for entity in &mut entities {
                let token_ids: Vec<u32> = encoding
                    .get_ids()
                    .iter()
                    .zip(encoding.get_offsets())
                    .filter(|(_, (start, end))| *start < entity.end && *end > entity.start)
                    .map(|(&id, _)| id)
                    .collect();
                entity.word = tokenizer.decode(&token_ids, true).unwrap_or_default();
            }

            Ok(PipelineOutput::TokenClassifier(entities))
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extract `id2label` mapping from a parsed `config.json` value.
///
/// The `id2label` field is a JSON object with string keys ("0", "1", ...)
/// mapping to label strings. Returns a `Vec<String>` ordered by numeric key.
///
/// # Errors
///
/// Returns [`CoreError::ModelValidation`] if the `id2label` field is missing,
/// keys are non-numeric, values are non-string, or keys are not contiguous
/// from 0 (CR-02).
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

    // CR-02: Validate that keys are contiguous from 0..N.
    for (expected_index, (actual_key, _)) in entries.iter().enumerate() {
        if expected_index != *actual_key {
            return Err(CoreError::ModelValidation(format!(
                "id2label keys must be contiguous from 0; expected key {expected_index}, found {actual_key}",
            )));
        }
    }

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
    fn test_id2label_rejects_noncontiguous() {
        // Arrange -- gap at key 1
        let config: serde_json::Value = serde_json::json!({
            "id2label": {
                "0": "NEG",
                "2": "POS"
            }
        });

        // Act
        let result = extract_id2label(&config);

        // Assert
        assert!(result.is_err(), "non-contiguous keys should be rejected");
        let msg = format!("{}", result.unwrap_err());
        assert!(
            msg.contains("contiguous"),
            "error should mention 'contiguous': {msg}"
        );
    }

    #[test]
    fn test_id2label_accepts_contiguous() {
        // Arrange -- contiguous keys 0, 1, 2
        let config: serde_json::Value = serde_json::json!({
            "id2label": {
                "0": "A",
                "1": "B",
                "2": "C"
            }
        });

        // Act
        let result = extract_id2label(&config);

        // Assert
        assert!(result.is_ok(), "contiguous keys should be accepted");
        assert_eq!(result.unwrap(), vec!["A", "B", "C"]);
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
                    encoding: None,
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
    fn test_token_type_ids_zeros_tensor_shape() {
        // Verify zeros tensor construction matches expected shape for
        // both single inference (1, seq_len) and batch (batch_size, seq_len).
        let single = Array2::<i64>::zeros((1, 7));
        assert_eq!(single.shape(), &[1, 7]);
        assert!(single.iter().all(|&v| v == 0));

        let batch = Array2::<i64>::zeros((4, 12));
        assert_eq!(batch.shape(), &[4, 12]);
        assert!(batch.iter().all(|&v| v == 0));
    }

    #[test]
    fn inference_input_from_string_produces_text() {
        let input = InferenceInput::from("hello".to_string());
        assert!(matches!(input, InferenceInput::Text(ref s) if s == "hello"));
    }

    #[test]
    fn prepared_data_into_text_returns_some_for_text() {
        let prepared = PreparedInput::new_for_test(vec![101], vec![1], 1);
        let data = PreparedData::Text(prepared);
        assert!(data.into_text().is_some());
    }

    #[test]
    fn prepared_data_into_text_returns_none_for_audio() {
        let features = Array2::<f32>::zeros((10, 80));
        let audio = PreparedAudio::new_for_test(features, None);
        let data = PreparedData::Audio(audio);
        assert!(data.into_text().is_none());
    }

    #[test]
    fn pipeline_output_classifier_to_json() {
        let output = PipelineOutput::Classifier {
            label: "POSITIVE".to_string(),
            score: 0.99,
        };
        let json = output.to_json();
        assert_eq!(json["label"], "POSITIVE");
        let score = json["score"].as_f64().unwrap();
        assert!((score - 0.99).abs() < 1e-4);
    }

    #[test]
    fn pipeline_output_embeddings_to_json() {
        let output = PipelineOutput::Embeddings(vec![0.1, 0.2, 0.3]);
        let json = output.to_json();
        let arr = json["embedding"].as_array().unwrap();
        assert_eq!(arr.len(), 3);
    }

    #[test]
    fn pipeline_output_seq2seq_to_json() {
        let output = PipelineOutput::Seq2Seq("translated text".to_string());
        let json = output.to_json();
        assert_eq!(json["generated_text"], "translated text");
    }

    #[test]
    fn pipeline_output_token_classifier_to_json() {
        let entities = vec![Entity {
            word: "John".to_string(),
            entity: "PER".to_string(),
            score: 0.98,
            start: 0,
            end: 4,
        }];
        let output = PipelineOutput::TokenClassifier(entities);
        let json = output.to_json();
        let arr = json["entities"].as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["word"], "John");
        assert_eq!(arr[0]["entity"], "PER");
    }

    #[test]
    fn pipeline_output_asr_to_json() {
        let output = PipelineOutput::Asr("hello world".to_string());
        let json = output.to_json();
        assert_eq!(json["text"], "hello world");
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
