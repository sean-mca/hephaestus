//! End-to-end integration test for the classifier pipeline.
//!
//! Downloads a real distilbert model from HuggingFace and exercises
//! the full classify flow: construct pipeline, prepare input, execute.
//!
//! Marked `#[ignore]` because it requires internet access and downloads
//! a ~260MB model on first run. Subsequent runs use the HF cache.
//!
//! Run with: `cargo test -p hephaestus-core --test classifier_e2e -- --ignored`

use std::path::PathBuf;

use hf_hub::HFClient;
use hephaestus_core::{ClassifierPipeline, Pipeline};

/// Downloads the distilbert-sst2 test model files to the HF cache.
///
/// Returns the snapshot root directory containing:
///   - `onnx/model.onnx`
///   - `tokenizer.json`
///   - `config.json`
async fn download_test_model() -> PathBuf {
    let client = HFClient::new().expect("failed to create HFClient");
    let model = client.model("Xenova", "distilbert-base-uncased-finetuned-sst-2-english");

    // Download all required files (returns cached paths).
    let model_path = model
        .download_file()
        .filename("onnx/model.onnx")
        .send()
        .await
        .expect("failed to download onnx/model.onnx");

    let _tokenizer_path = model
        .download_file()
        .filename("tokenizer.json")
        .send()
        .await
        .expect("failed to download tokenizer.json");

    let _config_path = model
        .download_file()
        .filename("config.json")
        .send()
        .await
        .expect("failed to download config.json");

    // model_path is {snapshot_root}/onnx/model.onnx
    // We want {snapshot_root} so that Pipeline::new can find:
    //   {snapshot_root}/onnx/model.onnx
    //   {snapshot_root}/tokenizer.json
    //   {snapshot_root}/config.json
    model_path
        .parent()
        .expect("model.onnx should have parent (onnx/)")
        .parent()
        .expect("onnx/ should have parent (snapshot root)")
        .to_path_buf()
}

#[tokio::test]
#[ignore]
async fn classify_positive_sentiment() {
    let model_dir = download_test_model().await;

    let mut pipeline =
        ClassifierPipeline::new(&model_dir).expect("failed to construct classifier pipeline");

    let prepared = pipeline
        .prepare("I love this movie!".to_string())
        .expect("failed to prepare input");

    let output = pipeline
        .execute(prepared)
        .expect("failed to execute inference");

    assert_eq!(output.label, "POSITIVE");
    assert!(
        output.score > 0.5,
        "expected confidence > 0.5, got {}",
        output.score
    );
}
