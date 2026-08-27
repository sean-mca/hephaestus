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
}

impl PipelineKind {
    /// Prepare input for any profile. All profiles accept text input.
    pub fn prepare(&self, input: String) -> Result<PreparedInput, CoreError> {
        match self {
            Self::Classifier(p) => p.prepare(input),
            Self::Embeddings(p) => p.prepare(input),
            Self::Seq2Seq(p) => p.prepare(input),
            Self::TokenClassifier(p) => p.prepare(input),
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
        batch: Vec<PreparedInput>,
    ) -> Vec<Result<serde_json::Value, CoreError>> {
        if batch.is_empty() {
            return Vec::new();
        }

        let batch_size = batch.len();
        let max_seq_len = batch.iter().map(|p| p.sequence_length).max().unwrap_or(0);

        // Check if the session expects token_type_ids (BERT vs DistilBERT).
        let needs_tti = match self {
            Self::Classifier(p) => session_expects_token_type_ids(&p.session),
            Self::Embeddings(p) => session_expects_token_type_ids(&p.session),
            Self::Seq2Seq(p) => session_expects_token_type_ids(&p.session),
            Self::TokenClassifier(p) => session_expects_token_type_ids(&p.session),
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
        match self {
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
            Self::Seq2Seq(p) => {
                let out = p.execute(prepared)?;
                Ok(serde_json::json!({
                    "generated_text": out,
                }))
            }
            Self::TokenClassifier(p) => {
                let out = p.execute(prepared)?;
                Ok(serde_json::json!({
                    "entities": out,
                }))
            }
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
) -> Vec<Result<serde_json::Value, CoreError>> {
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
            Ok(serde_json::json!({ "label": label, "score": score }))
        })
        .collect()
}

/// Embeddings batch post-processing: mean pool + L2 normalize per sample.
fn batch_postprocess_embeddings(
    outputs: ort::session::SessionOutputs<'_>,
    batch_size: usize,
    max_seq_len: usize,
    attention_mask_array: &Array2<i64>,
) -> Vec<Result<serde_json::Value, CoreError>> {
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
            Ok(serde_json::json!({ "embedding": pooled }))
        })
        .collect()
}

/// Seq2Seq batch post-processing: extract token IDs and decode per sample.
/// WR-02: Uses checked u32 conversion instead of bare `as u32` casts.
fn batch_postprocess_seq2seq(
    outputs: ort::session::SessionOutputs<'_>,
    batch_size: usize,
    tokenizer: &Tokenizer,
) -> Vec<Result<serde_json::Value, CoreError>> {
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
                    Ok(text) => Ok(serde_json::json!({ "generated_text": text })),
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
                    Ok(text) => Ok(serde_json::json!({ "generated_text": text })),
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
) -> Vec<Result<serde_json::Value, CoreError>> {
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

            Ok(serde_json::json!({ "entities": entities }))
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
