//! Protobuf definitions and generated gRPC types for the Hephaestus inference API.
//!
//! This crate provides the compiled protobuf types and service traits
//! generated from `proto/hephaestus/v1/inference.proto`. The generated
//! [`v1`] module contains [`InferRequest`](v1::InferRequest),
//! [`InferResponse`](v1::InferResponse), and the
//! [`InferenceService`](v1::inference_service_server::InferenceService) trait.

/// Generated gRPC types for the `hephaestus.v1` package.
pub mod v1 {
    tonic::include_proto!("hephaestus.v1");
}

/// Encoded file descriptor set for gRPC server reflection.
pub const FILE_DESCRIPTOR_SET: &[u8] =
    tonic::include_file_descriptor_set!("hephaestus_descriptor");

#[cfg(test)]
mod tests {
    use super::v1::{InferRequest, InferResponse};
    use prost::Message;

    #[test]
    fn infer_request_roundtrip() {
        let original = InferRequest {
            text: "hello".into(),
        };

        let mut buf = Vec::new();
        original.encode(&mut buf).expect("encode should succeed");

        let decoded = InferRequest::decode(&buf[..]).expect("decode should succeed");
        assert_eq!(decoded.text, "hello");
    }

    #[test]
    fn infer_response_roundtrip() {
        let json_payload = br#"{"label":"POSITIVE","score":0.99}"#;
        let original = InferResponse {
            model_id: "test/model".into(),
            latency_ms: 42,
            result_json: json_payload.to_vec().into(),
        };

        let mut buf = Vec::new();
        original.encode(&mut buf).expect("encode should succeed");

        let decoded = InferResponse::decode(&buf[..]).expect("decode should succeed");
        assert_eq!(decoded.model_id, "test/model");
        assert_eq!(decoded.latency_ms, 42);

        let parsed: serde_json::Value =
            serde_json::from_slice(&decoded.result_json).expect("result_json should be valid JSON");
        assert_eq!(parsed["label"], "POSITIVE");
        assert_eq!(parsed["score"], 0.99);
    }

    #[test]
    fn file_descriptor_set_is_nonempty() {
        assert!(
            super::FILE_DESCRIPTOR_SET.len() > 0,
            "FILE_DESCRIPTOR_SET should contain the encoded proto descriptors"
        );
    }
}
