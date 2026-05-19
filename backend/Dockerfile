# syntax=docker/dockerfile:1

FROM lukemathwalker/cargo-chef:latest-rust-1-alpine AS chef
# BuildKit cache mount keeps APKINDEX persistent across builds.
# Retry loop handles transient CDN failures on first fetch.
RUN --mount=type=cache,target=/var/cache/apk \
    for i in 1 2 3 4 5; do apk update && break || (echo "Retry $i..." && sleep $((i*5))); done \
    && apk add \
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

# Runtime: alpine:3.21 with BuildKit cache mount + retry for CDN resilience.
FROM alpine:3.21 AS runtime
RUN --mount=type=cache,target=/var/cache/apk \
    for i in 1 2 3 4 5; do apk update && break || (echo "Retry $i..." && sleep $((i*5))); done \
    && apk add ca-certificates
COPY --from=builder /app/target/release/torex /torex
EXPOSE 8080
ENTRYPOINT ["/torex"]
