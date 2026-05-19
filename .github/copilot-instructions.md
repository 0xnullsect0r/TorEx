# TorEx — Copilot Instructions

## Build & Test Commands

```bash
# Check Rust backend compiles
cd backend && cargo check

# Run Rust unit tests (no DB needed)
cd backend && cargo test --lib

# Run specific test
cd backend && cargo test --lib -p matching test_limit_buy_matches_limit_sell

# Build Flutter web
cd frontend && flutter pub get && flutter build web --release

# Validate Docker Compose config
docker compose config

# Start everything
cp .env.example .env  # fill in your node URLs
docker compose up --build
```

## Architecture

**Single Docker Compose deployment** — nginx on port 80 is the only public port. All other services (Postgres, Redis, Kafka, Vault, Tor) are on an internal bridge network.

**Rust backend** — Tokio + Axum 0.7 workspace of 9 crates compiled into one static musl binary (`torex`):
- `shared` — PgPool, Redis, Kafka wrappers, newtypes (UserId, Amount), AppConfig
- `crypto_primitives` — Noise XX (snow), Signal Double Ratchet (x25519-dalek + AES-GCM), Groth16 ZK (arkworks), DLEQ stealth addresses, balance AES-GCM encryption
- `auth` — Public-key-only auth (ed25519 signatures), Noise session = credential (no JWT), admin login (hCaptcha + TOTP + Passkeys), Redis sessions
- `wallet` — Internal USDT ledger, stealth deposit addresses, ZK-proof withdrawal, encrypted balance blob
- `matching` — In-memory BTreeMap order book (FIFO price-time priority), Limit + Market orders
- `settlement` — Kafka consumer on `trades`, atomic Postgres debit/credit + fee deduction
- `fees` — Maker/taker tiered fee engine (30d rolling volume → tier lookup)
- `blockchain` — TRC-20 + ERC-20 monitor, withdrawal executor (hot < 10k USDT, cold ≥ 10k)
- `ws_gateway` — Axum WebSocket, Noise_XX + Signal ratchet on connect, 100ms orderbook snapshots
- `admin` — Admin stats, fee tier CRUD, withdrawal approval, user list (no PII)

**Flutter frontend** — Web (WASM renderer) + Android/iOS. State: flutter_riverpod. Key files:
- `lib/app.dart` — Auth gate (mnemonic present → TradingPage, else OnboardingPage)
- `lib/core/crypto/` — WASM bridge for Rust crypto, key derivation service
- `lib/core/network/` — NoiseHttpClient (Dio interceptor), WsClient (WebSocket + Signal)
- `lib/features/trading/` — OrderBookWidget, ChartWidget (fl_chart), OrderEntryWidget
- `lib/features/wallet/` — Deposit (QR + stealth addr), withdraw (ZK proof), balance tiers
- `lib/features/admin/` — Admin login (hCaptcha + TOTP), dashboard (stats, fee revenue, withdrawals)

## Key Conventions

**No PII logged anywhere.** Tracing fields: `event` (string enum), `order_id`, `trade_id` only. Never log `user_id`, `pubkey`, IP, or session tokens.

**Noise session = auth.** No JWT. After Noise_XX handshake, the `X-Session-Id` header maps to a Redis key. The `NoiseAuth` extractor (in `auth` crate) provides the authenticated `UserId`.

**Admin sessions are separate.** Admin uses `X-Admin-Session` header, stored in Redis under `admin:session:<token>`. 1-hour TTL vs 24-hour for user sessions.

**Balances are always encrypted.** The `enc_balance` column in Postgres is AES-256-GCM. The decryption key lives only in Vault (`secret/torex/balance_enc_key`). In dev, `fetch_balance_key()` in settlement falls back to a zeroed key.

**Maker vs taker.** In the matching crate, the resting order (limit order on the book) is always the **maker**. The incoming order is the **taker**. Settlement assigns fee tiers accordingly.

**Fees deducted in settlement.** The `fees` crate's `record_trade_fees()` is called inside `process_trade()` in `settlement`. Fees are subtracted from balances after the main trade settlement within the same logical flow.

**Price ticks.** The in-memory order book stores prices as `i64` ticks (`(price * 1e6) as i64`). Use `price_to_ticks(f64) -> i64` and `ticks_to_price(i64) -> f64` helpers from `matching`.

**SQLX_OFFLINE=true for Docker builds.** The Dockerfile sets `SQLX_OFFLINE=true` so sqlx compile-time query checks are skipped. For dev, run `cargo sqlx prepare` after schema changes.

**Top 200 trading pairs** are all `CRYPTO/USDT`. Chain config for ERC-20, TRC-20, BEP-20, Polygon, Avalanche, Arbitrum, Optimism, Solana is in the `chain_config` table seeded in `init.sql`.

**Tor vanity generation** runs once at first `docker compose up`. The `torex` prefix takes ~minutes with `-t 4`. Key stored in the `tor_data` Docker volume. Set `start_period: 600s` in the tor healthcheck.

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

## Environment Variables (.env.example)

```
POSTGRES_PASSWORD=   # Postgres superuser password
REDIS_PASSWORD=      # Redis requirepass
VAULT_TOKEN=root     # Vault dev root token
ETH_NODE_URL=        # wss:// Ethereum node (Infura, Alchemy, etc.)
TRON_NODE_URL=       # https://api.trongrid.io or private node
KAFKA_BROKER=kafka:9092
SECRET_NOISE_STATIC_KEY=   # hex-encoded static keypair for Noise
HCAPTCHA_SECRET=     # hCaptcha secret key
HCAPTCHA_SITE_KEY=   # hCaptcha site key (used by Flutter)
```
