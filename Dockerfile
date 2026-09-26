# syntax=docker/dockerfile:1

# ============================================================
# HyperLight — multi-stage build
#   stage 1 (builder): Rust toolchain + trunk + tailwindcss →
#                      release server binary + WASM frontend
#   stage 2 (runtime): minimal Debian + binary + dist/
# ============================================================

FROM rust:1.83-slim-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
        pkg-config libssl-dev ca-certificates git curl unzip \
    && rm -rf /var/lib/apt/lists/*

RUN rustup target add wasm32-unknown-unknown
RUN cargo install trunk --version 0.21.14 --locked

WORKDIR /build

# ---- dependency cache layer (survives source edits) ----
COPY Cargo.toml Cargo.lock ./
COPY crates/core/Cargo.toml crates/core/Cargo.toml
COPY crates/server/Cargo.toml crates/server/Cargo.toml
COPY crates/ui/Cargo.toml crates/ui/Cargo.toml
COPY crates/ui/Trunk.toml crates/ui/Trunk.toml
RUN mkdir -p crates/core/src crates/server/src crates/ui/src && \
    echo "pub fn _x() {}" > crates/core/src/lib.rs && \
    echo "fn main() {}" > crates/server/src/main.rs && \
    echo "pub fn _x() {}" > crates/ui/src/lib.rs && \
    cargo build --release -p ob-server && \
    cargo build --release -p ob-ui --target wasm32-unknown-unknown || true

# ---- real sources ----
COPY crates ./crates
RUN touch crates/core/src/lib.rs crates/server/src/main.rs && \
    cargo build --release -p ob-server

# ---- frontend: tailwind css (standalone binary, no node needed) ----
RUN curl -fsSL https://github.com/tailwindlabs/tailwindcss/releases/download/v4.1.11/tailwindcss-x86_64-unknown-linux-gnu.zip \
        -o /tmp/tw.zip && \
    unzip -o /tmp/tw.zip -d /usr/local/bin && chmod +x /usr/local/bin/tailwindcss
WORKDIR /build/crates/ui
RUN tailwindcss -i style.css -o tailwind.out.css --minify

# ---- frontend: trunk WASM bundle (outputs to /build/dist per Trunk.toml) ----
ENV NO_COLOR=false
RUN trunk build --release

# ---------------- runtime ----------------
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -m -u 10001 hyper

WORKDIR /app
COPY --from=builder /build/target/release/hyperlight-server /app/hyperlight-server
COPY --from=builder /build/dist /app/dist

USER hyper
ENV PORT=3000
EXPOSE 3000

# Only network requirement: outbound WSS to 9 crypto exchanges.
ENTRYPOINT ["/app/hyperlight-server"]
