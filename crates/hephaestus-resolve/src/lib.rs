//! Model resolution for the Hephaestus ONNX inference runtime.
//!
//! This crate implements the 3-tier model resolution chain: S3 cache,
//! HuggingFace Hub, and Forge conversion. Callers interact only through
//! [`ModelResolver::resolve()`] -- all download, caching, and retry
//! details are hidden behind this single method (RSLV-05).

pub mod error;
pub mod forge;
pub(crate) mod hf;
pub mod resolver;
pub(crate) mod s3;

pub use error::ResolveError;
pub use forge::{ForgeClient, StubForgeClient};
pub use resolver::ModelResolver;
