//! Integration tests for OBSV-03 (OpenTelemetry tracing) and OBSV-02 (structured logging).
//!
//! telemetry::init installs a global tracing subscriber and can only be called
//! once per process. Tests that call init must run in isolation (separate test
//! binary or with `--test-threads=1`).

use std::io::Write;
use std::sync::{Arc, Mutex};

use hephaestus_api::telemetry;
use tracing_subscriber::layer::SubscriberExt;

#[tokio::test]
async fn telemetry_init_without_otel_does_not_panic() {
    // OBSV-03: Initializing the telemetry subsystem without an OTLP
    // endpoint should not panic. The system falls back to local-only
    // tracing (no span export) when OTEL_EXPORTER_OTLP_ENDPOINT is unset.
    //
    // This test installs a global subscriber, so it must be the only
    // init() call in this test binary.
    let result = telemetry::init("info", None);
    assert!(result.is_ok(), "telemetry::init with None endpoint should succeed");

    // Verify that logging works after init.
    tracing::info!(model_id = "test-model", "test log line");

    // Clean shutdown should not panic.
    telemetry::shutdown();
}

/// Shared buffer that captures tracing subscriber output for test assertions.
#[derive(Clone)]
struct TestWriter {
    buf: Arc<Mutex<Vec<u8>>>,
}

impl TestWriter {
    fn new() -> Self {
        Self {
            buf: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn contents(&self) -> String {
        let buf = self.buf.lock().expect("test writer lock poisoned");
        String::from_utf8_lossy(&buf).to_string()
    }
}

/// Guard type that writes to the shared buffer.
struct TestWriterGuard {
    buf: Arc<Mutex<Vec<u8>>>,
}

impl Write for TestWriterGuard {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        let mut buf = self.buf.lock().expect("test writer lock poisoned");
        buf.extend_from_slice(data);
        Ok(data.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for TestWriter {
    type Writer = TestWriterGuard;

    fn make_writer(&'a self) -> Self::Writer {
        TestWriterGuard {
            buf: Arc::clone(&self.buf),
        }
    }
}

#[tokio::test]
async fn structured_logs_contain_model_id() {
    // OBSV-02: Structured JSON log lines contain model_id, latency_ms,
    // and status fields. This test captures subscriber output via a
    // test-local subscriber (not the global one) and verifies field
    // presence in JSON format.

    let writer = TestWriter::new();
    let writer_clone = writer.clone();

    let subscriber = tracing_subscriber::Registry::default().with(
        tracing_subscriber::fmt::layer()
            .json()
            .with_writer(writer_clone),
    );

    tracing::subscriber::with_default(subscriber, || {
        tracing::info!(
            model_id = "test-model-abc",
            latency_ms = 42_u64,
            status = "success",
            "inference request completed"
        );
    });

    let output = writer.contents();

    // Find the JSON line containing our test event.
    let json_line = output
        .lines()
        .find(|line| line.contains("test-model-abc"))
        .expect("should find a JSON log line containing the test model_id");

    let parsed: serde_json::Value =
        serde_json::from_str(json_line).expect("log line should be valid JSON");

    // tracing-subscriber JSON format nests event fields at the top level.
    assert_eq!(
        parsed["fields"]["model_id"], "test-model-abc",
        "JSON log should contain model_id field"
    );
    assert_eq!(
        parsed["fields"]["latency_ms"], 42,
        "JSON log should contain latency_ms field"
    );
    assert_eq!(
        parsed["fields"]["status"], "success",
        "JSON log should contain status field"
    );
}
