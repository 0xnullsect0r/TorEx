# syntax=docker/dockerfile:1

FROM lukemathwalker/cargo-chef:latest-rust-1-alpine AS chef
# Split apk update into its own layer so Docker's layer cache preserves the APKINDEX.
# Subsequent builds reuse this cached layer without hitting the network.
RUN apk update
RUN apk add \
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

# Runtime: alpine:3.21 with split apk layers (same resilience pattern as build stage).
# apk update is its own cached layer — the APKINDEX is never re-fetched on rebuild.
FROM alpine:3.21 AS runtime
RUN apk update
RUN apk add ca-certificates
COPY --from=builder /app/target/release/torex /torex
EXPOSE 8080
ENTRYPOINT ["/torex"]
