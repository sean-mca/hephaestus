//! Feature-gated integration tests for all supported inference profiles.
//!
//! Downloads real models from HuggingFace and exercises the full pipeline:
//! construct pipeline, prepare input, execute inference, assert output.
//!
//! Run with: `cargo test -p hephaestus-core --features integration -- --nocapture`
//!
//! **Seq2seq excluded:** Fused single-pass seq2seq models with baked-in beam
//! search are not reliably available in the Xenova namespace with a compatible
//! output format. The three profiles tested (classifier, embeddings,
//! token_classifier) cover the critical inference paths.
#![cfg(feature = "integration")]

use std::path::PathBuf;

use hf_hub::HFClient;
use hephaestus_core::{
    ClassifierPipeline, EmbeddingsPipeline, ExecutionProvider, Pipeline,
    TokenClassifierPipeline,
};

// ---------------------------------------------------------------------------
// Shared helper
// ---------------------------------------------------------------------------

/// Download model files from HuggingFace and return the snapshot root directory.
///
/// Uses the HF cache so subsequent runs are instant. The returned path
/// is the parent of `onnx/` (i.e., the snapshot root where `tokenizer.json`
/// and `config.json` also live).
async fn download_model(org: &str, repo: &str, filenames: &[&str]) -> PathBuf {
    let client = HFClient::new().expect("failed to create HFClient");
    let model = client.model(org, repo);

    let mut onnx_model_path: Option<PathBuf> = None;

    for filename in filenames {
        let path = model
            .download_file()
            .filename(*filename)
            .send()
            .await
            .unwrap_or_else(|e| panic!("failed to download {filename}: {e}"));

        // Track the onnx/model.onnx path to derive the snapshot root.
        if *filename == "onnx/model.onnx" {
            onnx_model_path = Some(path);
        }
    }

    let onnx_path = onnx_model_path.expect("onnx/model.onnx must be in the filenames list");

    // onnx_path is {snapshot_root}/onnx/model.onnx
    // We want {snapshot_root}.
    onnx_path
        .parent()
        .expect("model.onnx should have parent (onnx/)")
        .parent()
        .expect("onnx/ should have parent (snapshot root)")
        .to_path_buf()
}

// ---------------------------------------------------------------------------
// Classifier tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_classifier_positive_sentiment() {
    let model_dir = download_model(
        "Xenova",
        "distilbert-base-uncased-finetuned-sst-2-english",
        &["onnx/model.onnx", "tokenizer.json", "config.json"],
    )
    .await;

    let mut pipeline = ClassifierPipeline::new(&model_dir, &ExecutionProvider::Cpu)
        .expect("failed to construct classifier pipeline");

    let prepared = pipeline
        .prepare("I love this movie!".to_string())
        .expect("failed to prepare input");

    let output = pipeline
        .execute(prepared)
        .expect("failed to execute inference");

    assert_eq!(output.label, "POSITIVE");
    assert!(
        output.score > 0.5 && output.score <= 1.0,
        "expected score in (0.5, 1.0], got {}",
        output.score
    );
}

#[tokio::test]
async fn test_classifier_negative_sentiment() {
    let model_dir = download_model(
        "Xenova",
        "distilbert-base-uncased-finetuned-sst-2-english",
        &["onnx/model.onnx", "tokenizer.json", "config.json"],
    )
    .await;

    let mut pipeline = ClassifierPipeline::new(&model_dir, &ExecutionProvider::Cpu)
        .expect("failed to construct classifier pipeline");

    let prepared = pipeline
        .prepare("This is terrible and I hate it".to_string())
        .expect("failed to prepare input");

    let output = pipeline
        .execute(prepared)
        .expect("failed to execute inference");

    assert_eq!(output.label, "NEGATIVE");
    assert!(
        output.score > 0.5 && output.score <= 1.0,
        "expected score in (0.5, 1.0], got {}",
        output.score
    );
}

// ---------------------------------------------------------------------------
// Embeddings test
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_embeddings_dimension_and_norm() {
    let model_dir = download_model(
        "Xenova",
        "multi-qa-distilbert-cos-v1",
        &["onnx/model.onnx", "tokenizer.json"],
    )
    .await;

    let mut pipeline = EmbeddingsPipeline::new(&model_dir, &ExecutionProvider::Cpu)
        .expect("failed to construct embeddings pipeline");

    let prepared = pipeline
        .prepare("How do I reset my password?".to_string())
        .expect("failed to prepare input");

    let embedding = pipeline
        .execute(prepared)
        .expect("failed to execute inference");

    // Assert 768-dimensional output.
    assert_eq!(
        embedding.len(),
        768,
        "expected 768-dim embedding, got {}",
        embedding.len()
    );

    // Assert all values are finite.
    for (i, &val) in embedding.iter().enumerate() {
        assert!(
            val.is_finite(),
            "embedding[{i}] is not finite: {val}"
        );
    }

    // Assert L2 norm is approximately 1.0 (unit vector).
    let l2_norm: f32 = embedding.iter().map(|v| v * v).sum::<f32>().sqrt();
    assert!(
        (l2_norm - 1.0).abs() < 1e-4,
        "expected L2 norm ~1.0, got {l2_norm}"
    );
}

// ---------------------------------------------------------------------------
// Token classifier (NER) test
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_token_classifier_ner_entities() {
    let model_dir = download_model(
        "Xenova",
        "bert-base-NER",
        &["onnx/model.onnx", "tokenizer.json", "config.json"],
    )
    .await;

    let mut pipeline = TokenClassifierPipeline::new(&model_dir, &ExecutionProvider::Cpu)
        .expect("failed to construct token classifier pipeline");

    let prepared = pipeline
        .prepare("John Smith works at Google in Mountain View, California.".to_string())
        .expect("failed to prepare input");

    let entities = pipeline
        .execute(prepared)
        .expect("failed to execute inference");

    // At least one entity should be returned.
    assert!(
        !entities.is_empty(),
        "expected at least one NER entity, got none"
    );

    // All entity scores must be in [0.0, 1.0] -- validates softmax fix from Plan 08-01.
    for entity in &entities {
        assert!(
            entity.score >= 0.0 && entity.score <= 1.0,
            "entity score out of [0.0, 1.0] range: {} (entity: {:?})",
            entity.score,
            entity
        );
    }

    // At least one PER entity (for "John Smith").
    let has_per = entities.iter().any(|e| e.entity == "PER");
    assert!(
        has_per,
        "expected at least one PER entity for 'John Smith'; entities: {entities:?}"
    );

    // At least one ORG entity (for "Google").
    let has_org = entities.iter().any(|e| e.entity == "ORG");
    assert!(
        has_org,
        "expected at least one ORG entity for 'Google'; entities: {entities:?}"
    );

    // Each entity has valid span (start < end) and non-empty word.
    for entity in &entities {
        assert!(
            entity.start < entity.end,
            "entity span invalid: start={} >= end={} (entity: {:?})",
            entity.start,
            entity.end,
            entity
        );
        assert!(
            !entity.word.is_empty(),
            "entity word is empty (entity: {:?})",
            entity
        );
    }
}
