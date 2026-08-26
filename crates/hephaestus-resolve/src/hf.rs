//! HuggingFace model download operations.
//!
//! Downloads ONNX model files from the HuggingFace Hub using the `hf-hub`
//! crate. Handles the `onnx/model.onnx` vs `model.onnx` layout convention
//! and returns the snapshot root directory for `ClassifierPipeline::new()`.

use std::path::PathBuf;

use hf_hub::{HFClient, HFError};

use crate::error::ResolveError;

/// Split a combined model ID into `(owner, name)` components.
///
/// If the model ID contains a `/`, splits on the first occurrence:
/// `"Xenova/distilbert"` becomes `("Xenova", "distilbert")`.
///
/// If no `/` is present, returns `(model_id, model_id)` as both owner
/// and name (Pitfall 6 from RESEARCH.md).
pub(crate) fn split_model_id(model_id: &str) -> (String, String) {
    todo!("RED: split_model_id not implemented")
}

/// Download a model's ONNX files from HuggingFace Hub.
///
/// Tries `onnx/model.onnx` first, falls back to `model.onnx`. If neither
/// exists, returns [`ResolveError::NoOnnxExport`] per D-04. Also downloads
/// `tokenizer.json` and `config.json`.
///
/// Returns the snapshot root directory containing all model files.
pub(crate) async fn download_from_hf(
    model_id: &str,
) -> Result<PathBuf, ResolveError> {
    todo!("RED: download_from_hf not implemented")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_model_id_with_org() {
        let (owner, name) = split_model_id("Xenova/distilbert");
        assert_eq!(owner, "Xenova");
        assert_eq!(name, "distilbert");
    }

    #[test]
    fn split_model_id_without_org() {
        let (owner, name) = split_model_id("bert-base");
        assert_eq!(owner, "bert-base");
        assert_eq!(name, "bert-base");
    }

    #[test]
    fn split_model_id_with_multiple_slashes() {
        // Only split on the first slash
        let (owner, name) = split_model_id("org/sub/model");
        assert_eq!(owner, "org");
        assert_eq!(name, "sub/model");
    }
}
