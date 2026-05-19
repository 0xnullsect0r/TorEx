# TorEx — Agent Handoff Document

## What This Is

TorEx is a privacy-first USDT spot exchange that runs **exclusively as a Tor v3 hidden service**. Users access it only through Tor Browser at the `.onion` address. There is no clearnet exposure. The entire stack runs in a single `docker compose` deployment.

## Current State (as of handoff)

**Working:**
- All 10 Docker containers build and run healthy
- Frontend (Leptos/WASM) serves at the `.onion` address
- Admin login at `/admin` with credentials `admin` / `adminpassword`
- Admin dashboard: stats, user list (GUID only), create user, fee-free toggle, fee tier management, withdrawal approval
- All 11 order types implemented in the matching engine: Limit, Market, StopLimit, StopMarket, TrailingStop, OCO, Iceberg, TWAP, FOK, IOC, PostOnly
- 4 UI themes: Dark, Light, Midnight, Cyberpunk
- Noise XX + Signal Double Ratchet encrypted WebSocket for all live data
- Tiered fee schedule (7 tiers, maker/taker, 30d rolling volume)

**Known gaps / next agent's work:**
- Frontend communicates with the backend over plain HTTP through nginx currently (see Security section below — this is the #1 priority to fix)
- User registration flow not fully tested end-to-end in the browser
- No real blockchain monitoring connected (ETH/TRON node URLs needed in `.env`)
- WebSocket live orderbook/trades push works but hasn't been verified in-browser
- No order history seeded / live trading not tested with real users
- Grafana/Loki/Prometheus are running but dashboards not configured

## Architecture

```
Tor Browser
    │  (Tor circuit)
    ▼
tor daemon (:9050) ── HiddenServicePort 80 ──► nginx (:80, host network)
                                                    │
                                          ┌─────────┴──────────┐
                                          │                      │
                                     static WASM            /ws WebSocket
                                     (Leptos SPA)           │
                                                    backend (:8080, host network)
                                                         │
                                              ┌──────────┼──────────┐
                                           postgres   redis      kafka
                                           (:5432)   (:6379)   (:9092)
                                              │
                                           vault (:8200)
                                           (balance enc keys, hot wallet keys)
```

**All services run `network_mode: host`** — they communicate via `127.0.0.1`. This is intentional for simplicity; on the server there is no external network exposure except what the OS firewall allows (only Tor connects to nginx port 80 from the `.onion` system).

## Stack Details

| Layer | Tech |
|---|---|
| Frontend | Leptos 0.7 (Rust→WASM), Trunk bundler, served by nginx |
| Backend | Tokio + Axum 0.7, Rust workspace of 9 crates |
| Database | PostgreSQL 16 |
| Cache/Sessions | Redis 7 |
| Event bus | Apache Kafka 3.7 (KRaft mode, no ZooKeeper) |
| Secrets | HashiCorp Vault (dev mode) |
| Monitoring | Prometheus + Grafana + Loki |
| Anonymity | Tor daemon, v3 hidden service, `torex` vanity prefix |

## Backend Crate Map

```
backend/
  crates/
    app/          — main.rs, router assembly, migrations
    shared/       — PgPool, Redis, Kafka wrappers, newtypes, AppConfig
    crypto_primitives/ — Noise XX, Signal ratchet, Groth16, DLEQ stealth addrs
    auth/         — Ed25519 pubkey auth, Noise sessions, admin bcrypt+TOTP
    wallet/       — USDT ledger, stealth deposit addrs, ZK withdrawal proofs
    matching/     — In-memory BTreeMap order book, all 11 order types
    settlement/   — Kafka consumer on `trades` topic, atomic DB debit/credit+fees
    fees/         — Maker/taker tiered fee engine, fee-free user flag
    blockchain/   — TRC-20 + ERC-20 monitor, withdrawal executor
    ws_gateway/   — Axum WebSocket, Noise XX + Signal ratchet on connect
    admin/        — Admin REST handlers (stats, users, fee tiers, withdrawals)
```

## Security Architecture

### What's implemented
- **Noise XX handshake** on every WebSocket connection — mutual authentication, forward secrecy
- **Signal Double Ratchet** on all WebSocket messages after handshake
- **No PII logged** — tracing only uses `order_id`, `trade_id`, `event` string enums. Never `user_id`, pubkey, IP
- **Encrypted balances** — `enc_balance` column in Postgres is AES-256-GCM, key lives in Vault only
- **No JWT** — Noise session = auth credential. `X-Session-Id` header maps to Redis key

### ⚠️ CRITICAL ISSUE — The #1 priority for the next agent

**The frontend currently calls the backend over plain HTTP** (`/api/*` and `/admin/api/*`). These nginx proxy blocks were REMOVED in the last commit — so those routes are now disabled. However, the frontend code (`frontend/src/core/api.rs`) still calls them, meaning the UI is partially broken.

**The correct fix**: migrate all frontend↔backend communication to the already-existing Noise+Signal encrypted WebSocket at `/ws`. The WebSocket gateway in `ws_gateway/src/lib.rs` currently only handles subscribe/push. It needs to be extended to handle bidirectional RPC (request/response pattern with a correlation ID).

The RPC format proposed:
```json
// Client → Server (encrypted in Signal ratchet)
{"id": "uuid", "method": "order.place", "params": {...}}

// Server → Client response
{"id": "uuid", "ok": true, "data": {...}}

// Server → Client push (no id)
{"type": "orderbook", "data": {...}}
```

Methods needed: `auth.register`, `order.place`, `order.cancel`, `order.open`, `order.history`, `ticker.get`, `orderbook.get`, `candles.get`, `trades.recent`, `wallet.balance`, `wallet.deposit_address`, `wallet.withdraw`, `admin.login`, `admin.stats`, `admin.users`, `admin.create_user`, `admin.toggle_fee_free`, `admin.fee_tiers`, `admin.withdrawals`, `admin.approve_withdrawal`.

## Frontend Structure

```
frontend/src/
  main.rs          — Leptos mount point
  app.rs           — Router, auth gate, theme context, AdminGuard
  core/
    api.rs         — ⚠️ HTTP client (should be replaced with WS RPC calls)
    ws.rs          — Noise XX + Signal ratchet WebSocket client
    crypto.rs      — WASM key derivation (BIP39 mnemonic → ed25519 + x25519)
    storage.rs     — localStorage wrapper (session_id, admin_session, mnemonic)
    types.rs       — Shared types (Order, Trade, OrderBook, etc.)
    mod.rs
  components/
    nav.rs         — Top nav bar, theme switcher, pair selector
    chart.rs       — Candlestick/line/depth chart (canvas-based)
    order_book.rs  — Live orderbook widget
    order_entry.rs — All 11 order type forms
  pages/
    onboarding.rs  — BIP39 mnemonic generate/import
    trading.rs     — Main trading page
    wallet.rs      — Deposit/withdraw
    admin_login.rs — Admin login (bypasses hCaptcha — correct for Tor)
    admin_dashboard.rs — 4-tab admin UI
```

## Things to NOT Do

1. **Do not add clearnet exposure.** No ports should be bound to `0.0.0.0` that aren't already there (all services use `network_mode: host` but the OS firewall + Tor is the perimeter).
2. **Do not add hCaptcha or any external service calls.** This is Tor — connecting to hcaptcha.com, Google Fonts CDN, or anything external defeats anonymity. (Note: the CSP currently still references `fonts.googleapis.com` — that should be cleaned up too.)
3. **Do not log PII.** No `user_id`, pubkey, IP, or session tokens in logs. Only structured events.
4. **Do not use JWT.** Auth is Noise session → Redis key. Admin auth is bcrypt+TOTP.
5. **Do not break the WASM build.** wasm-bindgen 0.2.121 requires reference-types to be ENABLED. `RUSTFLAGS="-C target-feature=-reference-types"` will cause `__wbindgen_externref_table_dealloc` errors. Do not add that flag anywhere.
6. **Do not use `network_mode: bridge` without updating all connection strings.** Currently everything talks to `127.0.0.1`. Switching to bridge networking requires updating all `POSTGRES_URL`, `REDIS_URL`, etc.

## Build Commands

```bash
# Check Rust backend compiles
cd backend && cargo check

# Run Rust unit tests (24 tests)
cd backend && cargo test --lib

# Check frontend WASM compiles
cd frontend && cargo check --target wasm32-unknown-unknown

# Validate docker compose config
docker compose config

# Full build and deploy
docker compose up -d --build
```

## Server Access

- SSH: `ssh root@hiddenservice` (auto-auth with key)
- Repo on server: `/root/TorEx`
- Deploy: `cd /root/TorEx && git pull && docker compose up -d --build`

## Default Credentials

| Service | Username | Password |
|---|---|---|
| Admin panel | `admin` | `adminpassword` |
| PostgreSQL | `postgres` | set in `.env` `POSTGRES_PASSWORD` |
| Redis | — | set in `.env` `REDIS_PASSWORD` |
| Vault | — | set in `.env` `VAULT_TOKEN` (dev mode) |

## Priority TODO for Next Agent

1. **[CRITICAL] Implement WS RPC** — migrate frontend from HTTP api.rs to WebSocket RPC. See Security section above for full spec.
2. **[HIGH] Fix CSP** — remove `fonts.googleapis.com` from Content-Security-Policy in nginx.conf and self-host the fonts (or remove them).
3. **[HIGH] End-to-end user flow** — test: onboarding → Noise handshake → register → place order → see in order book → cancel
4. **[MEDIUM] Vault prod mode** — currently running in dev mode (no seal, in-memory). Should be file-based or use auto-unseal for persistence across restarts.
5. **[MEDIUM] Grafana dashboards** — configure dashboards for order volume, trade count, WS connections, error rates.
6. **[LOW] TWAP executor** — TWAP orders are accepted but the time-sliced execution isn't wired to a background task yet.
7. **[LOW] Iceberg order refill** — Iceberg orders place the visible quantity but don't auto-refill from the hidden reserve quantity yet.
