# syntax=docker/dockerfile:1

# cargo-chef pre-installed; avoids `cargo install --locked` failures
FROM lukemathwalker/cargo-chef:latest-rust-1-slim-bookworm AS chef
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev musl-tools \
    && rm -rf /var/lib/apt/lists/*
RUN rustup target add x86_64-unknown-linux-musl
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --target x86_64-unknown-linux-musl --recipe-path recipe.json
COPY . .
ENV SQLX_OFFLINE=true
RUN cargo build --release --target x86_64-unknown-linux-musl --bin torex

FROM scratch AS runtime
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/torex /torex
EXPOSE 8080
ENTRYPOINT ["/torex"]
