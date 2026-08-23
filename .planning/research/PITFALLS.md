# Pitfalls Research

**Domain:** ONNX Model Inference Runtime (Rust)
**Researched:** 2026-08-22
**Confidence:** MEDIUM

## Critical Pitfalls

### Pitfall 1: GPU Execution Provider Silently Falls Back to CPU

**What goes wrong:**
The `ort` crate registers execution providers in the order they are passed, but silently falls back to the CPU provider if none of the requested GPU providers (CUDA, TensorRT) are available. Your inference server starts, accepts requests, and returns correct results -- but runs 10-100x slower than expected because it is using CPU without any error or warning.

**Why it happens:**
This is by design in ONNX Runtime. The fallback philosophy is "better slow than broken." Developers assume that requesting CUDA means they will get CUDA, but the provider can be unavailable due to missing libraries, driver version mismatches, or container misconfiguration.

**How to avoid:**
Call `ExecutionProvider::cuda().is_available()` at startup before building the session. If the expected provider is not available, fail loudly with a clear error message listing the CUDA/cuDNN/driver versions detected vs. required. Never silently degrade to CPU in a production inference service.

```rust
let cuda = ExecutionProvider::cuda();
if !cuda.is_available() {
    panic!("CUDA EP unavailable. Check CUDA {}, cuDNN {}, driver {} versions", ...);
}
```

**Warning signs:**
- Inference latency 10-100x higher than expected benchmarks
- GPU utilization at 0% while inference is running
- `nvidia-smi` shows no process using the GPU

**Phase to address:**
Core session management phase (Phase 1). This check must be in the earliest working code.

---

### Pitfall 2: CUDA/cuDNN/Driver Version Matrix Hell

**What goes wrong:**
ONNX Runtime requires an exact version match between the CUDA toolkit, cuDNN library, and NVIDIA driver. A mismatch causes crashes (access violations, undefined symbol errors), silent failures (provider not loading), or container startup failures ("no CUDA-capable device detected").

**Why it happens:**
The ONNX Runtime binary is compiled against specific CUDA/cuDNN versions. The `ort` crate's download strategy fetches a pre-built ORT binary pinned to particular CUDA versions. The container's NVIDIA driver and the host's driver must also be compatible. This creates a four-way version dependency: ort crate version -> ORT binary version -> CUDA version -> driver version.

**How to avoid:**
Pin all four versions explicitly in the Dockerfile and document them in a compatibility matrix. Use NVIDIA's official CUDA base images. Test the exact container image on the exact GPU hardware before any other development.

Pin in Cargo.toml:
```toml
[dependencies]
ort = { version = "=2.0.x", features = ["cuda"] }
```

Pin in Dockerfile:
```dockerfile
FROM nvidia/cuda:12.x.y-cudnn9-runtime-ubuntu22.04
# Lock ORT_DYLIB_PATH to the specific binary
```

**Warning signs:**
- DLL/SO loading crashes at startup
- `undefined symbol: nvrtcGetProgramLogSize` errors
- "CUDA execution provider is not enabled in this build"
- Works on dev machine but fails in container

**Phase to address:**
Infrastructure/Docker phase. Must be solved before any GPU inference work begins.

---

### Pitfall 3: GPU Memory Never Released After Session Drop

**What goes wrong:**
When using CUDAExecutionProvider (and especially TensorRT), GPU memory allocated during inference is never fully returned to the OS even after dropping the ONNX Runtime session. In a long-running inference server, GPU VRAM grows monotonically and eventually causes OOM kills.

**Why it happens:**
ONNX Runtime's CUDA memory allocator uses an internal cache that grows to accommodate peak usage but never shrinks. After processing a long-sequence or large-batch request, the memory high-water mark becomes the new baseline. This is a known, long-standing issue documented across multiple GitHub issues (microsoft/onnxruntime #26831, #24376, #11801, #25996). TensorRT optimization specifically triggers leaks that normal ONNX models do not have.

**How to avoid:**
- For Hephaestus's one-model-per-pod architecture, this is partially mitigated: the pod runs one model, so the memory ceiling is predictable. Set the k8s memory limit to the known peak + headroom.
- Do NOT implement hot model reloading by creating/dropping sessions. If a model needs updating, replace the entire pod.
- Avoid variable batch sizes that create new memory allocation patterns. Pad batches to fixed sizes (e.g., always pad to max_batch_size).
- Monitor GPU memory via `nvidia-smi` metrics exported to Prometheus. Alert when VRAM exceeds 80% of the limit.

**Warning signs:**
- GPU memory usage climbing over hours/days
- OOM kills on pods that initially ran fine
- Memory not recovering after traffic drops to zero

**Phase to address:**
GPU execution provider phase and k8s deployment phase. Memory monitoring must be in place before production.

---

### Pitfall 4: Model Loading Cold Start Kills Readiness Probes

**What goes wrong:**
ONNX Runtime applies graph optimizations (constant folding, node elimination, node fusion) when loading a model. For large models, this takes 10-60+ seconds. Kubernetes readiness probes with default timeouts (10s) kill the pod before the model finishes loading, creating an infinite restart loop.

**Why it happens:**
Developers configure standard web-service readiness probes (short timeouts, frequent checks) without accounting for model loading time. The pod starts, the HTTP/gRPC server is not yet listening (because model loading blocks startup), and k8s decides the pod is broken.

**How to avoid:**
1. Start the HTTP/gRPC health endpoint BEFORE loading the model. Return `NOT_READY` (503) on the readiness probe while loading, `READY` (200) after.
2. Use Kubernetes `startupProbe` with generous `failureThreshold * periodSeconds` (e.g., 300s for large models).
3. Pre-optimize the ONNX model file (save the optimized graph to disk via `SessionOptions::optimized_model_filepath`). Load the pre-optimized file in production to skip runtime optimization.
4. Run a warmup inference with dummy input before flipping readiness to `READY`.

```yaml
startupProbe:
  httpGet:
    path: /health/startup
    port: 8080
  failureThreshold: 30
  periodSeconds: 10  # 300s total budget for model loading
readinessProbe:
  httpGet:
    path: /health/ready
    port: 8080
  periodSeconds: 5
  failureThreshold: 3
```

**Warning signs:**
- Pods in CrashLoopBackOff with no application error logs
- Pod restarts increasing over time
- `kubectl describe pod` shows "Readiness probe failed"

**Phase to address:**
Initial server skeleton phase (health endpoints) and k8s deployment phase.

---

### Pitfall 5: Thread Pool Misconfiguration Destroys Inference Performance

**What goes wrong:**
ONNX Runtime's default threading configuration creates one intra-op thread per physical core and one inter-op thread per physical core. In a containerized environment, the runtime sees ALL host cores (not the cgroup CPU limit), spawning far more threads than allocated, causing massive context-switching overhead and contention.

**Why it happens:**
ONNX Runtime reads `/proc/cpuinfo` (Linux) to detect cores, which reports host-level core counts even inside containers. A container limited to 4 CPUs on a 64-core host spawns 64 intra-op threads competing for 4 CPU time slices. Additionally, explicitly setting `intra_op_num_threads` disables thread affinity, which can increase cache miss rates.

**How to avoid:**
- Explicitly set `intra_op_num_threads` to match the container's CPU limit (read from cgroup): `SessionOptions::intra_op_num_threads(cpu_limit)`.
- For single-model-per-pod with GPU: set `intra_op_num_threads` to 1-2 (CPU threads are just for data prep, GPU does the heavy work).
- Leave execution mode as `ORT_SEQUENTIAL` unless the model has significant branching/parallelism (most classifiers and embedding models do not).
- Do NOT enable `ORT_PARALLEL` execution mode blindly -- it adds coordination overhead that hurts non-branching models.
- On NUMA systems, configure thread affinity to keep threads on the same NUMA node (~20% penalty for cross-node).

**Warning signs:**
- CPU utilization at 100% but throughput is low
- High context-switching counts in container metrics
- Inference latency variance is high (ONNX Runtime's constant cost model causes load imbalance)

**Phase to address:**
Performance tuning phase, after basic inference is working. Must be tuned per deployment target.

---

### Pitfall 6: Tokenizer/Model Input Shape Mismatch

**What goes wrong:**
The tokenizer produces input tensors with shapes (padding length, attention mask dimensions, special token counts) that do not match what the ONNX model was exported to expect. This causes either runtime errors ("shape mismatch") or, worse, silently wrong inference results from truncated or mis-padded inputs.

**Why it happens:**
The ONNX model was exported with specific dynamic axes and input expectations, but the tokenizer is configured independently. Common mismatches: (1) tokenizer pads to longest-in-batch but model expects fixed max_length, (2) model exported without dynamic batch axis so batch_size > 1 fails, (3) special tokens (CLS, SEP) counted differently between tokenizer config and model export, (4) attention_mask or token_type_ids expected by model but not produced by tokenizer.

**How to avoid:**
- Load the model's expected input names and shapes from the ONNX graph at startup. Validate that the tokenizer output matches these shapes BEFORE serving any requests.
- Always set `padding = PaddingStrategy::Fixed(max_length)` and `truncation = true` with `max_length` matching the model's exported sequence length.
- Write an integration test that tokenizes known inputs, runs them through the model, and validates output shape and approximate values against a Python reference.
- If the model was exported with dynamic axes, verify which axes are dynamic (`batch`, `sequence_length`) vs. fixed.

```rust
// At startup, validate tokenizer output matches model inputs
let model_inputs = session.inputs();
for input in model_inputs {
    let name = input.name();
    let shape = input.input_type(); // Check dimensions
    // Verify tokenizer produces this input name with compatible shape
}
```

**Warning signs:**
- Shape mismatch errors on the first request
- Model produces constant outputs regardless of input (wrong padding/truncation)
- Different results between Rust inference and Python reference

**Phase to address:**
Pre/post-processing pipeline phase. Must be validated with reference tests before any model type profile is considered complete.

---

### Pitfall 7: HuggingFace Hub Downloads Stall or Rate-Limit in Production

**What goes wrong:**
Model downloads from HuggingFace Hub fail silently (TCP connection stalls), hit rate limits (429), or take far longer than expected. In k8s, this causes pods to fail startup probes while downloading multi-GB model files. At scale, multiple pods starting simultaneously exhaust the rate limit (3000 requests per 5-minute window for anonymous, counting each chunk/redirect separately).

**Why it happens:**
Corporate firewalls, load balancers, and ISPs silently drop idle TCP connections during large downloads. The hf-hub Rust crate does not use hf_transfer (the Rust-based chunked downloader). Multiple pods starting simultaneously (e.g., during a deployment rollout) each try to download the same model, multiplying API requests.

**How to avoid:**
- The S3 cache (Hephaestus's 3-tier resolution) is the primary mitigation. Models should be in S3 99% of the time. HuggingFace download is a fallback.
- Always use an authenticated HuggingFace token (50,000 requests/hour vs. 3,000/5min anonymous).
- Implement download timeouts with per-chunk retry (not whole-file retry).
- Use `RollingUpdate` with `maxSurge=1` to prevent multiple pods downloading simultaneously.
- Set a download timeout in the hf-hub client. If download does not complete within the startup probe window, fail fast rather than hanging.

**Warning signs:**
- Pod startup times varying wildly (seconds vs. minutes)
- 429 errors in logs during deployment rollouts
- Downloads hanging at a specific percentage
- Startup probe failures only during rollouts, not single-pod restarts

**Phase to address:**
Model resolution phase (S3 cache + HF download). The S3-first architecture makes this less critical but the fallback path must be robust.

---

### Pitfall 8: Dynamic Batching Adds Latency Without Clear Throughput Gains for Small Models

**What goes wrong:**
Implementing dynamic batching for classifier and embedding models adds code complexity, request queuing latency, and p99 latency spikes without meaningful throughput improvement. Small models (classifiers, embeddings) often complete inference in 1-5ms -- the batching overhead (queue wait, padding, result routing) exceeds the inference time itself.

**Why it happens:**
Dynamic batching is borrowed from LLM/large-model serving patterns (Triton, vLLM) where batch formation amortizes expensive GPU kernel launches. For small models on CPU or even GPU, the per-request overhead is already minimal. Batching adds: (1) a queue wait timeout (typically 5-50ms), (2) padding short inputs to match the longest in the batch, (3) routing results back to individual requests, (4) head-of-line blocking when a slow request holds up the batch.

**How to avoid:**
- Start with NO batching (Hephaestus's design already defaults to single-request). Only add batching when profiling shows GPU utilization is low due to kernel launch overhead.
- If batching is needed, use fixed small batch sizes (2-4) with very short timeouts (1-2ms), not large dynamic batches.
- Never batch across different model types or input shapes.
- Measure p99 latency, not just throughput, when evaluating batching.

**Warning signs:**
- p99 latency 3-10x higher than p50
- GPU utilization unchanged between batched and unbatched
- Queue depth growing while GPU sits idle between batches

**Phase to address:**
Batching phase (explicitly marked as optional/configurable in PROJECT.md). Do not implement until single-request serving is proven.

---

## Technical Debt Patterns

Shortcuts that seem reasonable but create long-term problems.

| Shortcut | Immediate Benefit | Long-term Cost | When Acceptable |
|----------|-------------------|----------------|-----------------|
| Hardcoded model input shapes | Fast initial implementation | Every new model requires code changes | Never -- read shapes from ONNX graph |
| String-based tensor type handling | Quick prototype | Runtime type errors, no compile-time safety | Never in Rust -- use enums |
| Downloading models at startup without S3 cache | Simpler deployment | Every pod restart re-downloads, HF rate limits | Only for local development |
| Skipping model warmup | Faster startup | First N requests have 10-100x latency | Never for production |
| Using `ort` download strategy in production | Easy initial setup | Uncontrolled network dependency at build/runtime | Only for CI/testing |
| Thread count = num_cpus::get() | Correct on bare metal | Wrong in containers (sees host cores) | Never in containers |

## Integration Gotchas

Common mistakes when connecting to external services.

| Integration | Common Mistake | Correct Approach |
|-------------|----------------|------------------|
| S3 model download | Using single GetObject for multi-GB files | Use multipart/ranged download with per-part retry. Failed parts retry independently. |
| HuggingFace Hub | Anonymous downloads in production | Always authenticate. Token raises rate limit 16x. |
| HuggingFace Hub | Whole-file retry on failure | Implement chunk-level resumption. Large models can take minutes to re-download. |
| Forge (Python conversion service) | Synchronous blocking call from Rust | Use async with timeout. Conversion can take minutes for large models. Set a circuit breaker. |
| Forge | No validation of converted ONNX output | Validate ONNX graph after Forge conversion: check opset version, input/output names and shapes, run test inference. |
| gRPC (tonic) | No application-level backpressure | Use bounded mpsc channels for request queuing. HTTP/2 flow control alone is not sufficient. |
| Prometheus metrics | Only exporting request count/latency | Must also export: GPU memory usage, model load status, batch queue depth, inference duration by model type. |

## Performance Traps

Patterns that work at small scale but fail as usage grows.

| Trap | Symptoms | Prevention | When It Breaks |
|------|----------|------------|----------------|
| Creating new ORT session per request | Works for 1 req/s, OOM at 100 req/s | Create session once at startup, share via Arc | Immediately at any real load |
| Arena allocator growing without bound | Stable for hours, OOM after days | Set memory limits, monitor RSS, use pod memory limits as safety net | After processing diverse input sizes over time |
| Synchronous model loading blocking tokio runtime | Works with 1 model | Use `spawn_blocking` or dedicated thread for model load | When model loading takes > 100ms |
| Allocating new input tensors per request | Fine for low traffic | Pre-allocate tensor buffers, reuse across requests | At 100+ req/s, GC pressure noticeable |
| Logging every request at DEBUG level | Useful during development | Use structured logging with configurable levels, sample at high throughput | At 1000+ req/s, I/O becomes bottleneck |

## Security Mistakes

Domain-specific security issues beyond general web security.

| Mistake | Risk | Prevention |
|---------|------|------------|
| Loading ONNX models from untrusted sources without validation | Malicious ONNX graphs can exploit runtime vulnerabilities | Only load models from S3 cache (controlled) or validated HF repos. Validate ONNX graph structure before loading. |
| Exposing model files via HTTP endpoint | Model intellectual property leakage | Serve inference only, never expose raw model files. S3 bucket must not be public. |
| HuggingFace token in environment variable without k8s Secret | Token leakage via pod spec, logs, or kubectl describe | Use k8s Secrets mounted as files, not env vars. Rotate tokens. |
| No input size limits on inference requests | Denial of service via oversized inputs | Enforce max sequence length, max batch size, max request payload size at the gRPC/HTTP layer. |
| Running inference container as root | Container escape risk amplified by GPU driver access | Use non-root user in Dockerfile. GPU device access via k8s device plugin, not privileged mode. |

## "Looks Done But Isn't" Checklist

Things that appear complete but are missing critical pieces.

- [ ] **Model loading:** Often missing warmup inference -- verify first real request latency matches steady-state
- [ ] **GPU inference:** Often missing EP availability check -- verify logs confirm GPU provider is actually active
- [ ] **Tokenization:** Often missing attention_mask and token_type_ids -- verify all model-expected inputs are produced
- [ ] **Health endpoint:** Often missing model-loaded check -- verify readiness returns 503 during loading, 200 after
- [ ] **gRPC service:** Often missing graceful shutdown -- verify in-flight requests complete before pod termination
- [ ] **S3 download:** Often missing integrity check -- verify downloaded model hash matches expected (ETag or SHA256)
- [ ] **Error handling:** Often missing per-request error isolation -- verify one bad request does not crash the session
- [ ] **Metrics:** Often missing GPU metrics -- verify VRAM usage, GPU utilization are exported, not just CPU metrics
- [ ] **Container image:** Often missing CUDA library verification -- verify `ldconfig -p | grep cuda` shows expected versions at build time

## Recovery Strategies

When pitfalls occur despite prevention, how to recover.

| Pitfall | Recovery Cost | Recovery Steps |
|---------|---------------|----------------|
| Silent CPU fallback | LOW | Add EP check, redeploy. No data loss, just slower. |
| CUDA version mismatch | MEDIUM | Pin versions in Dockerfile, rebuild image, redeploy. May require base image change. |
| GPU memory leak | LOW | Restart pod (one-model-per-pod makes this safe). Add memory monitoring to prevent recurrence. |
| Cold start probe kills | LOW | Increase startup probe timeout, add pre-optimized model caching. |
| Thread oversubscription | LOW | Set explicit thread count from cgroup, redeploy. |
| Tokenizer/model mismatch | MEDIUM | Requires investigation of export config vs. tokenizer config. Fix either the export or the tokenizer setup, re-test against Python reference. |
| HF rate limiting | LOW | Populate S3 cache manually, add token auth. Transient issue. |
| Dynamic batching latency | MEDIUM | Remove batching code, revert to single-request. May require API contract changes if clients expect batch endpoints. |

## Pitfall-to-Phase Mapping

How roadmap phases should address these pitfalls.

| Pitfall | Prevention Phase | Verification |
|---------|------------------|--------------|
| Silent CPU fallback | Phase 1: Core session management | Startup logs confirm EP name; integration test asserts GPU EP active |
| CUDA version mismatch | Phase 1-2: Docker/infrastructure setup | CI builds container and runs `nvidia-smi` + test inference on GPU runner |
| GPU memory leak | Phase 3-4: Production hardening | Prometheus alert on VRAM > 80%; soak test runs 24h with monitoring |
| Cold start probe kills | Phase 2: k8s deployment | Deployment tested with `kubectl rollout` watching pod transitions |
| Thread oversubscription | Phase 2-3: Performance tuning | Benchmark with thread count = cgroup CPUs vs. default; measure latency |
| Tokenizer/model mismatch | Phase 2: Pre/post-processing pipeline | Integration test: tokenize -> infer -> compare against Python reference output |
| HF rate limiting | Phase 1-2: Model resolution | Integration test: S3 cache hit path; manual test: HF fallback with token |
| Dynamic batching latency | Phase 4+: Optional batching | Only implement after profiling shows GPU utilization < 50% on single requests |
| Forge conversion errors | Phase 2-3: Forge integration | Validate ONNX output: opset version, input/output shapes, test inference |

## Sources

- [ort crate documentation](https://ort.pyke.io/) - Execution provider fallback behavior, session management, v2 migration
- [ort v2 migration guide](https://ort.pyke.io/migrating/v2) - Breaking API changes from v1 to v2
- [ONNX Runtime memory consumption docs](https://onnxruntime.ai/docs/performance/tune-performance/memory.html) - Arena allocation configuration
- [ONNX Runtime threading docs](https://onnxruntime.ai/docs/performance/tune-performance/threading.html) - Thread pool configuration and NUMA considerations
- [ONNX Runtime model optimizations](https://onnxruntime.ai/docs/performance/model-optimizations/) - Graph optimization and pre-optimization strategies
- [microsoft/onnxruntime #26831](https://github.com/microsoft/onnxruntime/issues/26831) - Memory not released by ReleaseSession
- [microsoft/onnxruntime #24376](https://github.com/microsoft/onnxruntime/issues/24376) - GPU memory leak with specific batch size sequences
- [microsoft/onnxruntime #25996](https://github.com/microsoft/onnxruntime/issues/25996) - Releasing GPU memory without unloading session
- [microsoft/onnxruntime #11801](https://github.com/microsoft/onnxruntime/issues/11801) - Clearing GPU memory without destroying session
- [HuggingFace Hub rate limits](https://huggingface.co/docs/hub/en/rate-limits) - API rate limit documentation
- [Kubernetes probes documentation](https://kubernetes.io/docs/tasks/configure-pod-container/configure-liveness-readiness-startup-probes/) - Startup and readiness probe configuration
- [GPU scheduling in Kubernetes](https://p4blo.dev/blog/kubernetes-gpu-scheduling/) - GPU scheduling lessons from ML workloads
- [Dynamic batching guide (Redis)](https://redis.io/blog/dynamic-batching-guide/) - Batching tradeoffs and implementation patterns
- [Continuous vs dynamic batching (Baseten)](https://www.baseten.co/blog/continuous-vs-dynamic-batching-for-ai-inference/) - Latency vs throughput tradeoffs
- [HuggingFace padding and truncation](https://huggingface.co/docs/transformers/v4.25.1/en/pad_truncation) - Tokenizer configuration for model input compatibility
- [Optimum ONNX export guide](https://huggingface.co/docs/optimum/exporters/onnx/usage_guides/export_a_model) - ONNX export with dynamic axes and opset configuration
- [triton-inference-server/server #8083](https://github.com/triton-inference-server/server/issues/8083) - GPU VRAM leak with ORT backend

---
*Pitfalls research for: Hephaestus -- ONNX Model Inference Runtime in Rust*
*Researched: 2026-08-22*
