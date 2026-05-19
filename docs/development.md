# Development Guide

This guide covers local development workflow for the TorEx backend and frontend.

---

## Prerequisites

```bash
# Rust (latest stable)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup target add x86_64-unknown-linux-musl
rustup component add clippy rustfmt

# Flutter
sudo snap install flutter --classic   # Linux
# Or use flutter.dev/docs/get-started for other OS

# wasm-pack (for Flutter web crypto bridge)
cargo install wasm-pack

# sqlx-cli (for schema migrations and query preparation)
cargo install sqlx-cli --no-default-features --features postgres

# Docker Compose (for running dependencies)
curl -fsSL https://get.docker.com | sh
```

---

## Running the Backend Locally

### 1. Start only the dependencies

```bash
# Start Postgres, Redis, Kafka, Vault only
docker compose up -d postgres redis kafka vault
```

### 2. Set environment variables

```bash
cp .env.example .env
# Edit .env — minimum required for local dev:
export POSTGRES_URL=postgres://postgres:$POSTGRES_PASSWORD@localhost:5432/torex
export REDIS_URL=redis://:$REDIS_PASSWORD@localhost:6379
export KAFKA_BROKER=localhost:9092
export VAULT_ADDR=http://localhost:8200
export VAULT_TOKEN=root
export SECRET_NOISE_STATIC_KEY=$(openssl rand -hex 64)
```

### 3. Run the backend

```bash
cd backend
SQLX_OFFLINE=true cargo run --bin torex
```

The server starts on `http://localhost:3000`.

`SQLX_OFFLINE=true` skips live database query verification at compile time (uses `.sqlx/` cached metadata instead). This is the same mode used in Docker builds.

---

## Running Tests

```bash
cd backend

# Unit tests (no database needed)
cargo test --lib

# Specific crate tests
cargo test --lib -p matching
cargo test --lib -p crypto_primitives
cargo test --lib -p fees

# Specific test
cargo test --lib -p matching test_limit_buy_matches_limit_sell

# With output (useful for debugging)
cargo test --lib -- --nocapture
```

Current test count: **24 tests** (10 matching, 8 crypto_primitives, 6 fees). All should pass with no external dependencies.

---

## Type-Checking

```bash
cd backend
cargo check          # fast type check, no binary output
cargo clippy         # lints
cargo fmt --check    # formatting check
```

---

## Adding sqlx Queries

When you add a new `sqlx::query!` or `sqlx::query_as!` macro, the compile-time verifier needs to talk to a live database (unless in offline mode).

```bash
# Start the database
docker compose up -d postgres

# Set DATABASE_URL
export DATABASE_URL=postgres://postgres:$POSTGRES_PASSWORD@localhost:5432/torex

# Run schema
psql $DATABASE_URL < infra/postgres/init.sql

# Regenerate offline metadata
cd backend
cargo sqlx prepare --workspace

# Commit the updated .sqlx/ directory
git add .sqlx/
git commit -m "chore: update sqlx offline metadata"
```

After `cargo sqlx prepare`, you can unset `DATABASE_URL` and build with `SQLX_OFFLINE=true` again.

---

## Flutter Development

### Running on web (hot reload)

```bash
cd frontend
flutter pub get
flutter run -d chrome --web-renderer canvaskit
```

### Running on mobile (Android/iOS)

```bash
# Android
flutter run -d android

# iOS
flutter run -d ios
```

### Widget tests

```bash
cd frontend
flutter test
```

---

## Building the Crypto WASM Bridge

The `crypto_primitives` crate exports WASM bindings via `wasm-bindgen`. For production Flutter web, you need to build and integrate these bindings.

```bash
cd backend/crates/crypto_primitives

# Build for web
wasm-pack build --target web --out-dir ../../../../frontend/web/pkg/

# This produces:
#   frontend/web/pkg/crypto_primitives.js
#   frontend/web/pkg/crypto_primitives_bg.wasm
#   frontend/web/pkg/crypto_primitives.d.ts
```

Update `frontend/web/index.html` to load the WASM module:

```html
<script type="module">
  import init from './pkg/crypto_primitives.js';
  await init();
</script>
```

Then update `lib/core/crypto/wasm_bridge.dart` to call the exported functions instead of returning stub values.

> **Note:** The current `wasm_bridge.dart` returns zero-byte proof stubs. The exchange will function without real ZK proofs, but withdrawal proof verification will accept all requests. For production, the WASM build is required.

---

## Adding a New Trading Pair

No code changes needed. The order book creates a new `OrderBook` per pair dynamically. Just submit an order with any pair string (e.g., `"SOL/USDT"`) and the book is created automatically.

To add a new chain for deposits/withdrawals:

1. Insert into `chain_config` table
2. Add polling logic in `blockchain/src/lib.rs`
3. Add node URL to `.env` and `docker-compose.yml`

---

## Adding a New Crate

```bash
cd backend

# Create the crate
cargo new --lib crates/my_crate

# Add to workspace Cargo.toml
# [members] section: add "crates/my_crate"

# Add shared as dependency
# In crates/my_crate/Cargo.toml:
# [dependencies]
# shared = { path = "../shared" }
```

Register the crate's router in `crates/app/src/main.rs`:

```rust
use my_crate;

// In main():
my_crate::init_state(pool.clone(), ...);

// In app builder:
let app = Router::new()
    // ... existing routers ...
    .merge(my_crate::router());
```

---

## Project Structure

```
TorEx/
├── backend/
│   ├── Cargo.toml              # Workspace manifest
│   ├── Dockerfile              # musl builder → FROM scratch
│   └── crates/
│       ├── app/                # Binary entry point (main.rs)
│       ├── shared/             # Common types, DB/Redis/Kafka wrappers
│       ├── crypto_primitives/  # Noise, Signal, ZK, stealth addresses
│       ├── auth/               # User + admin authentication
│       ├── wallet/             # Balances, deposits, withdrawals
│       ├── matching/           # Order book engine
│       ├── settlement/         # Kafka trade consumer
│       ├── fees/               # Fee tier engine
│       ├── blockchain/         # ETH + Tron monitors
│       ├── ws_gateway/         # WebSocket server
│       └── admin/              # Admin API
├── frontend/
│   ├── pubspec.yaml
│   ├── lib/
│   │   ├── main.dart
│   │   ├── app.dart            # AuthGate, GoRouter
│   │   ├── core/
│   │   │   ├── crypto/         # Key derivation, WASM bridge
│   │   │   ├── network/        # HTTP + WebSocket clients
│   │   │   └── storage/        # Secure storage
│   │   ├── features/
│   │   │   ├── onboarding/     # Mnemonic generate/import
│   │   │   ├── trading/        # Order book, chart, order entry
│   │   │   ├── wallet/         # Deposit, withdraw, balance
│   │   │   ├── settings/       # Theme, backup, clear
│   │   │   └── admin/          # Admin login + dashboard
│   │   └── shared/
│   │       ├── theme/          # Dark theme, colors, typography
│   │       └── widgets/        # MonoText, shared components
│   └── web/
│       ├── index.html
│       └── pkg/                # wasm-pack output (gitignored)
├── infra/
│   ├── postgres/init.sql       # Full production schema
│   ├── kafka/topics.sh         # Topic creation script
│   ├── vault/config.hcl        # Vault server config
│   └── tor/                    # Dockerfile + entrypoint + torrc
├── nginx/
│   ├── nginx.conf
│   └── Dockerfile
├── monitoring/
│   ├── prometheus/
│   ├── loki/
│   └── grafana/
├── docs/
│   ├── api.md                  # REST + WebSocket API reference
│   ├── architecture.md         # System design
│   ├── deployment.md           # Production deployment
│   └── development.md          # This file
├── docker-compose.yml
├── .env.example
├── README.md
└── DECISIONS.md                # Architecture Decision Records
```

---

## Logging

The backend uses `tracing` with structured JSON output. Log levels are set via `RUST_LOG`:

```bash
# Default
RUST_LOG=info cargo run --bin torex

# Verbose matching engine
RUST_LOG=torex_matching=debug,info cargo run --bin torex

# Trace everything
RUST_LOG=trace cargo run --bin torex
```

**Never log:** `user_id`, `pubkey`, IP addresses, session tokens, balance values, order prices (in production).

The tracing subscriber is configured in `app/src/main.rs`:
```rust
tracing_subscriber::fmt()
    .with_env_filter(EnvFilter::from_default_env())
    .json()
    .init();
```

---

## Environment Variables

See `.env.example` for all variables. Key ones for development:

| Variable | Dev default | Purpose |
|---|---|---|
| `POSTGRES_PASSWORD` | `postgres` | Postgres superuser password |
| `REDIS_PASSWORD` | (empty) | Redis password |
| `VAULT_TOKEN` | `root` | Vault dev root token |
| `ETH_NODE_URL` | — | wss:// Ethereum node |
| `TRON_NODE_URL` | `https://api.trongrid.io` | Tron REST API |
| `KAFKA_BROKER` | `localhost:9092` | Kafka broker address |
| `SECRET_NOISE_STATIC_KEY` | — | 64-byte hex Noise static keypair |
| `HCAPTCHA_SECRET` | — | hCaptcha secret (admin login) |
| `HCAPTCHA_SITE_KEY` | — | hCaptcha site key (Flutter) |

---

## Common Issues

### `cargo check` fails with sqlx error

Ensure `SQLX_OFFLINE=true` is set:

```bash
SQLX_OFFLINE=true cargo check
```

Or run `cargo sqlx prepare` with a live database.

### Tor takes too long

On first start, mkp224o generates a `torex*.onion` address using 4 threads. This takes 5–30 minutes depending on CPU. This is expected behaviour. Subsequent restarts reuse the key from the `tor_data` volume.

To skip Tor during development:

```bash
docker compose up -d --scale tor=0
```

### Kafka topic not found

If you get `TopicNotFound` errors, run the topic creation script:

```bash
docker compose exec kafka /bin/sh /etc/kafka/topics.sh
```

### Balance decryption fails in dev

The dev fallback uses a zeroed AES key. If you've stored balances with a real Vault key and then switch to dev mode (or vice versa), decryption will fail. Wipe and reseed the database:

```bash
docker compose exec postgres psql -U postgres torex -c "DELETE FROM balances;"
```
