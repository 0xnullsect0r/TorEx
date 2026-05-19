# syntax=docker/dockerfile:1

FROM lukemathwalker/cargo-chef:latest-rust-1-alpine AS chef
# dl-cdn.alpinelinux.org can be unreliable. Try non-CDN mirrors in order.
# Extract Alpine version from existing repos file so we don't hardcode it.
# BuildKit cache mount persists the APKINDEX across builds once fetched.
RUN --mount=type=cache,target=/var/cache/apk \
    ALPINE_VER=$(grep -oE 'v[0-9]+\.[0-9]+' /etc/apk/repositories | head -1) \
    && for MIRROR in \
        "https://mirrors.edge.kernel.org/alpine" \
        "https://dl-4.alpinelinux.org/alpine" \
        "https://dl-cdn.alpinelinux.org/alpine"; \
    do \
        printf '%s/%s/main\n%s/%s/community\n' "$MIRROR" "$ALPINE_VER" "$MIRROR" "$ALPINE_VER" > /etc/apk/repositories \
        && apk update \
        && break \
        || echo "Mirror $MIRROR failed, trying next..."; \
    done \
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

# Runtime: alpine:3.21 with non-CDN mirror fallback + BuildKit cache.
FROM alpine:3.21 AS runtime
RUN --mount=type=cache,target=/var/cache/apk \
    for MIRROR in \
        "https://mirrors.edge.kernel.org/alpine" \
        "https://dl-4.alpinelinux.org/alpine" \
        "https://dl-cdn.alpinelinux.org/alpine"; \
    do \
        printf '%s/v3.21/main\n%s/v3.21/community\n' "$MIRROR" "$MIRROR" > /etc/apk/repositories \
        && apk update \
        && break \
        || echo "Mirror $MIRROR failed, trying next..."; \
    done \
    && apk add ca-certificates
COPY --from=builder /app/target/release/torex /torex
EXPOSE 8080
ENTRYPOINT ["/torex"]
