//! Protobuf definitions and generated gRPC types for the Hephaestus inference API.
//!
//! This crate contains the proto-generated Rust types for the `hephaestus.v1`
//! gRPC service, including request/response messages, result variant types,
//! and the `InferenceService` server trait. The [`FILE_DESCRIPTOR_SET`] constant
//! provides encoded file descriptors for gRPC server reflection.

/// Generated types for the `hephaestus.v1` protobuf package.
pub mod v1 {
    tonic::include_proto!("hephaestus.v1");
}

/// Encoded file descriptor set for gRPC server reflection.
///
/// Pass this to `tonic_reflection::server::Builder::register_encoded_file_descriptor_set`
/// to enable runtime service discovery via the gRPC reflection protocol.
pub const FILE_DESCRIPTOR_SET: &[u8] =
    tonic::include_file_descriptor_set!("hephaestus_descriptor");

#[cfg(test)]
mod tests {
    use super::v1::*;
    use prost::Message;

    #[test]
    fn infer_request_roundtrip() {
        let request = InferRequest {
            text: "hello".to_string(),
        };

        let mut buf = Vec::new();
        request.encode(&mut buf).expect("encode should succeed");

        let decoded = InferRequest::decode(&buf[..]).expect("decode should succeed");
        assert_eq!(decoded.text, "hello");
    }

    #[test]
    fn infer_response_with_classification() {
        let response = InferResponse {
            model_id: "test/model".to_string(),
            latency_ms: 42,
            result: Some(infer_response::Result::Classification(
                ClassificationResult {
                    label: "POSITIVE".to_string(),
                    score: 0.9998,
                },
            )),
        };

        assert_eq!(response.model_id, "test/model");
        assert_eq!(response.latency_ms, 42);

        match response.result {
            Some(infer_response::Result::Classification(c)) => {
                assert_eq!(c.label, "POSITIVE");
                assert!((c.score - 0.9998).abs() < 1e-4);
            }
            other => panic!("expected Classification, got {other:?}"),
        }
    }

    #[test]
    fn infer_response_with_embedding() {
        let values = vec![0.1, 0.2, 0.3, 0.4, 0.5];
        let response = InferResponse {
            model_id: "test/embed".to_string(),
            latency_ms: 10,
            result: Some(infer_response::Result::Embedding(EmbeddingResult {
                values: values.clone(),
            })),
        };

        match response.result {
            Some(infer_response::Result::Embedding(e)) => {
                assert_eq!(e.values.len(), 5);
                assert_eq!(e.values, values);
            }
            other => panic!("expected Embedding, got {other:?}"),
        }
    }

    #[test]
    fn file_descriptor_set_is_nonempty() {
        assert!(
            super::FILE_DESCRIPTOR_SET.len() > 0,
            "FILE_DESCRIPTOR_SET should contain encoded descriptors"
        );
    }

    #[test]
    fn entity_message_fields() {
        let entity = Entity {
            word: "Google".to_string(),
            entity: "ORG".to_string(),
            score: 0.95,
            start: 10,
            end: 16,
        };

        assert_eq!(entity.word, "Google");
        assert_eq!(entity.entity, "ORG");
        assert!((entity.score - 0.95).abs() < 1e-4);
        assert_eq!(entity.start, 10);
        assert_eq!(entity.end, 16);
    }
}
