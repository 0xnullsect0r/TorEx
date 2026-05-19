# Stage 1: Build Leptos/WASM frontend with trunk
FROM rust:1 AS builder

RUN rustup target add wasm32-unknown-unknown
RUN curl -fsSL \
    https://github.com/trunk-rs/trunk/releases/download/v0.21.14/trunk-x86_64-unknown-linux-gnu.tar.gz \
    | tar xz -C /usr/local/bin/

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
