//! Forge conversion service client.
//!
//! Defines the [`ForgeClient`] trait for requesting model conversion
//! to ONNX format and the [`StubForgeClient`] stub implementation.
//! Phase 3 ships the stub; Phase 5 provides the real HTTP client
//! using reqwest (D-08, D-10).

use crate::error::ResolveError;

/// Forge service client for converting models to ONNX format (D-10).
///
/// Phase 3 ships [`StubForgeClient`] which always returns
/// [`ResolveError::ForgeUnavailable`]. Phase 5 provides the real
/// HTTP implementation using reqwest to POST to the Forge service.
///
/// The trait follows the Ousterhout deep module pattern with a single
/// `convert()` method hiding all conversion complexity.
#[cfg_attr(test, mockall::automock)]
pub trait ForgeClient: Send + Sync {
    /// Request model conversion to ONNX format.
    ///
    /// Returns the S3 paths of the converted model files on success,
    /// or a [`ResolveError`] on failure.
    fn convert(
        &self,
        model_id: &str,
    ) -> impl std::future::Future<Output = Result<Vec<String>, ResolveError>> + Send;
}

/// Stub Forge client that always returns an unavailable error (D-10).
///
/// Used when `FORGE_URL` is not configured. Returns a clear error
/// message indicating the model has no ONNX export and the Forge
/// service is not available (D-04).
pub struct StubForgeClient;

impl ForgeClient for StubForgeClient {
    async fn convert(&self, model_id: &str) -> Result<Vec<String>, ResolveError> {
        Err(ResolveError::ForgeUnavailable {
            model_id: model_id.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stub_forge_returns_forge_unavailable() {
        let client = StubForgeClient;
        let result = client.convert("org/model").await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, ResolveError::ForgeUnavailable { ref model_id } if model_id == "org/model"),
            "expected ForgeUnavailable with model_id 'org/model', got: {err:?}"
        );
    }

    #[tokio::test]
    async fn stub_forge_error_message_mentions_model() {
        let client = StubForgeClient;
        let err = client.convert("test-org/test-model").await.unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("test-org/test-model"),
            "error message should contain the model ID: {msg}"
        );
        assert!(
            msg.contains("Forge"),
            "error message should mention Forge: {msg}"
        );
    }

    #[tokio::test]
    async fn stub_forge_error_message_mentions_configuration() {
        let client = StubForgeClient;
        let err = client.convert("some/model").await.unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("FORGE_URL") || msg.contains("not configured"),
            "error message should mention configuration: {msg}"
        );
    }

    #[test]
    fn forge_client_trait_has_single_method() {
        // Compile-time verification: ForgeClient trait has exactly
        // one required method (convert). This test documents the
        // Ousterhout deep module constraint (D-10).
        //
        // If additional methods are added to ForgeClient, this test
        // still compiles but serves as documentation that the trait
        // should remain minimal.
        fn _assert_forge_client_impl<T: ForgeClient>() {}
        _assert_forge_client_impl::<StubForgeClient>();
    }
}
