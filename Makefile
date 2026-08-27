.PHONY: test-inference test-classifier test-ner test-embeddings build

PORT ?= 8090
BINARY := ./target/release/hephaestus
RUST_LOG := hephaestus=info
STORAGE_TYPE := none
CURL := curl -s -X POST http://localhost:$(PORT)/infer -H "Content-Type: application/json"

build:
	cargo build --release

test-inference: test-classifier test-ner test-embeddings
	@echo ""
	@echo "All inference tests passed."

test-classifier: build
	@echo ""
	@echo "════════════════════════════════════════"
	@echo " CLASSIFIER — Xenova/distilbert-base-uncased-finetuned-sst-2-english"
	@echo "════════════════════════════════════════"
	@RUST_LOG=$(RUST_LOG) MODEL_ID=Xenova/distilbert-base-uncased-finetuned-sst-2-english \
		STORAGE_TYPE=$(STORAGE_TYPE) PORT=$(PORT) $(BINARY) 2>/dev/null & \
		PID=$$!; \
		trap "kill $$PID 2>/dev/null; wait $$PID 2>/dev/null" EXIT; \
		until curl -sf http://localhost:$(PORT)/healthz/ready >/dev/null 2>&1; do sleep 0.3; done; \
		echo ""; \
		echo '→ POST /infer {"text": "This product is amazing"}'; \
		$(CURL) -d '{"text": "This product is amazing"}' | python3 -m json.tool; \
		echo ""; \
		echo '→ POST /infer {"text": "This is terrible and broken"}'; \
		$(CURL) -d '{"text": "This is terrible and broken"}' | python3 -m json.tool; \
		kill $$PID 2>/dev/null; wait $$PID 2>/dev/null
	@rm -rf ~/.cache/huggingface/hub/models--Xenova--distilbert-base-uncased-finetuned-sst-2-english

test-ner: build
	@echo ""
	@echo "════════════════════════════════════════"
	@echo " NER — Xenova/bert-base-NER"
	@echo "════════════════════════════════════════"
	@RUST_LOG=$(RUST_LOG) MODEL_ID=Xenova/bert-base-NER \
		STORAGE_TYPE=$(STORAGE_TYPE) PORT=$(PORT) $(BINARY) 2>/dev/null & \
		PID=$$!; \
		trap "kill $$PID 2>/dev/null; wait $$PID 2>/dev/null" EXIT; \
		until curl -sf http://localhost:$(PORT)/healthz/ready >/dev/null 2>&1; do sleep 0.3; done; \
		echo ""; \
		echo '→ POST /infer {"text": "John Smith works at Google in Mountain View, California."}'; \
		$(CURL) -d '{"text": "John Smith works at Google in Mountain View, California."}' | python3 -m json.tool; \
		echo ""; \
		echo '→ POST /infer {"text": "Apple and Microsoft are tech companies."}'; \
		$(CURL) -d '{"text": "Apple and Microsoft are tech companies."}' | python3 -m json.tool; \
		kill $$PID 2>/dev/null; wait $$PID 2>/dev/null
	@rm -rf ~/.cache/huggingface/hub/models--Xenova--bert-base-NER

test-embeddings: build
	@echo ""
	@echo "════════════════════════════════════════"
	@echo " EMBEDDINGS — Xenova/multi-qa-distilbert-cos-v1"
	@echo "════════════════════════════════════════"
	@RUST_LOG=$(RUST_LOG) MODEL_ID=Xenova/multi-qa-distilbert-cos-v1 \
		STORAGE_TYPE=$(STORAGE_TYPE) PORT=$(PORT) $(BINARY) 2>/dev/null & \
		PID=$$!; \
		trap "kill $$PID 2>/dev/null; wait $$PID 2>/dev/null" EXIT; \
		until curl -sf http://localhost:$(PORT)/healthz/ready >/dev/null 2>&1; do sleep 0.3; done; \
		echo ""; \
		echo '→ POST /infer {"text": "How do I reset my password?"}'; \
		$(CURL) -d '{"text": "How do I reset my password?"}' | python3 -c "\
import json, sys; \
d = json.load(sys.stdin); \
emb = d['embedding']; \
print(json.dumps({'dim': len(emb), 'first_5': emb[:5], 'model_id': d['model_id'], 'latency_ms': d['latency_ms']}, indent=4))"; \
		kill $$PID 2>/dev/null; wait $$PID 2>/dev/null
	@rm -rf ~/.cache/huggingface/hub/models--Xenova--multi-qa-distilbert-cos-v1
