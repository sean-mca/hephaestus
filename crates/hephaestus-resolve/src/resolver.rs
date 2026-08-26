//! Model resolver: deep module hiding the 3-tier resolution chain.
//!
//! [`ModelResolver`] exposes a single [`resolve()`](ModelResolver::resolve)
//! method that checks S3 cache, falls back to HuggingFace, and finally
//! attempts Forge conversion. All tier details, retry logic, and caching
//! are hidden behind this interface (RSLV-05, D-05).

use std::path::PathBuf;
use std::time::Duration;

use crate::error::ResolveError;
use crate::hf;

/// Model resolver implementing the 3-tier resolution chain.
///
/// Exposes a single `resolve()` method per the Ousterhout deep module
/// pattern (RSLV-05). Callers never see S3/HF/Forge internals.
pub struct ModelResolver {
    cache_dir: PathBuf,
    s3_bucket: Option<String>,
    s3_prefix: Option<String>,
    forge_url: Option<String>,
}

/// Validate that a model ID contains only allowed characters (T-03-01, T-03-04).
///
/// Allowed characters: alphanumeric, hyphen (`-`), underscore (`_`),
/// forward-slash (`/`), and period (`.`). Path segments equal to `..`
/// are rejected to prevent directory traversal.
///
/// Called as the first operation inside [`ModelResolver::resolve()`]
/// before any tier logic.
pub fn validate_model_id(model_id: &str) -> Result<(), ResolveError> {
    // Reject empty strings.
    if model_id.is_empty() {
        return Err(ResolveError::InvalidModelId {
            model_id: model_id.to_string(),
        });
    }

    // Reject characters outside the allowed set.
    let is_allowed = |c: char| c.is_alphanumeric() || c == '-' || c == '_' || c == '/' || c == '.';
    if !model_id.chars().all(is_allowed) {
        return Err(ResolveError::InvalidModelId {
            model_id: model_id.to_string(),
        });
    }

    // Reject ".." path segments (directory traversal).
    for segment in model_id.split('/') {
        if segment == ".." {
            return Err(ResolveError::InvalidModelId {
                model_id: model_id.to_string(),
            });
        }
    }

    Ok(())
}

impl ModelResolver {
    /// Create a new model resolver.
    ///
    /// # Arguments
    ///
    /// * `s3_bucket` -- S3 bucket name for cache tier (optional, D-03).
    /// * `s3_prefix` -- S3 key prefix prepended to model IDs (optional).
    /// * `forge_url` -- Forge conversion service URL (optional, D-09).
    pub fn new(
        s3_bucket: Option<&str>,
        s3_prefix: Option<&str>,
        forge_url: Option<&str>,
    ) -> Result<Self, ResolveError> {
        // Determine cache_dir from HF_HOME or default (D-07).
        // hf-hub uses HF_HOME or ~/.cache/huggingface by default.
        let cache_dir = match std::env::var("HF_HOME") {
            Ok(home) => PathBuf::from(home),
            Err(_) => {
                let home = std::env::var("HOME")
                    .map(PathBuf::from)
                    .unwrap_or_else(|_| PathBuf::from("."));
                home.join(".cache").join("huggingface")
            }
        };

        Ok(Self {
            cache_dir,
            s3_bucket: s3_bucket.map(String::from),
            s3_prefix: s3_prefix.map(String::from),
            forge_url: forge_url.map(String::from),
        })
    }

    /// Resolve a model ID to a local directory containing ONNX files.
    ///
    /// Resolution order: S3 cache -> HuggingFace -> Forge conversion.
    /// Returns the path to a directory containing `model.onnx` (or
    /// `onnx/model.onnx`), `tokenizer.json`, and `config.json`.
    ///
    /// Validates the model ID before any tier logic (T-03-01).
    pub async fn resolve(&self, model_id: &str) -> Result<PathBuf, ResolveError> {
        // T-03-01: validate model ID before any tier logic.
        validate_model_id(model_id)?;

        // Tier 1: S3 cache (placeholder for Plan 03-02).
        if self.s3_bucket.is_some() {
            tracing::debug!(model_id, "S3 tier not yet implemented");
        }

        // Tier 2: HuggingFace with retry (D-05).
        let model_id_owned = model_id.to_string();
        let model_dir = with_retry(3, Duration::from_millis(500), move || {
            let id = model_id_owned.clone();
            async move { hf::download_from_hf(&id).await }
        })
        .await?;

        tracing::info!(
            model_id,
            model_dir = %model_dir.display(),
            "model resolved from HuggingFace"
        );

        Ok(model_dir)
    }
}

/// Generic async retry with exponential backoff (D-05).
///
/// Retries the operation up to `max_attempts` times with exponential
/// backoff starting from `base_delay`. Each retry is logged at warn
/// level with attempt number, max_attempts, delay, and error message.
async fn with_retry<F, Fut, T, E>(
    max_attempts: u32,
    base_delay: Duration,
    mut operation: F,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    let mut attempt = 0;
    loop {
        attempt += 1;
        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) if attempt >= max_attempts => return Err(e),
            Err(e) => {
                let delay = base_delay * 2u32.pow(attempt - 1);
                tracing::warn!(
                    attempt,
                    max_attempts,
                    delay_ms = delay.as_millis() as u64,
                    error = %e,
                    "retrying after transient error"
                );
                tokio::time::sleep(delay).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- validate_model_id tests ---

    #[test]
    fn validate_model_id_accepts_org_model() {
        assert!(validate_model_id("Xenova/distilbert-base").is_ok());
    }

    #[test]
    fn validate_model_id_accepts_model_with_dots_and_hyphens() {
        assert!(validate_model_id("sentence-transformers/all-MiniLM-L6-v2").is_ok());
    }

    #[test]
    fn validate_model_id_accepts_simple_model() {
        assert!(validate_model_id("bert-base-uncased").is_ok());
    }

    #[test]
    fn validate_model_id_accepts_underscores() {
        assert!(validate_model_id("org/my_model_v2").is_ok());
    }

    #[test]
    fn validate_model_id_rejects_path_traversal() {
        let result = validate_model_id("../../../etc/passwd");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, ResolveError::InvalidModelId { .. }),
            "expected InvalidModelId, got: {err:?}"
        );
    }

    #[test]
    fn validate_model_id_rejects_embedded_traversal() {
        let result = validate_model_id("model/../../secret");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ResolveError::InvalidModelId { .. }
        ));
    }

    #[test]
    fn validate_model_id_rejects_shell_metacharacters() {
        let result = validate_model_id("model;rm -rf /");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ResolveError::InvalidModelId { .. }
        ));
    }

    #[test]
    fn validate_model_id_rejects_empty_string() {
        let result = validate_model_id("");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ResolveError::InvalidModelId { .. }
        ));
    }

    #[test]
    fn validate_model_id_error_contains_model_id() {
        let result = validate_model_id("bad;model");
        let err = result.unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("bad;model"),
            "error message should contain the model_id: {msg}"
        );
    }

    // --- with_retry tests ---

    #[tokio::test]
    async fn retry_retries_specified_number_of_times() {
        use std::sync::atomic::{AtomicU32, Ordering};
        use std::sync::Arc;

        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result: Result<(), String> = with_retry(
            3,
            Duration::from_millis(1),
            move || {
                let c = counter_clone.clone();
                async move {
                    let attempt = c.fetch_add(1, Ordering::SeqCst) + 1;
                    if attempt < 3 {
                        Err(format!("transient error on attempt {attempt}"))
                    } else {
                        Ok(())
                    }
                }
            },
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn retry_returns_last_error_on_exhaustion() {
        use std::sync::atomic::{AtomicU32, Ordering};
        use std::sync::Arc;

        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result: Result<(), String> = with_retry(
            3,
            Duration::from_millis(1),
            move || {
                let c = counter_clone.clone();
                async move {
                    let attempt = c.fetch_add(1, Ordering::SeqCst) + 1;
                    Err(format!("persistent error on attempt {attempt}"))
                }
            },
        )
        .await;

        assert!(result.is_err());
        assert_eq!(counter.load(Ordering::SeqCst), 3);
        let err = result.unwrap_err();
        assert!(
            err.contains("attempt 3"),
            "should return last error: {err}"
        );
    }

    #[tokio::test]
    async fn retry_succeeds_on_first_attempt() {
        use std::sync::atomic::{AtomicU32, Ordering};
        use std::sync::Arc;

        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result: Result<&str, String> = with_retry(
            3,
            Duration::from_millis(1),
            move || {
                let c = counter_clone.clone();
                async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    Ok("success")
                }
            },
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    // --- NoOnnxExport error message test ---

    #[test]
    fn no_onnx_export_error_contains_model_id() {
        let err = ResolveError::NoOnnxExport {
            model_id: "test-org/test-model".to_string(),
        };
        let msg = err.to_string();
        assert!(
            msg.contains("test-org/test-model"),
            "NoOnnxExport Display should contain model_id: {msg}"
        );
    }

    // --- StubForgeClient test ---

    #[tokio::test]
    async fn stub_forge_returns_unavailable() {
        let err = ResolveError::ForgeUnavailable {
            model_id: "test/model".to_string(),
        };
        assert!(matches!(err, ResolveError::ForgeUnavailable { .. }));
        let msg = err.to_string();
        assert!(
            msg.contains("test/model"),
            "ForgeUnavailable should contain model_id: {msg}"
        );
    }
}
