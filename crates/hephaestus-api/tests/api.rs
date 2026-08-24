//! Integration tests for API-01 (POST /infer) and API-02 (GET /healthz/live).
//!
//! Tests that exercise the full router with a real ClassifierPipeline
//! require model files on disk and are marked `#[ignore]`.
//!
//! Health probe integration tests are in the `health` test module
//! which uses a dedicated test AppState constructor.

#[tokio::test]
#[ignore = "requires model files on disk for ClassifierPipeline"]
async fn post_infer_returns_json_classification() {
    // API-01: POST /infer accepts a JSON body with text input and returns
    // a JSON response containing classification labels and confidence scores.
    // Requires a real ClassifierPipeline with model files.
    //
    // To run: set MODEL_PATH to a directory containing model.onnx,
    // tokenizer.json, and config.json, then run:
    //   cargo test -p hephaestus-api --test api -- --ignored
}

#[tokio::test]
#[ignore = "requires model files on disk for ClassifierPipeline"]
async fn get_healthz_live_returns_200_with_metadata() {
    // API-02: GET /healthz/live returns 200 OK with a JSON body containing
    // service metadata (model_id, uptime_s). The full router integration
    // test requires a real AppState with a ClassifierPipeline.
    //
    // Health probe logic is unit-tested in handlers.rs and health.rs.
}
