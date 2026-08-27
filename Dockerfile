# ---------- builder ----------
FROM ubuntu:24.04 AS builder

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
       curl ca-certificates build-essential pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

WORKDIR /build

# ort prebuilt static lib needs libstdc++; use the system linker instead of rust-lld.
RUN mkdir -p .cargo \
    && printf '[build]\nrustflags = ["-C", "linker=g++", "-C", "link-arg=-lstdc++"]\n' > .cargo/config.toml

# Cache dependency builds: copy manifests first, build a dummy.
COPY Cargo.toml Cargo.lock ./
COPY crates/hephaestus/Cargo.toml crates/hephaestus/Cargo.toml
COPY crates/hephaestus-api/Cargo.toml crates/hephaestus-api/Cargo.toml
COPY crates/hephaestus-core/Cargo.toml crates/hephaestus-core/Cargo.toml
COPY crates/hephaestus-proto/Cargo.toml crates/hephaestus-proto/Cargo.toml
COPY crates/hephaestus-resolve/Cargo.toml crates/hephaestus-resolve/Cargo.toml
RUN mkdir -p crates/hephaestus/src crates/hephaestus-api/src crates/hephaestus-core/src \
             crates/hephaestus-proto/src crates/hephaestus-resolve/src \
    && echo "fn main() {}" > crates/hephaestus/src/main.rs \
    && echo "" > crates/hephaestus-api/src/lib.rs \
    && echo "" > crates/hephaestus-core/src/lib.rs \
    && echo "" > crates/hephaestus-proto/src/lib.rs \
    && echo "" > crates/hephaestus-resolve/src/lib.rs \
    && cargo build --release 2>/dev/null || true

# Copy real source and build.
COPY crates/ crates/
RUN touch crates/*/src/*.rs && cargo build --release --bin hephaestus

# Collect runtime dependencies into a staging directory.
RUN mkdir -p /runtime/lib /runtime/etc/ssl/certs /runtime/tmp /runtime/cache \
    && cp /build/target/release/hephaestus /runtime/ \
    && ldd /build/target/release/hephaestus | awk '/=>/ {print $3}' | xargs -I{} cp {} /runtime/lib/ \
    && cp $(ldd /build/target/release/hephaestus | awk '/ld-linux/ {print $1}') /runtime/lib/ \
    && ln -sf $(basename $(ldd /build/target/release/hephaestus | awk '/ld-linux/ {print $1}')) /runtime/lib/ld.so \
    && cp /etc/ssl/certs/ca-certificates.crt /runtime/etc/ssl/certs/ \
    && chown 65534:65534 /runtime/cache

# ---------- runtime ----------
FROM scratch

COPY --from=builder /runtime/ /

ENV SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt
ENV HOME=/cache
ENV PORT=8080
EXPOSE 8080

USER 65534

ENTRYPOINT ["/lib/ld.so", "/hephaestus"]
