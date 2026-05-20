# Stage 1: Build Leptos/WASM frontend with trunk
# Pinned to rust:1.82 because wasm-bindgen 0.2.121 handles the reference-types
# intrinsics emitted by Rust 1.82. Newer Rust (1.83+) changed those intrinsics in
# a way that causes wasm-bindgen-cli 0.2.121 to fail with "failed to find intrinsics
# to enable clone_ref function". rust:1.82 keeps the whole toolchain consistent.
FROM rust:1.82 AS builder

RUN rustup target add wasm32-unknown-unknown
RUN curl -fsSL \
    https://github.com/trunk-rs/trunk/releases/download/v0.21.14/trunk-x86_64-unknown-linux-gnu.tar.gz \
    | tar xz -C /usr/local/bin/

# Install wasm-bindgen-cli exactly matching the crate version in Cargo.lock (0.2.121).
# Trunk will find this in PATH and use it instead of any bundled/downloaded version.
# With CLI and crate both at 0.2.121, reference-types are handled correctly and the
# -C target-feature=-reference-types flag is NOT needed (and was causing runtime crashes).
RUN cargo install wasm-bindgen-cli --version 0.2.121 --locked

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
