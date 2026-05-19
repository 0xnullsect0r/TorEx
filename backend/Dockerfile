# syntax=docker/dockerfile:1

FROM lukemathwalker/cargo-chef:latest-rust-1-alpine AS chef
# Alpine base image has no ca-certificates, so HTTPS to external repos fails
# inside Docker. Switch to HTTP (packages are still GPG-verified by apk).
# Extract the Alpine version already set in the image's repos file.
RUN --mount=type=cache,id=apk-backend-chef,target=/var/cache/apk \
    ALPINE_VER=$(grep -oE 'v[0-9]+\.[0-9]+' /etc/apk/repositories | head -1) \
    && printf 'http://dl-cdn.alpinelinux.org/alpine/%s/main\nhttp://dl-cdn.alpinelinux.org/alpine/%s/community\n' \
        "$ALPINE_VER" "$ALPINE_VER" > /etc/apk/repositories \
    && apk update \
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

# Runtime: alpine:3.21 — use HTTP + --no-cache for a single small package.
# Avoids BuildKit cache mount sharing conflicts with concurrent build stages.
FROM alpine:3.21 AS runtime
RUN printf 'http://dl-cdn.alpinelinux.org/alpine/v3.21/main\nhttp://dl-cdn.alpinelinux.org/alpine/v3.21/community\n' > /etc/apk/repositories \
    && apk add --no-cache ca-certificates
COPY --from=builder /app/target/release/torex /torex
EXPOSE 8080
ENTRYPOINT ["/torex"]
