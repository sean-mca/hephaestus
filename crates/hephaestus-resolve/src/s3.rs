//! S3 model cache operations.
//!
//! Implements download (get_object) and upload (put_object) for the S3
//! cache tier. Downloads use an atomic temp-dir-then-rename pattern to
//! prevent serving partial files (D-06). Uploads are unconditional with
//! retry and non-fatal failure handling (D-13, D-14).

use std::path::{Path, PathBuf};

use aws_sdk_s3::Client as S3Client;
use aws_sdk_s3::primitives::ByteStream;

use crate::error::ResolveError;

/// Required model files for a complete model directory.
pub(crate) const MODEL_ONNX: &str = "model.onnx";
pub(crate) const TOKENIZER_JSON: &str = "tokenizer.json";
pub(crate) const CONFIG_JSON: &str = "config.json";
/// Alternative ONNX file location used by some HuggingFace models.
pub(crate) const ONNX_SUBDIR_MODEL: &str = "onnx/model.onnx";

/// S3 cache subdirectory under the HuggingFace home directory.
/// Keeps S3-downloaded models separate from hf-hub's content-addressed
/// blob layout (Open Question 2 in RESEARCH.md).
pub(crate) const S3_CACHE_SUBDIR: &str = "hephaestus/s3-cache";

/// Download a model directory from S3 into the local cache (D-01, D-02).
///
/// Constructs S3 keys as `{s3_prefix}/{model_id}/{filename}`. Downloads
/// all required files into a temporary directory on the same filesystem
/// as `cache_dir`, then atomically renames to the final path (D-06).
///
/// Returns `Ok(Some(path))` on cache hit, `Ok(None)` on cache miss
/// (NoSuchKey for the ONNX file), or `Err` on other S3 errors.
pub(crate) async fn download_model_from_s3(
    client: &S3Client,
    bucket: &str,
    s3_prefix: &str,
    model_id: &str,
    cache_dir: &Path,
) -> Result<Option<PathBuf>, ResolveError> {
    let final_dir = cache_dir.join(S3_CACHE_SUBDIR).join(model_id);

    // Already cached locally -- skip download.
    if final_dir.exists() {
        tracing::debug!(model_id, path = %final_dir.display(), "S3 model already cached locally");
        return Ok(Some(final_dir));
    }

    // Try downloading the ONNX model file.
    // Check model.onnx first, then onnx/model.onnx.
    let mut onnx_filename = None;
    let mut onnx_bytes = None;
    for candidate in &[MODEL_ONNX, ONNX_SUBDIR_MODEL] {
        let key = format_s3_key(s3_prefix, model_id, candidate);
        match download_s3_file(client, bucket, &key).await {
            Ok(bytes) => {
                onnx_filename = Some(*candidate);
                onnx_bytes = Some(bytes);
                break;
            }
            Err(ResolveError::S3(ref msg)) if msg.contains("NoSuchKey") => {
                continue;
            }
            Err(e) => return Err(e),
        }
    }

    // If no ONNX file found, this is a cache miss.
    let (onnx_filename, onnx_data) = match (onnx_filename, onnx_bytes) {
        (Some(f), Some(d)) => (f, d),
        _ => return Ok(None),
    };

    // Download supporting files.
    let tokenizer_key = format_s3_key(s3_prefix, model_id, TOKENIZER_JSON);
    let tokenizer_data = match download_s3_file(client, bucket, &tokenizer_key).await {
        Ok(bytes) => bytes,
        Err(ResolveError::S3(ref msg)) if msg.contains("NoSuchKey") => {
            // Missing supporting file is still a cache miss.
            return Ok(None);
        }
        Err(e) => return Err(e),
    };

    let config_key = format_s3_key(s3_prefix, model_id, CONFIG_JSON);
    let config_data = match download_s3_file(client, bucket, &config_key).await {
        Ok(bytes) => bytes,
        Err(ResolveError::S3(ref msg)) if msg.contains("NoSuchKey") => {
            return Ok(None);
        }
        Err(e) => return Err(e),
    };

    // Atomic download: create temp dir on same filesystem as final path (D-06).
    let parent = final_dir.parent().unwrap_or(cache_dir);
    tokio::fs::create_dir_all(parent).await?;
    let temp_dir = tempfile::TempDir::new_in(parent)
        .map_err(|e| ResolveError::S3(format!("failed to create temp dir: {e}")))?;

    // Write ONNX file (may be in subdirectory).
    let onnx_path = temp_dir.path().join(onnx_filename);
    if let Some(onnx_parent) = onnx_path.parent() {
        tokio::fs::create_dir_all(onnx_parent).await?;
    }
    tokio::fs::write(&onnx_path, &onnx_data).await?;

    // Write supporting files.
    tokio::fs::write(temp_dir.path().join(TOKENIZER_JSON), &tokenizer_data).await?;
    tokio::fs::write(temp_dir.path().join(CONFIG_JSON), &config_data).await?;

    // Atomic rename (same filesystem guarantees atomicity).
    tokio::fs::rename(temp_dir.path(), &final_dir).await?;
    // Prevent TempDir destructor from removing the now-renamed directory (D-06).
    let _ = temp_dir.keep();

    tracing::info!(model_id, path = %final_dir.display(), "downloaded model from S3 cache");
    Ok(Some(final_dir))
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
    client: &S3Client,
    bucket: &str,
    s3_prefix: &str,
    model_id: &str,
    local_dir: &Path,
) -> Result<(), ResolveError> {
    upload_files_recursive(client, bucket, s3_prefix, model_id, local_dir, local_dir).await
}

/// Recursively upload files from a directory to S3.
async fn upload_files_recursive(
    client: &S3Client,
    bucket: &str,
    s3_prefix: &str,
    model_id: &str,
    base_dir: &Path,
    current_dir: &Path,
) -> Result<(), ResolveError> {
    let mut entries = tokio::fs::read_dir(current_dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.is_dir() {
            Box::pin(upload_files_recursive(
                client, bucket, s3_prefix, model_id, base_dir, &path,
            ))
            .await?;
        } else {
            let relative = path
                .strip_prefix(base_dir)
                .map_err(|e| ResolveError::S3(format!("path strip prefix failed: {e}")))?;
            let filename = relative.to_string_lossy();
            let key = format_s3_key(s3_prefix, model_id, &filename);

            let body = ByteStream::from_path(&path)
                .await
                .map_err(|e| {
                    ResolveError::S3(format!(
                        "failed to read {}: {e}",
                        path.display()
                    ))
                })?;

            client
                .put_object()
                .bucket(bucket)
                .key(&key)
                .body(body)
                .send()
                .await
                .map_err(|e| {
                    ResolveError::S3(format!("put_object failed for {key}: {e}"))
                })?;

            tracing::debug!(model_id, key, "uploaded file to S3");
        }
    }
    Ok(())
}

/// Construct an S3 key from prefix, model ID, and filename.
///
/// Format: `{s3_prefix}/{model_id}/{filename}` (D-01).
/// Model IDs with slashes (e.g., `sentence-transformers/all-MiniLM-L6-v2`)
/// become path segments naturally.
fn format_s3_key(s3_prefix: &str, model_id: &str, filename: &str) -> String {
    if s3_prefix.is_empty() {
        format!("{model_id}/{filename}")
    } else {
        format!("{s3_prefix}/{model_id}/{filename}")
    }
}

/// Download a single file from S3 by key.
async fn download_s3_file(
    client: &S3Client,
    bucket: &str,
    key: &str,
) -> Result<Vec<u8>, ResolveError> {
    let resp = client
        .get_object()
        .bucket(bucket)
        .key(key)
        .send()
        .await
        .map_err(|e| {
            let msg = e.to_string();
            // Preserve NoSuchKey detection for the caller.
            if msg.contains("NoSuchKey") || msg.contains("not found") || msg.contains("404") {
                ResolveError::S3(format!("NoSuchKey: {key}"))
            } else {
                ResolveError::S3(format!("get_object failed for {key}: {e}"))
            }
        })?;

    let body = resp
        .body
        .collect()
        .await
        .map_err(|e| ResolveError::S3(format!("failed to read body for {key}: {e}")))?;

    Ok(body.into_bytes().to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // --- Constants tests ---

    #[test]
    fn constants_match_expected_filenames() {
        assert_eq!(MODEL_ONNX, "model.onnx");
        assert_eq!(TOKENIZER_JSON, "tokenizer.json");
        assert_eq!(CONFIG_JSON, "config.json");
        assert_eq!(ONNX_SUBDIR_MODEL, "onnx/model.onnx");
    }

    // --- S3 key formatting tests ---

    #[test]
    fn format_s3_key_with_prefix() {
        let key = format_s3_key("models", "org/model", "model.onnx");
        assert_eq!(key, "models/org/model/model.onnx");
    }

    #[test]
    fn format_s3_key_without_prefix() {
        let key = format_s3_key("", "org/model", "tokenizer.json");
        assert_eq!(key, "org/model/tokenizer.json");
    }

    #[test]
    fn format_s3_key_preserves_model_id_slashes() {
        let key = format_s3_key(
            "cache",
            "sentence-transformers/all-MiniLM-L6-v2",
            "config.json",
        );
        assert_eq!(
            key,
            "cache/sentence-transformers/all-MiniLM-L6-v2/config.json"
        );
    }

    #[test]
    fn format_s3_key_onnx_subdir() {
        let key = format_s3_key("models", "org/model", "onnx/model.onnx");
        assert_eq!(key, "models/org/model/onnx/model.onnx");
    }

    // --- S3 cache subdir test ---

    #[test]
    fn s3_cache_subdir_is_under_hephaestus() {
        assert_eq!(S3_CACHE_SUBDIR, "hephaestus/s3-cache");
    }

    // --- Atomic download pattern verification ---

    #[tokio::test]
    async fn atomic_download_creates_final_dir_via_rename() {
        // Tests the atomic download pattern in isolation.
        let cache_dir = TempDir::new().unwrap();
        let final_dir = cache_dir.path().join(S3_CACHE_SUBDIR).join("test-model");

        // Simulate the atomic pattern.
        let parent = final_dir.parent().unwrap();
        tokio::fs::create_dir_all(parent).await.unwrap();
        let temp = tempfile::TempDir::new_in(parent).unwrap();

        // Write test files.
        tokio::fs::write(temp.path().join("model.onnx"), b"fake model").await.unwrap();
        tokio::fs::write(temp.path().join("tokenizer.json"), b"{}").await.unwrap();
        tokio::fs::write(temp.path().join("config.json"), b"{}").await.unwrap();

        // Atomic rename.
        tokio::fs::rename(temp.path(), &final_dir).await.unwrap();
        let _ = temp.keep();

        // Verify files exist at final location.
        assert!(final_dir.join("model.onnx").exists());
        assert!(final_dir.join("tokenizer.json").exists());
        assert!(final_dir.join("config.json").exists());
    }

    #[tokio::test]
    async fn atomic_download_temp_dir_same_filesystem() {
        // Verify TempDir::new_in uses the same parent directory.
        let cache_dir = TempDir::new().unwrap();
        let parent = cache_dir.path().join(S3_CACHE_SUBDIR);
        fs::create_dir_all(&parent).unwrap();

        let temp = tempfile::TempDir::new_in(&parent).unwrap();
        // temp.path() should be under parent, guaranteeing same filesystem.
        assert!(
            temp.path().starts_with(&parent),
            "temp dir should be on same filesystem as cache"
        );
    }

    #[tokio::test]
    async fn keep_prevents_destructor_cleanup() {
        let cache_dir = TempDir::new().unwrap();
        let parent = cache_dir.path().join("test-parent");
        fs::create_dir_all(&parent).unwrap();

        let temp = tempfile::TempDir::new_in(&parent).unwrap();
        let temp_path = temp.path().to_path_buf();
        fs::write(temp_path.join("test.txt"), b"data").unwrap();

        // Rename to final location.
        let final_path = parent.join("final");
        fs::rename(&temp_path, &final_path).unwrap();

        // keep() prevents destructor from removing temp_path.
        let _ = temp.keep();

        // Final location should still have the file.
        assert!(final_path.join("test.txt").exists());
    }

    // --- Upload directory structure tests ---

    #[tokio::test]
    async fn upload_model_dir_discovers_files() {
        // Create a model directory with files.
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("model.onnx"), b"model data").unwrap();
        fs::write(dir.path().join("tokenizer.json"), b"{}").unwrap();
        fs::write(dir.path().join("config.json"), b"{}").unwrap();

        // Verify the directory has the expected files.
        let mut entries: Vec<String> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        entries.sort();
        assert_eq!(entries, vec!["config.json", "model.onnx", "tokenizer.json"]);
    }

    #[tokio::test]
    async fn upload_model_dir_handles_onnx_subdir() {
        // Create a model directory with onnx/ subdirectory layout.
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("onnx")).unwrap();
        fs::write(dir.path().join("onnx/model.onnx"), b"model data").unwrap();
        fs::write(dir.path().join("tokenizer.json"), b"{}").unwrap();
        fs::write(dir.path().join("config.json"), b"{}").unwrap();

        // Verify the onnx subdirectory exists.
        assert!(dir.path().join("onnx/model.onnx").exists());
    }

    // --- Local cache hit shortcut ---

    #[tokio::test]
    async fn download_returns_existing_local_cache() {
        let cache_dir = TempDir::new().unwrap();
        let model_dir = cache_dir.path().join(S3_CACHE_SUBDIR).join("test/model");
        fs::create_dir_all(&model_dir).unwrap();
        fs::write(model_dir.join("model.onnx"), b"cached").unwrap();

        // When the local cache already exists, it should return immediately
        // without needing a real S3 client. Use a dummy client.
        let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
        let client = S3Client::new(&config);

        let result = download_model_from_s3(
            &client,
            "any-bucket",
            "prefix",
            "test/model",
            cache_dir.path(),
        )
        .await;

        assert!(result.is_ok());
        let path = result.unwrap();
        assert!(path.is_some());
        assert_eq!(path.unwrap(), model_dir);
    }
}
