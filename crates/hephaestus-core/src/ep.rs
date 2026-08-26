//! Execution provider selection for ONNX Runtime sessions.
//!
//! Maps the `EXECUTION_PROVIDER` configuration value to the concrete
//! [`ort::ep`] types that register hardware-accelerated backends on a
//! session builder.

use std::fmt;
use std::str::FromStr;

use ort::ep::ExecutionProviderDispatch;

use crate::error::CoreError;

/// ONNX Runtime execution provider selection.
///
/// Each variant corresponds to a hardware backend. GPU variants
/// require the matching Cargo feature to be compiled in; requesting
/// one without the feature produces a clear [`CoreError::Config`]
/// at startup rather than a silent CPU fallback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionProvider {
    /// CPU backend (always available, the default).
    Cpu,
    /// NVIDIA CUDA backend (requires `cuda` feature).
    Cuda,
    /// NVIDIA TensorRT backend (requires `tensorrt` feature).
    TensorRt,
    /// Apple CoreML backend (requires `coreml` feature).
    CoreMl,
}

impl fmt::Display for ExecutionProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cpu => write!(f, "cpu"),
            Self::Cuda => write!(f, "cuda"),
            Self::TensorRt => write!(f, "tensorrt"),
            Self::CoreMl => write!(f, "coreml"),
        }
    }
}

impl FromStr for ExecutionProvider {
    type Err = CoreError;

    /// Parse an execution provider name (case-insensitive).
    ///
    /// Accepted values: `cpu`, `cuda`, `tensorrt`, `coreml`.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::Config`] for unrecognised values, listing
    /// the accepted set.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "cpu" => Ok(Self::Cpu),
            "cuda" => Ok(Self::Cuda),
            "tensorrt" => Ok(Self::TensorRt),
            "coreml" => Ok(Self::CoreMl),
            other => Err(CoreError::Config(format!(
                "unknown execution provider '{other}'; accepted values: cpu, cuda, tensorrt, coreml",
            ))),
        }
    }
}

impl ExecutionProvider {
    /// Convert to the [`ort`] execution provider dispatches to register
    /// on a session builder.
    ///
    /// - `Cpu` returns an empty vec (ort uses CPU by default when no
    ///   EPs are registered).
    /// - GPU variants return a single-element vec with the configured
    ///   provider, gated behind the corresponding Cargo feature.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::Config`] if the required Cargo feature was
    /// not compiled in (T-EP-01 strict allowlist).
    pub fn to_ort_providers(&self) -> Result<Vec<ExecutionProviderDispatch>, CoreError> {
        match self {
            Self::Cpu => Ok(Vec::new()),
            Self::Cuda => {
                #[cfg(feature = "cuda")]
                {
                    Ok(vec![ort::ep::CUDA::default().build()])
                }
                #[cfg(not(feature = "cuda"))]
                {
                    Err(CoreError::Config(
                        "EXECUTION_PROVIDER=cuda requires the 'cuda' cargo feature; \
                         rebuild with: cargo build --features cuda"
                            .to_string(),
                    ))
                }
            }
            Self::TensorRt => {
                #[cfg(feature = "tensorrt")]
                {
                    Ok(vec![ort::ep::TensorRT::default().build()])
                }
                #[cfg(not(feature = "tensorrt"))]
                {
                    Err(CoreError::Config(
                        "EXECUTION_PROVIDER=tensorrt requires the 'tensorrt' cargo feature; \
                         rebuild with: cargo build --features tensorrt"
                            .to_string(),
                    ))
                }
            }
            Self::CoreMl => {
                #[cfg(feature = "coreml")]
                {
                    Ok(vec![ort::ep::CoreML::default().build()])
                }
                #[cfg(not(feature = "coreml"))]
                {
                    Err(CoreError::Config(
                        "EXECUTION_PROVIDER=coreml requires the 'coreml' cargo feature; \
                         rebuild with: cargo build --features coreml"
                            .to_string(),
                    ))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_all_valid_variants() {
        assert_eq!("cpu".parse::<ExecutionProvider>().unwrap(), ExecutionProvider::Cpu);
        assert_eq!("cuda".parse::<ExecutionProvider>().unwrap(), ExecutionProvider::Cuda);
        assert_eq!("tensorrt".parse::<ExecutionProvider>().unwrap(), ExecutionProvider::TensorRt);
        assert_eq!("coreml".parse::<ExecutionProvider>().unwrap(), ExecutionProvider::CoreMl);
    }

    #[test]
    fn parse_case_insensitive() {
        assert_eq!("CPU".parse::<ExecutionProvider>().unwrap(), ExecutionProvider::Cpu);
        assert_eq!("CUDA".parse::<ExecutionProvider>().unwrap(), ExecutionProvider::Cuda);
        assert_eq!("TensorRT".parse::<ExecutionProvider>().unwrap(), ExecutionProvider::TensorRt);
        assert_eq!("CoreML".parse::<ExecutionProvider>().unwrap(), ExecutionProvider::CoreMl);
    }

    #[test]
    fn parse_rejects_unknown() {
        let err = "vulkan".parse::<ExecutionProvider>().unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("unknown execution provider"), "got: {msg}");
        assert!(msg.contains("vulkan"), "should echo the bad value: {msg}");
        assert!(msg.contains("cpu, cuda, tensorrt, coreml"), "should list accepted: {msg}");
    }

    #[test]
    fn display_matches_parse_input() {
        for name in &["cpu", "cuda", "tensorrt", "coreml"] {
            let ep: ExecutionProvider = name.parse().unwrap();
            assert_eq!(&ep.to_string(), *name);
        }
    }

    #[test]
    fn cpu_returns_empty_providers() {
        let providers = ExecutionProvider::Cpu.to_ort_providers().unwrap();
        assert!(providers.is_empty());
    }

    // GPU variants without features compiled in should return Config error.
    #[cfg(not(feature = "cuda"))]
    #[test]
    fn cuda_without_feature_returns_error() {
        let err = ExecutionProvider::Cuda.to_ort_providers().unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("cuda"), "should mention cuda: {msg}");
        assert!(msg.contains("cargo feature"), "should mention feature: {msg}");
    }

    #[cfg(not(feature = "tensorrt"))]
    #[test]
    fn tensorrt_without_feature_returns_error() {
        let err = ExecutionProvider::TensorRt.to_ort_providers().unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("tensorrt"), "should mention tensorrt: {msg}");
    }

    #[cfg(not(feature = "coreml"))]
    #[test]
    fn coreml_without_feature_returns_error() {
        let err = ExecutionProvider::CoreMl.to_ort_providers().unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("coreml"), "should mention coreml: {msg}");
    }
}
