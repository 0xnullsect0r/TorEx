# ──────────────────────────────────────────────────────────────────────────────
# Stage 0: cargo-chef base
# rust:1 (Debian/glibc) — required because trunk is a glibc binary.
# wasm32-unknown-unknown exposes reference-types in CARGO_CFG_TARGET_FEATURE
# since Rust 1.84, so wasm-bindgen 0.2.121 sets wbg_reference_types correctly
# and __externref_table_alloc is compiled into WASM output.
# ──────────────────────────────────────────────────────────────────────────────
FROM rust:1 AS chef
RUN rustup target add wasm32-unknown-unknown
# Install cargo-chef for proper dep-caching (same pattern as the backend).
RUN cargo install cargo-chef --locked
WORKDIR /app

# ──────────────────────────────────────────────────────────────────────────────
# Stage 1: Generate dependency recipe
# ──────────────────────────────────────────────────────────────────────────────
FROM chef AS planner
COPY frontend/ .
RUN cargo chef prepare --recipe-path recipe.json

# ──────────────────────────────────────────────────────────────────────────────
# Stage 2: Cook deps then build the app
# ──────────────────────────────────────────────────────────────────────────────
FROM chef AS builder

# Pre-built trunk binary
RUN curl -fsSL \
    https://github.com/trunk-rs/trunk/releases/download/v0.21.14/trunk-x86_64-unknown-linux-gnu.tar.gz \
    | tar xz -C /usr/local/bin/

# Pre-built wasm-bindgen-cli 0.2.121 (must match wasm-bindgen crate in Cargo.lock).
RUN curl -fsSL \
    "https://github.com/wasm-bindgen/wasm-bindgen/releases/download/0.2.121/wasm-bindgen-0.2.121-x86_64-unknown-linux-musl.tar.gz" \
    | tar xz --strip-components=1 -C /usr/local/bin/ \
    "wasm-bindgen-0.2.121-x86_64-unknown-linux-musl/wasm-bindgen"

# Cook deps: compiles ALL dependencies for wasm32 but NOT torex-frontend itself.
# This layer is cached as long as Cargo.toml / Cargo.lock are unchanged.
# Because torex-frontend is never compiled here there are zero stale WASM artifacts.
COPY frontend/.cargo/ .cargo/
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --target wasm32-unknown-unknown --recipe-path recipe.json

# Build the app — only torex-frontend needs to compile; all deps come from above.
COPY frontend/ .
RUN trunk build --release

# ──────────────────────────────────────────────────────────────────────────────
# Stage 3: Serve with nginx
# ──────────────────────────────────────────────────────────────────────────────
FROM nginx:1.27-alpine
COPY nginx/nginx.conf /etc/nginx/nginx.conf
COPY --from=builder /app/dist /usr/share/nginx/html
