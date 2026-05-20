# Stage 1: Build Leptos/WASM frontend with trunk
# Use rust:1 (latest stable). wasm32-unknown-unknown exposes reference-types in
# CARGO_CFG_TARGET_FEATURE since Rust 1.84, so wasm-bindgen 0.2.121 sets wbg_reference_types
# correctly and __externref_table_alloc is included in the WASM output.
FROM rust:1 AS builder

RUN rustup target add wasm32-unknown-unknown
RUN curl -fsSL \
    https://github.com/trunk-rs/trunk/releases/download/v0.21.14/trunk-x86_64-unknown-linux-gnu.tar.gz \
    | tar xz -C /usr/local/bin/

# Download the official pre-built wasm-bindgen-cli 0.2.121 binary (x86_64 musl).
# Using a pre-built binary avoids the entire MSRV chain from `cargo install`.
# Trunk finds it in PATH and uses it instead of downloading its own version.
RUN curl -fsSL \
    "https://github.com/wasm-bindgen/wasm-bindgen/releases/download/0.2.121/wasm-bindgen-0.2.121-x86_64-unknown-linux-musl.tar.gz" \
    | tar xz --strip-components=1 -C /usr/local/bin/ \
    "wasm-bindgen-0.2.121-x86_64-unknown-linux-musl/wasm-bindgen"

WORKDIR /app

# Cache dependencies (dummy build first)
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
