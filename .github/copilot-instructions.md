# TorEx — Copilot Instructions

## Project Overview

TorEx is a privacy-first USDT spot exchange that runs **exclusively as a Tor v3 hidden service**. Users access it only through Tor Browser at the `.onion` address. There is **no clearnet exposure**. All traffic flows: `Tor Browser → tor daemon → nginx → backend`.

**There is no public-facing REST API.** nginx only serves static WASM files and proxies the `/ws` WebSocket endpoint. All frontend↔backend communication must go through the Noise XX + Signal ratchet encrypted WebSocket.

## Build & Test Commands

```bash
# Check Rust backend compiles
cd backend && cargo check

# Run Rust unit tests (no DB needed)
cd backend && cargo test --lib

# Run specific test
cd backend && cargo test --lib -p matching test_limit_buy_matches_limit_sell

# Check frontend WASM compiles
cd frontend && cargo check --target wasm32-unknown-unknown

# Validate Docker Compose config
docker compose config

# Start everything
cp .env.example .env  # fill in your node URLs
docker compose up --build
```

## Architecture

**Single Docker Compose deployment** — all services use `network_mode: host`. nginx on port 80 is the only port reachable from Tor. The OS firewall prevents any direct clearnet access.

```
Tor Browser
    │  (Tor circuit)
    ▼
tor daemon (:9050) ── HiddenServicePort 80 ──► nginx (:80)
                                                    │
                                          ┌─────────┴──────────┐
                                          │                      │
                                     static WASM            /ws WebSocket
                                     (Leptos SPA)           │
                                                    backend (:8080)
                                                         │
                                              ┌──────────┼──────────┐
                                           postgres   redis      kafka
                                           (:5432)   (:6379)   (:9092)
                                              │
                                           vault (:8200)
```

**Rust backend** — Tokio + Axum 0.7 workspace of 9 crates compiled into one static musl binary (`torex`):
- `shared` — PgPool, Redis, Kafka wrappers, newtypes (UserId, Amount), AppConfig
- `crypto_primitives` — Noise XX (snow), Signal Double Ratchet (x25519-dalek + AES-GCM), Groth16 ZK (arkworks), DLEQ stealth addresses, balance AES-GCM encryption
- `auth` — Public-key-only auth (ed25519 signatures), Noise session = credential (no JWT), admin login (bcrypt+TOTP), Redis sessions
- `wallet` — Internal USDT ledger, stealth deposit addresses, ZK-proof withdrawal, encrypted balance blob
- `matching` — In-memory BTreeMap order book (FIFO price-time priority), all 11 order types
- `settlement` — Kafka consumer on `trades`, atomic Postgres debit/credit + fee deduction
- `fees` — Maker/taker tiered fee engine (30d rolling volume → tier lookup)
- `blockchain` — TRC-20 + ERC-20 monitor, withdrawal executor (hot < 10k USDT, cold ≥ 10k)
- `ws_gateway` — Axum WebSocket, Noise_XX + Signal ratchet on connect, 100ms orderbook snapshots
- `admin` — Admin REST handlers (stats, users, fee tiers, withdrawals) — internal only, not nginx-proxied

**Leptos frontend** — Rust → WASM (wasm32-unknown-unknown), built with Trunk, served by nginx:
- `src/app.rs` — Auth gate (mnemonic present → TradingPage, else OnboardingPage), theme context, AdminGuard
- `src/core/crypto.rs` — BIP39 key derivation, ed25519, x25519 in WASM
- `src/core/ws.rs` — Noise XX + Signal ratchet WebSocket client (the ONLY frontend↔backend channel)
- `src/core/api.rs` — ⚠️ Legacy HTTP client — nginx proxy removed, routes 404. Must be replaced with WS RPC.
- `src/features/trading/` — OrderBook, Chart, OrderEntry (all 11 order types)
- `src/features/wallet/` — Deposit (QR + stealth addr), withdraw (ZK proof), balance
- `src/features/admin/` — Admin login (no hCaptcha — correct for Tor), dashboard (stats, fee tiers, withdrawals)

## Key Conventions

**No public-facing API.** nginx does NOT proxy `/api/` or `/admin/api/`. Backend routes exist internally but are unreachable from outside. All user-facing communication must go through `/ws` (WebSocket).

**No PII logged anywhere.** Tracing fields: `event` (string enum), `order_id`, `trade_id` only. Never log `user_id`, `pubkey`, IP, or session tokens.

**Noise session = auth.** No JWT. After Noise_XX handshake, the `X-Session-Id` header maps to a Redis key. The `NoiseAuth` extractor (in `auth` crate) provides the authenticated `UserId`.

**Admin sessions are separate.** Admin uses `X-Admin-Session` header, stored in Redis under `admin:session:<token>`. 1-hour TTL vs 24-hour for user sessions. Default credentials: `admin` / `adminpassword`.

**Balances are always encrypted.** The `enc_balance` column in Postgres is AES-256-GCM. The decryption key lives only in Vault (`secret/torex/balance_enc_key`). In dev, `fetch_balance_key()` in settlement falls back to a zeroed key.

**Maker vs taker.** In the matching crate, the resting order (limit order on the book) is always the **maker**. The incoming order is the **taker**. Settlement assigns fee tiers accordingly.

**Fees deducted in settlement.** The `fees` crate's `record_trade_fees()` is called inside `process_trade()` in `settlement`. Fees are subtracted from balances after the main trade settlement within the same logical flow.

**Price ticks.** The in-memory order book stores prices as `i64` ticks (`(price * 1e6) as i64`). Use `price_to_ticks(f64) -> i64` and `ticks_to_price(i64) -> f64` helpers from `matching`.

**SQLX_OFFLINE=true for Docker builds.** The Dockerfile sets `SQLX_OFFLINE=true` so sqlx compile-time query checks are skipped. For dev, run `cargo sqlx prepare` after schema changes.

**wasm-bindgen 0.2.121 requires reference-types ENABLED.** Do NOT add `RUSTFLAGS="-C target-feature=-reference-types"` anywhere — it will cause `__wbindgen_externref_table_dealloc` linker errors.

**No external service calls from the frontend.** This is Tor — no Google Fonts, no hCaptcha, no analytics, no CDNs. Everything must be self-hosted or omitted. ⚠️ The nginx Content-Security-Policy still references `fonts.googleapis.com` — this needs to be removed.

**nginx rate-limits by `$server_addr`, not `$binary_remote_addr`.** Tor exit nodes share IPs; per-IP limiting would DOS all Tor users. Per-server-address limits apply uniformly.

**Vault runs in dev mode** (in-memory, no persistence). Vault data is lost on restart. For production, switch to file-backed storage with auto-unseal.

**User identity** is `BLAKE2b(ed25519_pubkey)[0..32]`, stored as hex TEXT in Postgres. Deterministic from the public key — no UUID generation.

**Stealth deposit addresses expire after 48 hours** if unconfirmed, to limit blockchain address enumeration.

**Top 200 trading pairs** are all `CRYPTO/USDT`. Chain config for ERC-20, TRC-20, BEP-20, Polygon, Avalanche, Arbitrum, Optimism, Solana is in the `chain_config` table seeded in `init.sql`.

**Tor vanity generation** runs once at first `docker compose up`. The `torex` prefix takes ~minutes with `-t 4`. Key stored in the `tor_data` Docker volume. Set `start_period: 600s` in the tor healthcheck.

## Supported Order Types (all 11 implemented in matching crate)

Limit, Market, StopLimit, StopMarket, TrailingStop, OCO (One-Cancels-Other), Iceberg, TWAP, FOK (Fill-or-Kill), IOC (Immediate-or-Cancel), PostOnly

**Known incomplete implementations:**
- **TWAP** — orders are accepted and stored but the time-sliced execution background task is not wired up yet.
- **Iceberg** — visible slice is placed correctly but the hidden reserve is not auto-refilled after the visible quantity fills.

## WebSocket RPC Protocol (next agent must implement)

All frontend↔backend communication should use this protocol over the Noise+Signal encrypted WebSocket at `/ws`:

```json
// Client → Server (encrypted in Signal ratchet)
{"id": "uuid", "method": "order.place", "params": {...}}

// Server → Client response
{"id": "uuid", "ok": true, "data": {...}}

// Server → Client push (no id, no response)
{"type": "orderbook", "data": {...}}
```

Methods needed: `auth.register`, `order.place`, `order.cancel`, `order.open`, `order.history`, `ticker.get`, `orderbook.get`, `candles.get`, `trades.recent`, `wallet.balance`, `wallet.deposit_address`, `wallet.withdraw`, `admin.login`, `admin.stats`, `admin.users`, `admin.create_user`, `admin.toggle_fee_free`, `admin.fee_tiers`, `admin.withdrawals`, `admin.approve_withdrawal`.

## Fee Schedule (defaults, admin-configurable)

| 30d Volume | Maker | Taker |
|---|---|---|
| < $10k | 0.16% | 0.26% |
| $10k – $50k | 0.14% | 0.24% |
| $50k – $100k | 0.12% | 0.22% |
| $100k – $250k | 0.10% | 0.20% |
| $250k – $1M | 0.08% | 0.18% |
| $1M – $10M | 0.06% | 0.16% |
| > $10M | 0.00% | 0.10% |

## Default Credentials

| Service | Username | Password / Token |
|---|---|---|
| Admin panel | `admin` | `adminpassword` — change after first boot |
| PostgreSQL | `postgres` | set via `POSTGRES_PASSWORD` in `.env` |
| Redis | — | set via `REDIS_PASSWORD` in `.env` |
| Vault | — | `VAULT_TOKEN=root` (dev mode) |

## Deploy

```bash
# Server: ssh root@hiddenservice, repo at /root/TorEx
cd /root/TorEx && git pull && docker compose up -d --build
```

## Environment Variables (.env.example)

```
POSTGRES_PASSWORD=   # Postgres superuser password
REDIS_PASSWORD=      # Redis requirepass
VAULT_TOKEN=root     # Vault dev root token
ETH_NODE_URL=        # wss:// Ethereum node (Infura, Alchemy, etc.)
TRON_NODE_URL=       # https://api.trongrid.io or private node
KAFKA_BROKER=kafka:9092
SECRET_NOISE_STATIC_KEY=   # hex-encoded static keypair for Noise
HCAPTCHA_SECRET=     # leave empty — hCaptcha bypassed on Tor
```
