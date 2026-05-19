# TorEx

A production-grade, privacy-first USDT cryptocurrency exchange. Runs entirely inside Docker Compose, accessible only over Tor via a vanity `.onion` address (`torex*.onion`). No accounts. No email. No KYC. Identity is a BIP-39 mnemonic.

```
┌─────────────────────────────────────────────┐
│              Tor Hidden Service              │
│          torex[...].onion  :80               │
└──────────────────┬──────────────────────────┘
                   │
             ┌─────▼──────┐
             │   nginx    │  ← only public port
             └─────┬──────┘
        ┌──────────┼──────────┐
   ┌────▼───┐  ┌───▼───┐  ┌──▼──┐
   │backend │  │ kafka │  │redis│
   └────┬───┘  └───────┘  └─────┘
        │
   ┌────▼────┐  ┌───────┐  ┌───────┐
   │postgres │  │ vault │  │  tor  │
   └─────────┘  └───────┘  └───────┘
```

## Features

- **~200 USDT trading pairs** — BTC, ETH, SOL, BNB, XRP, and 195 more
- **Privacy by design** — Noise_XX transport encryption, Signal Double Ratchet on WebSocket, DLEQ stealth deposit addresses, ZK balance proofs
- **No JWT** — Noise session handshake *is* the credential; no tokens to steal
- **Maker/taker fee tiers** — 0.16%/0.26% down to 0%/0.10% based on 30-day rolling volume (Kraken/Bitstamp style), fully admin-configurable
- **Multi-chain USDT** — ERC-20, TRC-20, BEP-20, Polygon, Avalanche, Arbitrum, Optimism, Solana
- **Flutter frontend** — Web (WASM), Android, iOS from one codebase
- **Single binary** — 10 Rust crates compiled into one ~10 MB static musl binary

## Quick Start

```bash
# 1. Clone
git clone https://github.com/0xnullsect0r/TorEx && cd TorEx

# 2. Configure
cp .env.example .env
# Edit .env — fill in POSTGRES_PASSWORD, REDIS_PASSWORD, ETH_NODE_URL, etc.

# 3. Build Flutter web (needed before first `up`)
cd frontend && flutter pub get && flutter build web --release && cd ..

# 4. Start
docker compose up --build
```

The Tor vanity key generation runs on first boot and takes **5–30 minutes**. The exchange is accessible at `http://localhost` during this time; the `.onion` address will be printed to the `tor` container logs once ready:

```bash
docker compose logs -f tor
# ...
# [*] Found: torex7abc123xyz.onion
```

> **Default admin credentials:** `admin` / `adminpassword`  
> Change these immediately via the admin panel after first boot.

## Repository Layout

```
TorEx/
├── backend/                  # Rust workspace (single musl binary)
│   ├── Cargo.toml            # Workspace manifest
│   ├── Dockerfile            # Multi-stage musl build → FROM scratch
│   └── crates/
│       ├── app/              # Entry point — merges all routers, starts tasks
│       ├── shared/           # DB pool, Redis, Kafka, newtypes, AppConfig
│       ├── crypto_primitives/# Noise XX, Signal ratchet, Groth16 ZK, stealth
│       ├── auth/             # Registration, Noise session, TOTP, Passkeys
│       ├── wallet/           # Stealth deposit, ZK withdrawal, encrypted balance
│       ├── matching/         # In-memory BTreeMap order book, REST endpoints
│       ├── settlement/       # Kafka consumer → atomic balance debit/credit
│       ├── fees/             # Tiered fee engine, rolling volume tracking
│       ├── blockchain/       # Chain monitors, withdrawal executor
│       ├── ws_gateway/       # Axum WebSocket with Noise+Signal
│       └── admin/            # Stats, fee tier CRUD, withdrawal approval
│
├── frontend/                 # Flutter application
│   ├── lib/
│   │   ├── app.dart          # Root app + AuthGate router
│   │   ├── core/             # crypto/, network/, storage/
│   │   ├── features/         # onboarding/, trading/, wallet/, settings/, admin/
│   │   └── shared/           # theme/, widgets/
│   ├── test/                 # Widget + unit tests
│   ├── web/                  # index.html, manifest.json
│   ├── pubspec.yaml
│   └── Dockerfile
│
├── infra/
│   ├── postgres/init.sql     # Production schema (RLS, roles, chain_config)
│   ├── kafka/topics.sh       # Creates trades/withdrawals/settlement_complete
│   ├── tor/                  # Dockerfile (mkp224o), entrypoint.sh, torrc
│   └── vault/config.hcl     # Vault dev-mode config
│
├── nginx/
│   ├── nginx.conf            # Reverse proxy, rate limiting, security headers
│   └── Dockerfile
│
├── monitoring/
│   ├── prometheus/           # Scrape config
│   ├── grafana/              # Datasource provisioning
│   └── loki/                 # Log aggregation config
│
├── docker-compose.yml        # 10 services, internal bridge network
├── .env.example              # Environment variable template
├── DECISIONS.md              # Architecture decision records
└── docs/
    ├── api.md                # Full REST + WebSocket API reference
    ├── architecture.md       # System design deep-dive
    ├── deployment.md         # Production deployment guide
    └── development.md        # Local development guide
```

## Environment Variables

| Variable | Required | Description |
|---|---|---|
| `POSTGRES_PASSWORD` | ✅ | Postgres superuser password |
| `REDIS_PASSWORD` | ✅ | Redis `requirepass` value |
| `VAULT_TOKEN` | ✅ | Vault dev root token (default: `root`) |
| `ETH_NODE_URL` | ✅ | `wss://` Ethereum node (Infura, Alchemy, etc.) |
| `TRON_NODE_URL` | ✅ | Tron node (`https://api.trongrid.io` works) |
| `SECRET_NOISE_STATIC_KEY` | ✅ | 64-byte hex static keypair for Noise XX |
| `HCAPTCHA_SECRET` | ✅ | hCaptcha secret key (admin login) |
| `HCAPTCHA_SITE_KEY` | ✅ | hCaptcha site key (Flutter admin UI) |
| `KAFKA_BROKER` | — | Default: `kafka:9092` |

## Fee Schedule

| 30-day Volume | Maker | Taker |
|---|---|---|
| < $10,000 | 0.16% | 0.26% |
| $10k – $50k | 0.14% | 0.24% |
| $50k – $100k | 0.12% | 0.22% |
| $100k – $250k | 0.10% | 0.20% |
| $250k – $1M | 0.08% | 0.18% |
| $1M – $10M | 0.06% | 0.16% |
| > $10M | 0.00% | 0.10% |

Fees are admin-configurable in real time via `/admin/api/fee-tiers`.

## Build & Test

```bash
# Rust backend
cd backend
cargo check                         # Verify compilation
cargo test --lib                    # Run 24 unit tests (no DB required)
cargo test --lib -p matching        # Run only order book tests

# Flutter frontend
cd frontend
flutter pub get
flutter test                        # Run widget + unit tests
flutter build web --release         # Build for Docker

# Infrastructure
docker compose config               # Validate compose file
```

## Documentation

| Doc | Contents |
|---|---|
| [docs/architecture.md](docs/architecture.md) | System design, data flow, crypto stack |
| [docs/api.md](docs/api.md) | Full REST and WebSocket API reference |
| [docs/deployment.md](docs/deployment.md) | Production hardening, TLS, backups |
| [docs/development.md](docs/development.md) | Local dev setup, adding pairs, testing |
| [DECISIONS.md](DECISIONS.md) | Architecture decision records |

## Security Model

- **No IP logging** — nginx strips `X-Forwarded-For` and `X-Real-IP`; access log is off
- **No PII in logs** — structured logs contain only `event` enum, `order_id`, `trade_id`
- **Encrypted balances** — `enc_balance` column is AES-256-GCM; plaintext never written to disk
- **Vault-held keys** — balance encryption key stored in HashiCorp Vault, never in env vars
- **Stealth addresses** — each deposit uses a fresh DLEQ stealth address; on-chain activity is unlinkable to exchange accounts
- **Session = Noise handshake** — no JWT, no cookies, no bearer tokens that can be logged or replayed
- **Rate limiting by server address** — prevents Tor users (who share exit IPs) from being collectively rate-limited

## Tech Stack

| Layer | Technology |
|---|---|
| Backend language | Rust (Tokio + Axum 0.7) |
| Transport crypto | Noise Protocol XX (snow crate) |
| Session crypto | Signal Double Ratchet (x25519-dalek) |
| ZK proofs | Groth16 on BN254 (arkworks) |
| Stealth addresses | DLEQ (x25519-dalek) |
| Balance encryption | AES-256-GCM (aes-gcm crate) |
| Database | PostgreSQL 16 (sqlx 0.8) |
| Cache / sessions | Redis 7 |
| Message bus | Apache Kafka 3.7 (KRaft, no ZooKeeper) |
| Secrets | HashiCorp Vault 1.17 (dev mode) |
| Onion routing | Tor + mkp224o vanity address |
| Frontend | Flutter 3 (Dart, WASM renderer) |
| State management | flutter_riverpod 2.5 |
| Container runtime | Docker Compose |
| Reverse proxy | nginx |
| Monitoring | Prometheus + Grafana + Loki |
