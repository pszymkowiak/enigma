FROM rust:1.85 AS builder

WORKDIR /build

# Copy workspace files
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/

# Build the enigma-proxy binary
RUN cargo build --release -p enigma-proxy

# Runtime image
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/enigma-proxy /usr/local/bin/enigma-proxy

RUN mkdir -p /data

EXPOSE 8333 9000

ENTRYPOINT ["enigma-proxy"]
CMD ["--config", "/etc/enigma/config.toml"]
