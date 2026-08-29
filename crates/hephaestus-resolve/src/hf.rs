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
    match model_id.split_once('/') {
        Some((owner, name)) => (owner.to_string(), name.to_string()),
        None => (model_id.to_string(), model_id.to_string()),
    }
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
    let client = HFClient::new()
        .map_err(|e| ResolveError::HuggingFace(format!("failed to create HFClient: {e}")))?;

    let (owner, name) = split_model_id(model_id);
    let repo = client.model(&owner, &name);

    // Try onnx/model.onnx first, then model.onnx (D-04, Pitfall 4).
    let onnx_path = match repo.download_file().filename("onnx/model.onnx").send().await {
        Ok(path) => path,
        Err(HFError::EntryNotFound { .. }) => {
            // Try flat layout.
            match repo.download_file().filename("model.onnx").send().await {
                Ok(path) => path,
                Err(HFError::EntryNotFound { .. }) => {
                    return Err(ResolveError::NoOnnxExport {
                        model_id: model_id.to_string(),
                    });
                }
                Err(e) => {
                    return Err(ResolveError::HuggingFace(e.to_string()));
                }
            }
        }
        Err(e) => {
            return Err(ResolveError::HuggingFace(e.to_string()));
        }
    };

    // Download supporting files.
    let _tokenizer = repo
        .download_file()
        .filename("tokenizer.json")
        .send()
        .await
        .map_err(|e| {
            ResolveError::HuggingFace(format!("failed to download tokenizer.json: {e}"))
        })?;

    let _config = repo
        .download_file()
        .filename("config.json")
        .send()
        .await
        .map_err(|e| {
            ResolveError::HuggingFace(format!("failed to download config.json: {e}"))
        })?;

    // Optional: vocab.json (required by CTC ASR models like wav2vec2).
    match repo.download_file().filename("vocab.json").send().await {
        Ok(_) => {}
        Err(HFError::EntryNotFound { .. }) => {}
        Err(e) => {
            tracing::warn!(model_id, error = %e, "failed to download optional vocab.json");
        }
    }

    // Navigate to snapshot root from the ONNX file path.
    // onnx_path is {snapshot_root}/onnx/model.onnx or {snapshot_root}/model.onnx
    let snapshot_root =
        if onnx_path.parent().and_then(|p| p.file_name()) == Some(std::ffi::OsStr::new("onnx")) {
            onnx_path
                .parent()
                .and_then(|p| p.parent())
                .ok_or_else(|| ResolveError::HuggingFace(
                    format!("unexpected ONNX path structure: {}", onnx_path.display()),
                ))?
                .to_path_buf()
        } else {
            onnx_path
                .parent()
                .ok_or_else(|| ResolveError::HuggingFace(
                    format!("ONNX file has no parent directory: {}", onnx_path.display()),
                ))?
                .to_path_buf()
        };

    Ok(snapshot_root)
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
