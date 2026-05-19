# Architecture

This document describes the system design of TorEx: how components fit together, why key decisions were made, and how data flows through the system.

---

## Overview

TorEx is a **single-node, single-process exchange** deployed via Docker Compose. All backend logic lives in one Rust binary (`torex`). There are no microservices — a single Axum HTTP server serves REST, WebSocket, and admin endpoints, with background Tokio tasks for Kafka consumption and blockchain monitoring.

```
┌───────────────────────────────────────────────────────────────────┐
│  Docker Compose (internal bridge network)                         │
│                                                                   │
│  ┌────────┐   HTTP/WS     ┌──────────────────────────────────┐   │
│  │ nginx  │──────────────►│         torex binary             │   │
│  │  :80   │               │                                   │   │
│  └────────┘               │  axum router (merged)            │   │
│      ▲                    │  ├─ auth::router()               │   │
│      │                    │  ├─ wallet::router()             │   │
│  ┌───┴────┐               │  ├─ matching::router()           │   │
│  │  tor   │               │  ├─ fees::router()               │   │
│  │ :9050  │               │  ├─ admin::router()              │   │
│  └────────┘               │  └─ ws_gateway::router()         │   │
│                            │                                   │   │
│                            │  background tasks (tokio::spawn) │   │
│                            │  ├─ settlement consumer (kafka)  │   │
│                            │  ├─ eth monitor (polling)        │   │
│                            │  ├─ tron monitor (polling)       │   │
│                            │  └─ withdrawal executor (kafka)  │   │
│                            └──────┬─────┬──────┬─────┬────────┘   │
│                                   │     │      │     │            │
│                  ┌────────────────┘  ┌──┘   ┌──┘  ┌──┘           │
│                  ▼                   ▼      ▼     ▼               │
│             ┌─────────┐  ┌───────┐ ┌─────┐ ┌───────┐            │
│             │postgres │  │ redis │ │kafka│ │ vault │            │
│             └─────────┘  └───────┘ └─────┘ └───────┘            │
└───────────────────────────────────────────────────────────────────┘
```

---

## Rust Workspace

The backend is a Cargo workspace of 10 crates that compile into a single binary. This gives module boundaries without network overhead.

### Crate dependency graph

```
app
├── auth          → shared, crypto_primitives
├── wallet        → shared, crypto_primitives, auth
├── matching      → shared, auth
├── settlement    → shared, crypto_primitives, fees
├── fees          → shared
├── blockchain    → shared, crypto_primitives
├── ws_gateway    → shared, crypto_primitives, auth, matching
└── admin         → shared, auth, fees
```

`shared` provides:
- `PgPool` wrapper (`create_pool`)
- Redis `ConnectionManager` wrapper (`create_redis`)
- `KafkaProducer` and `KafkaConsumer` (rdkafka wrappers)
- Newtypes: `UserId`, `OrderId`, `Amount`
- `AppConfig` (from env vars)
- `AppError` (Axum `IntoResponse` impl)

### Static initialisation pattern

Each crate that needs shared state uses a `OnceLock<Arc<State>>`:

```rust
static MATCHING_STATE: OnceLock<Arc<MatchingState>> = OnceLock::new();

pub fn init_state(pool: PgPool, kafka: KafkaProducer) {
    let _ = MATCHING_STATE.set(Arc::new(MatchingState { pool, kafka, books: ... }));
}

fn matching_state() -> &'static Arc<MatchingState> {
    MATCHING_STATE.get().expect("matching not initialised")
}
```

`app/src/main.rs` initialises each crate in order, then merges all routers:

```rust
auth::init_state(pool.clone(), redis.clone(), config.clone());
wallet::init_state(pool.clone(), kafka.clone(), config.clone())?;
// matching / settlement don't need explicit init — state passed via closure

let app = Router::new()
    .merge(auth::router())
    .merge(wallet::router())
    .merge(matching::router(books.clone(), pool.clone(), kafka.clone()))
    .merge(fees::router(pool.clone()))
    .merge(admin::router(pool.clone(), redis.clone()))
    .merge(ws_gateway::router(books.clone(), redis.clone()));
```

---

## Authentication

### User authentication (Noise Protocol XX)

No passwords. No email. No JWT.

1. **Registration** — client submits ed25519 pubkey + signature. Server derives `user_id = BLAKE2b(pubkey)[0..32]` and creates a session in Redis (`sessions:{session_id}` → `user_id`, TTL 24h).
2. **Subsequent requests** — `X-Session-Id` header. The `NoiseAuth` extractor validates the Redis session and injects `UserId` into the handler.
3. **Optional upgrade** — `POST /api/noise/handshake` performs a Noise_XX handshake, establishing an encrypted channel. Required for WebSocket (where messages are also Signal-ratchet encrypted).

```
Noise_XX_25519_ChaChaPoly_BLAKE2s
```

The server's static keypair comes from `SECRET_NOISE_STATIC_KEY` in `.env`.

### Admin authentication

Separate session namespace (`admin:session:{token}` in Redis, TTL 1h):

1. `POST /admin/api/auth/login` with username + bcrypt password
2. Optionally: TOTP 6-digit code (TOTP-rs, SHA-1, 30s window)
3. Optionally: Passkey / WebAuthn (webauthn-rs 0.5)

Admin sessions use the `X-Admin-Session` header, never `X-Session-Id`.

---

## Order Book

### In-memory BTreeMap (per trading pair)

```rust
pub struct OrderBook {
    bids: BTreeMap<Reverse<i64>, VecDeque<Order>>,  // highest bid first
    asks: BTreeMap<i64, VecDeque<Order>>,            // lowest ask first
}
```

- Prices stored as **integer ticks** (`(price * 1_000_000) as i64`) to avoid floating-point comparison bugs.
- Each price level is a `VecDeque<Order>` for O(1) FIFO pop.
- Matching is synchronous and lock-protected (`parking_lot::Mutex`).
- One `OrderBook` per pair, stored in `Arc<DashMap<String, Arc<Mutex<OrderBook>>>>`.

### Matching algorithm

```
submit(order):
  if order is Buy Limit or Buy Market:
    loop while asks not empty and order not filled:
      best_ask = asks.first_entry()
      if order is Limit and order.price < best_ask.price: break
      match against best_ask queue (FIFO)
    if unfilled and Limit: insert into bids

  if order is Sell Limit or Sell Market:
    loop while bids not empty and order not filled:
      best_bid = bids.first_entry()
      if order is Limit and order.price > best_bid.price: break
      match against best_bid queue (FIFO)
    if unfilled and Limit: insert into asks
```

Each match produces a `Trade` event published to the `trades` Kafka topic.

### Order book snapshots (WebSocket)

A Tokio task per active WebSocket connection pushes `DepthSnapshot` every **100ms**. The snapshot is computed from the live BTreeMap — no separate snapshot store.

---

## Trade Settlement

Settlement is a Kafka consumer (`torex-settlement` consumer group) on the `trades` topic.

For each trade:

1. Look up maker and taker fee tiers (`fees::get_user_fee_tier`)
2. Calculate fees (`fees::calculate_fee`)
3. Begin Postgres transaction:
   - Debit seller's encrypted balance by `(quantity + taker_fee)` USDT
   - Credit buyer's encrypted balance by `(quantity - maker_fee)` USDT
   - Update order `filled` and `status` columns
   - Insert fee revenue records
   - Update 30-day rolling volumes for both parties
4. Commit
5. Publish to `settlement_complete` Kafka topic

Balance decryption/encryption uses the key from Vault (`secret/torex/balance_enc_key`). In dev mode, a zeroed key is used as fallback.

---

## Cryptographic Primitives (`crypto_primitives` crate)

### Balance encryption

```
AES-256-GCM
key:   256-bit from Vault (secret/torex/balance_enc_key)
nonce: 96-bit random (stored with ciphertext)
plaintext: u64 balance in micro-USDT (8 bytes, little-endian)
```

The `enc_balance` Postgres column is `BYTEA`: `nonce (12) ‖ ciphertext (8) ‖ tag (16)` = 36 bytes.

### Noise Protocol XX

```
Noise_XX_25519_ChaChaPoly_BLAKE2s
```

Used for:
- Initial HTTP session establishment (`POST /api/noise/handshake`)
- WebSocket connection setup (before Signal ratchet)

### Signal Double Ratchet

```
KDF: HKDF-SHA256
Diffie-Hellman: X25519
Message encryption: AES-256-GCM
```

After the Noise handshake, a Signal Double Ratchet session is established for the WebSocket connection. Every message advances the ratchet, providing forward secrecy per-message.

### Groth16 ZK Proofs (balance proofs, withdrawal proofs)

Circuit: `BalanceCircuit` on BN254 (ark-bn254).

The circuit proves: `balance >= threshold` without revealing the balance.

- **Balance proofs** — returned with the encrypted balance blob; clients can prove tier membership to the UI without decrypting their balance on the server.
- **Withdrawal proofs** — submitted with withdraw requests; server verifies `balance >= amount` before processing.

The proving key is generated once at startup (`ZkProver::setup()`).

### DLEQ Stealth Addresses

Based on Monero-style stealth addresses adapted for EVM/Tron.

```
receiver has: (spend_key, view_key)
sender generates: ephemeral r
stealth_pubkey = H(r * view_key) * G + spend_pubkey
stealth_address = keccak256(stealth_pubkey)[12..] (EVM) or base58(stealth_pubkey) (Tron/Solana)
```

To scan: the receiver tests `H(view_key * ephemeral_pubkey) * G + spend_pubkey == stealth_pubkey`.

---

## Blockchain Integration

The `blockchain` crate runs three Tokio tasks:

### ETH / ERC-20 monitor

Polls every 30 seconds via `eth_getTransactionByHash` JSON-RPC for unconfirmed deposit addresses. Credits the balance once `confirmations >= chain_config.confirmations` (12 for Ethereum mainnet).

### Tron / TRC-20 monitor

Polls TronGrid REST API every 30 seconds. Same confirmation logic (20 confirmations for Tron).

### Withdrawal executor

Consumes the `withdrawals` Kafka topic. Batches withdrawals and flushes every 5 minutes.

- **Hot wallet** (< $10,000 USDT): signing key fetched from Vault, transaction broadcast immediately.
- **Cold wallet** (≥ $10,000 USDT): withdrawal set to `pending_cold_wallet`; admin approves via dashboard; executor picks it up on next flush.

---

## Database Schema

See [`infra/postgres/init.sql`](../infra/postgres/init.sql) for the full production schema. Key design points:

| Table | Notes |
|---|---|
| `users` | `user_id TEXT` (hex BLAKE2b of pubkey). No email, no PII. |
| `balances` | `enc_balance BYTEA` — AES-GCM encrypted. RLS enabled. |
| `orders` | Status: `open`, `partially_filled`, `filled`, `cancelled` (all lowercase). |
| `trades` | Immutable audit log. |
| `deposits` | Stealth addresses expire via application logic (48h). |
| `withdrawals` | Status machine: `pending` → `pending_cold_wallet` → `approved` → `broadcast` → `confirmed`. |
| `fee_tiers` | Admin-editable 7-row table. |
| `rolling_volume` | Per-user 30-day aggregate, updated on each settlement. |
| `fee_revenue` | Maker/taker fee records for admin reporting. |
| `chain_config` | 8 chains seeded at init; `enabled` flag for per-chain toggles. |

**Row-level security** is enabled on `balances` and `orders` in production. Runtime dev migrations (`CREATE TABLE IF NOT EXISTS`) do not enable RLS.

---

## Tor Hidden Service

The `tor` container runs mkp224o on first start to generate a `torex*.onion` v3 address. The search uses 4 threads and typically takes 5–30 minutes for a 5-character prefix.

The generated key is stored in the `tor_data` Docker volume. Subsequent restarts skip generation and reuse the existing key.

`torrc` directs Tor to forward `.onion:80` → `nginx:80` on the internal network. Tor sees only nginx, not the backend directly.

---

## Nginx

nginx sits between the outside world and the backend:

- **IP stripping** — `proxy_set_header X-Forwarded-For ""` and `X-Real-IP ""` — the backend never sees client IPs.
- **Rate limiting** — `limit_req_zone $server_addr` — keyed on the server's own address, not the client's, so all Tor users get the same combined budget rather than per-IP limits.
- **Security headers** — `X-Frame-Options DENY`, `Referrer-Policy no-referrer`, strict `Content-Security-Policy`, `Permissions-Policy`.
- **Access log off** — no request log. Error log only.
- **Static assets** — Flutter web build served from `/usr/share/nginx/html`.
- **WebSocket upgrade** — `/ws` proxied to backend with proper `Upgrade` headers.

---

## Monitoring

| Component | Port (internal) | Purpose |
|---|---|---|
| Prometheus | 9090 | Scrapes `/metrics` from backend every 15s |
| Grafana | 3000 | Dashboards; datasources: Prometheus + Loki |
| Loki | 3100 | Log aggregation (Docker json-file driver) |

Grafana and Prometheus are on the internal network only — not exposed via nginx. Access them via `docker compose exec grafana` or an SSH tunnel.

The backend exposes a minimal `/metrics` endpoint in Prometheus text format. For production use, add `prometheus` crate instrumentation to each handler.
