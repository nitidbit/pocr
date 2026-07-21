# syntax=docker/dockerfile:1

# --- Base stage: Rust toolchain + cargo-chef + protobuf compiler ---
FROM rust:1.95@sha256:f49565f188ee00bc2a18dd418183f2c5f23ef7d6e691890517ed341a598f67c3 AS chef
RUN cargo install cargo-chef
RUN apt-get update && apt-get install -y --no-install-recommends \
    protobuf-compiler && \
    rm -rf /var/lib/apt/lists/*
WORKDIR /app

# --- Development target (used by compose.dev.yaml) ---
FROM chef AS development
RUN cargo install cargo-watch
# Source code is mounted via volume; no COPY needed
CMD ["cargo", "watch", "-x", "run -p pocr-engine"]

# --- Planner: generate dependency recipe ---
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# --- Builder: compile dependencies then application ---
FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    cargo chef cook --release --recipe-path recipe.json
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    cargo build --release --locked

# --- Production: minimal runtime image ---
FROM debian:bookworm-slim@sha256:7b140f374b289a7c2befc338f42ebe6441b7ea838a042bbd5acbfca6ec875818 AS production

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    wget && \
    rm -rf /var/lib/apt/lists/*

RUN groupadd --system --gid 1001 pocr && \
    useradd --system --uid 1001 --gid pocr --no-create-home pocr

COPY --from=builder --chown=1001:1001 /app/target/release/pocr-engine /usr/local/bin/pocr-engine

USER 1001

ENV HOST=0.0.0.0
ENV API_PORT=3000
ENV GRPC_PORT=50051

EXPOSE 3000 50051

ENTRYPOINT ["pocr-engine"]
