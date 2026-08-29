//! Model profile detection from config.json fields.
//!
//! Detects which pipeline profile to use based on the model's
//! `config.json` `architectures` field or `pipeline_tag` field.
//! An optional string override takes precedence over auto-detection (D-02).

use crate::error::CoreError;

/// Supported model profile types.
///
/// Each variant maps to a concrete pipeline implementation that handles
/// profile-specific pre/post-processing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelProfile {
    /// Text classification (e.g., sentiment analysis).
    Classifier,
    /// Sentence/document embeddings (e.g., sentence-transformers).
    Embeddings,
    /// Sequence-to-sequence generation (e.g., translation, summarization).
    Seq2Seq,
    /// Token-level classification (e.g., NER, POS tagging).
    TokenClassifier,
    /// Automatic speech recognition (e.g., wav2vec2, Whisper).
    Asr,
}

/// Detect the model profile from config.json fields.
///
/// Priority order (D-01, D-02):
/// 1. Explicit `override_profile` string (from `MODEL_PROFILE` env var)
/// 2. `architectures` array suffix matching
/// 3. `pipeline_tag` field fallback
///
/// # Errors
///
/// Returns [`CoreError::Config`] if the profile cannot be determined
/// from any source, or if the override string is not a recognized profile.
pub fn detect_profile(
    config: &serde_json::Value,
    override_profile: Option<&str>,
) -> Result<ModelProfile, CoreError> {
    // D-02: explicit override takes precedence.
    if let Some(profile_str) = override_profile {
        return parse_profile_string(profile_str);
    }

    // D-01: check architectures field.
    if let Some(archs) = config.get("architectures").and_then(|v| v.as_array()) {
        for arch in archs {
            if let Some(name) = arch.as_str() {
                // ASR: CTC models (Wav2Vec2ForCTC, HubertForCTC, etc.).
                if name.ends_with("ForCTC") {
                    return Ok(ModelProfile::Asr);
                }
                // ASR: Whisper encoder-decoder. Must come BEFORE the
                // generic ForConditionalGeneration check to avoid
                // misdetecting Whisper as Seq2Seq.
                if name == "WhisperForConditionalGeneration" {
                    return Ok(ModelProfile::Asr);
                }
                if name.ends_with("ForSequenceClassification") {
                    return Ok(ModelProfile::Classifier);
                }
                if name.ends_with("ForTokenClassification") {
                    return Ok(ModelProfile::TokenClassifier);
                }
                if name.ends_with("ForConditionalGeneration") {
                    return Ok(ModelProfile::Seq2Seq);
                }
                if name.ends_with("Model") || name.ends_with("ForMaskedLM") {
                    return Ok(ModelProfile::Embeddings);
                }
            }
        }
    }

    // Fallback: check pipeline_tag if present.
    if let Some(tag) = config.get("pipeline_tag").and_then(|v| v.as_str()) {
        match tag {
            "automatic-speech-recognition" => return Ok(ModelProfile::Asr),
            "text-classification" | "sentiment-analysis" => return Ok(ModelProfile::Classifier),
            "token-classification" | "ner" => return Ok(ModelProfile::TokenClassifier),
            "text2text-generation" | "translation" | "summarization" => {
                return Ok(ModelProfile::Seq2Seq);
            }
            "feature-extraction" | "sentence-similarity" => {
                return Ok(ModelProfile::Embeddings);
            }
            _ => {}
        }
    }

    Err(CoreError::Config(
        "unable to detect model profile from config.json: no matching architectures or pipeline_tag found"
            .to_string(),
    ))
}

/// Parse a profile string into a [`ModelProfile`].
///
/// Accepts case-insensitive values: `classifier`, `embeddings`,
/// `seq2seq`, `token_classifier` (with optional hyphen variant).
fn parse_profile_string(s: &str) -> Result<ModelProfile, CoreError> {
    match s.to_lowercase().as_str() {
        "classifier" => Ok(ModelProfile::Classifier),
        "embeddings" => Ok(ModelProfile::Embeddings),
        "seq2seq" => Ok(ModelProfile::Seq2Seq),
        "token_classifier" | "token-classifier" => Ok(ModelProfile::TokenClassifier),
        "asr" => Ok(ModelProfile::Asr),
        _ => Err(CoreError::Config(format!(
            "unrecognized model profile '{s}'; expected one of: classifier, embeddings, seq2seq, token_classifier, asr"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_classifier_from_architectures() {
        // Arrange
        let config = serde_json::json!({
            "architectures": ["DistilBertForSequenceClassification"]
        });

        // Act
        let profile = detect_profile(&config, None).expect("should detect profile");

        // Assert
        assert_eq!(profile, ModelProfile::Classifier);
    }

    #[test]
    fn test_detect_embeddings_from_architectures() {
        // Arrange
        let config = serde_json::json!({
            "architectures": ["BertModel"]
        });

        // Act
        let profile = detect_profile(&config, None).expect("should detect profile");

        // Assert
        assert_eq!(profile, ModelProfile::Embeddings);
    }

    #[test]
    fn test_detect_embeddings_from_masked_lm() {
        // Arrange
        let config = serde_json::json!({
            "architectures": ["BertForMaskedLM"]
        });

        // Act
        let profile = detect_profile(&config, None).expect("should detect profile");

        // Assert
        assert_eq!(profile, ModelProfile::Embeddings);
    }

    #[test]
    fn test_detect_seq2seq_from_architectures() {
        // Arrange
        let config = serde_json::json!({
            "architectures": ["T5ForConditionalGeneration"]
        });

        // Act
        let profile = detect_profile(&config, None).expect("should detect profile");

        // Assert
        assert_eq!(profile, ModelProfile::Seq2Seq);
    }

    #[test]
    fn test_detect_token_classifier_from_architectures() {
        // Arrange
        let config = serde_json::json!({
            "architectures": ["BertForTokenClassification"]
        });

        // Act
        let profile = detect_profile(&config, None).expect("should detect profile");

        // Assert
        assert_eq!(profile, ModelProfile::TokenClassifier);
    }

    #[test]
    fn test_override_takes_precedence() {
        // Arrange -- config says classifier, override says embeddings
        let config = serde_json::json!({
            "architectures": ["BertForSequenceClassification"]
        });

        // Act
        let profile =
            detect_profile(&config, Some("embeddings")).expect("should detect profile");

        // Assert
        assert_eq!(profile, ModelProfile::Embeddings);
    }

    #[test]
    fn test_override_case_insensitive() {
        // Arrange
        let config = serde_json::json!({});

        // Act
        let profile =
            detect_profile(&config, Some("CLASSIFIER")).expect("should detect profile");

        // Assert
        assert_eq!(profile, ModelProfile::Classifier);
    }

    #[test]
    fn test_fallback_to_pipeline_tag() {
        // Arrange -- no architectures field, only pipeline_tag
        let config = serde_json::json!({
            "pipeline_tag": "feature-extraction"
        });

        // Act
        let profile = detect_profile(&config, None).expect("should detect profile");

        // Assert
        assert_eq!(profile, ModelProfile::Embeddings);
    }

    #[test]
    fn test_fallback_pipeline_tag_classifier() {
        // Arrange
        let config = serde_json::json!({
            "pipeline_tag": "text-classification"
        });

        // Act
        let profile = detect_profile(&config, None).expect("should detect profile");

        // Assert
        assert_eq!(profile, ModelProfile::Classifier);
    }

    #[test]
    fn test_fallback_pipeline_tag_ner() {
        // Arrange
        let config = serde_json::json!({
            "pipeline_tag": "ner"
        });

        // Act
        let profile = detect_profile(&config, None).expect("should detect profile");

        // Assert
        assert_eq!(profile, ModelProfile::TokenClassifier);
    }

    #[test]
    fn test_fallback_pipeline_tag_seq2seq() {
        // Arrange
        let config = serde_json::json!({
            "pipeline_tag": "translation"
        });

        // Act
        let profile = detect_profile(&config, None).expect("should detect profile");

        // Assert
        assert_eq!(profile, ModelProfile::Seq2Seq);
    }

    #[test]
    fn test_unknown_architecture_returns_error() {
        // Arrange -- architecture suffix doesn't match any known pattern
        let config = serde_json::json!({
            "architectures": ["CustomArchitectureForSomethingNew"]
        });

        // Act
        let result = detect_profile(&config, None);

        // Assert
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_config_returns_error() {
        // Arrange
        let config = serde_json::json!({});

        // Act
        let result = detect_profile(&config, None);

        // Assert
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_override_returns_error() {
        // Arrange
        let config = serde_json::json!({});

        // Act
        let result = detect_profile(&config, Some("invalid_profile"));

        // Assert
        assert!(result.is_err());
    }

    #[test]
    fn test_detect_asr_from_ctc_architecture() {
        // Arrange
        let config = serde_json::json!({
            "architectures": ["Wav2Vec2ForCTC"]
        });

        // Act
        let profile = detect_profile(&config, None).expect("should detect profile");

        // Assert
        assert_eq!(profile, ModelProfile::Asr);
    }

    #[test]
    fn test_detect_asr_from_hubert_ctc_architecture() {
        // Arrange
        let config = serde_json::json!({
            "architectures": ["HubertForCTC"]
        });

        // Act
        let profile = detect_profile(&config, None).expect("should detect profile");

        // Assert
        assert_eq!(profile, ModelProfile::Asr);
    }

    #[test]
    fn test_detect_asr_from_whisper_architecture() {
        // Arrange
        let config = serde_json::json!({
            "architectures": ["WhisperForConditionalGeneration"]
        });

        // Act
        let profile = detect_profile(&config, None).expect("should detect profile");

        // Assert -- Whisper must be ASR, not Seq2Seq
        assert_eq!(profile, ModelProfile::Asr);
    }

    #[test]
    fn test_detect_asr_from_pipeline_tag() {
        // Arrange
        let config = serde_json::json!({
            "pipeline_tag": "automatic-speech-recognition"
        });

        // Act
        let profile = detect_profile(&config, None).expect("should detect profile");

        // Assert
        assert_eq!(profile, ModelProfile::Asr);
    }

    #[test]
    fn test_override_asr() {
        // Arrange -- config says classifier, override says asr
        let config = serde_json::json!({
            "architectures": ["BertForSequenceClassification"]
        });

        // Act
        let profile = detect_profile(&config, Some("asr")).expect("should detect profile");

        // Assert
        assert_eq!(profile, ModelProfile::Asr);
    }
}
