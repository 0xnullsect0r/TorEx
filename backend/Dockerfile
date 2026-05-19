# syntax=docker/dockerfile:1
FROM rust:1.78-slim as chef
RUN cargo install cargo-chef --locked
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
WORKDIR /app

FROM chef as planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef as builder
COPY --from=planner /app/recipe.json recipe.json
RUN rustup target add x86_64-unknown-linux-musl
RUN apt-get update && apt-get install -y musl-tools && rm -rf /var/lib/apt/lists/*
RUN cargo chef cook --release --target x86_64-unknown-linux-musl --recipe-path recipe.json
COPY . .
ENV SQLX_OFFLINE=true
RUN cargo build --release --target x86_64-unknown-linux-musl --bin torex

FROM scratch as runtime
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/torex /torex
EXPOSE 8080
ENTRYPOINT ["/torex"]
