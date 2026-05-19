# syntax=docker/dockerfile:1

# Alpine-based cargo-chef: native musl target, no apt-get needed
FROM lukemathwalker/cargo-chef:latest-rust-1-alpine AS chef
RUN apk add --no-cache \
    pkgconfig \
    openssl-dev \
    openssl-libs-static \
    cmake \
    make \
    musl-dev \
    zlib-dev \
    zlib-static \
    curl-dev \
    build-base
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
ENV OPENSSL_STATIC=1
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
ENV SQLX_OFFLINE=true
RUN cargo build --release --bin torex

# Minimal Alpine runtime with CA certs for TLS
FROM alpine:3.19 AS runtime
RUN apk add --no-cache ca-certificates
COPY --from=builder /app/target/release/torex /torex
EXPOSE 8080
ENTRYPOINT ["/torex"]
