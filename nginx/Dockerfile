# Stage 1: Build Leptos/WASM frontend with trunk
FROM rust:1 AS builder

# Disable WebAssembly reference-types feature to avoid wasm-bindgen intrinsic
# lookup failures (clone_ref / externref_table_dealloc) with Rust 1.82+.
# Must be set before ANY cargo step so deps and app are compiled consistently.
ENV RUSTFLAGS="-C target-feature=-reference-types"

RUN rustup target add wasm32-unknown-unknown
RUN curl -fsSL \
    https://github.com/trunk-rs/trunk/releases/download/v0.21.7/trunk-x86_64-unknown-linux-gnu.tar.gz \
    | tar xz -C /usr/local/bin/

WORKDIR /app

# Cache dependencies (dummy build first) — include .cargo/config.toml so
# rustflags are identical between this step and the real trunk build.
COPY frontend/.cargo/ .cargo/
COPY frontend/Cargo.toml frontend/Cargo.lock* ./
RUN mkdir src && echo 'fn main(){}' > src/main.rs \
    && cargo build --release --target wasm32-unknown-unknown 2>/dev/null || true \
    && rm -rf src

# Build the actual app
COPY frontend/ .
RUN trunk build --release

# Stage 2: Serve with nginx
FROM nginx:1.27-alpine
COPY nginx/nginx.conf /etc/nginx/nginx.conf
COPY --from=builder /app/dist /usr/share/nginx/html
