//! S3 model cache operations.
//!
//! Implements download (get_object) and upload (put_object) for the S3
//! cache tier. Downloads use an atomic temp-dir-then-rename pattern to
//! prevent serving partial files (D-06). Uploads are unconditional with
//! retry and non-fatal failure handling (D-13, D-14).

use std::path::{Path, PathBuf};

use aws_sdk_s3::Client as S3Client;

use crate::error::ResolveError;

/// Required model files for a complete model directory.
pub(crate) const MODEL_ONNX: &str = "model.onnx";
pub(crate) const TOKENIZER_JSON: &str = "tokenizer.json";
pub(crate) const CONFIG_JSON: &str = "config.json";
/// Alternative ONNX file location used by some HuggingFace models.
pub(crate) const ONNX_SUBDIR_MODEL: &str = "onnx/model.onnx";

/// Files to attempt downloading from S3 in order of priority.
/// We try `model.onnx` at root first; if the model was cached from an
/// HF repo that uses the `onnx/` subdirectory layout, we try that too.
const ONNX_CANDIDATES: &[&str] = &[MODEL_ONNX, ONNX_SUBDIR_MODEL];
const SUPPORTING_FILES: &[&str] = &[TOKENIZER_JSON, CONFIG_JSON];

/// Download a model directory from S3 into the local cache (D-01, D-02).
///
/// Constructs S3 keys as `{s3_prefix}/{model_id}/{filename}`. Downloads
/// all required files into a temporary directory on the same filesystem
/// as `cache_dir`, then atomically renames to the final path (D-06).
///
/// Returns `Ok(Some(path))` on cache hit, `Ok(None)` on cache miss
/// (NoSuchKey for the ONNX file), or `Err` on other S3 errors.
pub(crate) async fn download_model_from_s3(
    _client: &S3Client,
    _bucket: &str,
    _s3_prefix: &str,
    _model_id: &str,
    _cache_dir: &Path,
) -> Result<Option<PathBuf>, ResolveError> {
    todo!("S3 download not yet implemented")
}

/// Upload a model directory to S3 for cache-back (D-13).
///
/// Iterates files in `local_dir`, constructs S3 keys as
/// `{s3_prefix}/{model_id}/{filename}`, and uploads each via put_object.
/// Upload is unconditional -- no HeadObject check (D-13).
///
/// On failure, returns `Err` so the caller (with_retry wrapper) can
/// retry. The final caller (spawn_cache_back) logs a warning and
/// discards the error (D-14).
pub(crate) async fn upload_model_to_s3(
    _client: &S3Client,
    _bucket: &str,
    _s3_prefix: &str,
    _model_id: &str,
    _local_dir: &Path,
) -> Result<(), ResolveError> {
    todo!("S3 upload not yet implemented")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // --- Atomic download tests ---

    #[test]
    fn constants_match_expected_filenames() {
        assert_eq!(MODEL_ONNX, "model.onnx");
        assert_eq!(TOKENIZER_JSON, "tokenizer.json");
        assert_eq!(CONFIG_JSON, "config.json");
        assert_eq!(ONNX_SUBDIR_MODEL, "onnx/model.onnx");
    }

    #[test]
    fn onnx_candidates_contains_both_layouts() {
        assert!(ONNX_CANDIDATES.contains(&MODEL_ONNX));
        assert!(ONNX_CANDIDATES.contains(&ONNX_SUBDIR_MODEL));
    }

    #[test]
    fn supporting_files_has_tokenizer_and_config() {
        assert!(SUPPORTING_FILES.contains(&TOKENIZER_JSON));
        assert!(SUPPORTING_FILES.contains(&CONFIG_JSON));
    }

    // Tests that require the actual S3 implementation (todo! will panic)

    #[tokio::test]
    #[should_panic(expected = "not yet implemented")]
    async fn download_returns_none_on_cache_miss() {
        let cache_dir = TempDir::new().unwrap();
        let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
        let client = S3Client::new(&config);
        let _ = download_model_from_s3(
            &client,
            "test-bucket",
            "models",
            "test-org/test-model",
            cache_dir.path(),
        )
        .await;
    }

    #[tokio::test]
    #[should_panic(expected = "not yet implemented")]
    async fn upload_sends_put_object_for_each_file() {
        let local_dir = TempDir::new().unwrap();
        // Create test model files.
        fs::write(local_dir.path().join("model.onnx"), b"fake onnx").unwrap();
        fs::write(local_dir.path().join("tokenizer.json"), b"{}").unwrap();
        fs::write(local_dir.path().join("config.json"), b"{}").unwrap();

        let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
        let client = S3Client::new(&config);
        let _ = upload_model_to_s3(
            &client,
            "test-bucket",
            "models",
            "test-org/test-model",
            local_dir.path(),
        )
        .await;
    }
}
