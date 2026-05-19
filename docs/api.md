# API Reference

All endpoints are served at the nginx reverse proxy on port 80 (or via the `.onion` address).

- REST: `http://[host]/api/...`
- Admin REST: `http://[host]/admin/api/...`
- WebSocket: `ws://[host]/ws`

---

## Authentication

TorEx uses **Noise Protocol XX** for transport-layer authentication. There is no username/password for regular users — identity is derived from a BIP-39 mnemonic.

### Registration Flow

```
Client                                  Server
  │                                        │
  │  POST /api/auth/register               │
  │  { pubkey, view_pubkey, spend_pubkey,  │
  │    signature, timestamp }              │
  │ ──────────────────────────────────── ► │
  │                                        │  Verify ed25519 sig over (pubkey+timestamp)
  │  { user_id, session_id }               │  within 30s replay window
  │ ◄ ──────────────────────────────────── │
```

**Registration request body:**

| Field | Type | Description |
|---|---|---|
| `pubkey` | `string` (hex) | ed25519 public key |
| `view_pubkey` | `string` (hex) | x25519 view key (for stealth address scanning) |
| `spend_pubkey` | `string` (hex) | x25519 spend key (for stealth address derivation) |
| `signature` | `string` (base64) | ed25519 signature over `SHA256(pubkey_bytes ‖ timestamp_le64)` |
| `timestamp` | `number` | Unix seconds; must be within 30s of server time |

**Response:**

```json
{
  "user_id": "a3f8c2...",
  "session_id": "uuid-v4"
}
```

After registration, include `X-Session-Id: <session_id>` on all subsequent requests.

---

### Noise Handshake

Establishes an encrypted channel. Optional but required for WebSocket use.

```
POST /api/noise/handshake
Content-Type: application/octet-stream
Body: <Noise_XX initiator first message (binary)>
```

Returns the server's Noise_XX response message (binary). The derived transport keys are then used to encrypt all WebSocket frames.

---

### Logout

```
POST /api/auth/logout
X-Session-Id: <session_id>
```

Immediately revokes the session in Redis.

---

## Order Book

### Get Order Book Depth

```
GET /api/orderbook/{pair}
```

Returns the top-25 bid and ask levels.

**Example:** `GET /api/orderbook/BTC_USDT`

```json
{
  "bids": [
    [68420.50, 0.523],
    [68400.00, 1.200],
    ...
  ],
  "asks": [
    [68421.00, 0.100],
    [68430.00, 2.500],
    ...
  ]
}
```

Each entry is `[price, total_quantity]`. Bids are descending, asks are ascending.

---

### Get Candles

```
GET /api/candles/{pair}?interval={interval}
```

| Parameter | Values | Default |
|---|---|---|
| `interval` | `1m`, `5m`, `15m`, `1h`, `4h`, `1d` | `1m` |

```json
[
  {
    "open_time": "2024-01-01T00:00:00Z",
    "open": 68000.00,
    "high": 68500.00,
    "low": 67800.00,
    "close": 68420.50,
    "volume": 123.456
  }
]
```

---

## Orders

### Place Order

```
POST /api/orders
X-Session-Id: <session_id>
Content-Type: application/json
```

**Request body:**

| Field | Type | Required | Description |
|---|---|---|---|
| `pair` | `string` | ✅ | e.g. `"BTC_USDT"` |
| `side` | `"buy"` \| `"sell"` | ✅ | Order side |
| `order_type` | `"limit"` \| `"market"` | ✅ | Order type |
| `price` | `number` | limit only | Price in USDT (ignored for market orders) |
| `quantity` | `number` | ✅ | Amount in base currency |
| `zk_proof` | `number[]` (32 bytes) | ✅ | ZK proof of sufficient balance |

**Response:**

```json
{
  "order_id": "550e8400-e29b-41d4-a716-446655440000",
  "trades": [
    {
      "trade_id": "...",
      "price": 68420.50,
      "quantity": 0.5,
      "buyer_user_id": "...",
      "seller_user_id": "...",
      "executed_at": "2024-01-01T00:00:00Z"
    }
  ]
}
```

`trades` is non-empty when the order matched immediately (taker). An empty array means the order is resting on the book (maker).

---

### List Open Orders

```
GET /api/orders
X-Session-Id: <session_id>
```

Returns orders with status `open` or `partially_filled`.

```json
[
  {
    "order_id": "550e8400-...",
    "pair": "BTC_USDT",
    "side": "buy",
    "order_type": "limit",
    "price": 68000.00,
    "quantity": 1.0,
    "filled": 0.0,
    "status": "open",
    "created_at": "2024-01-01T00:00:00Z"
  }
]
```

---

### Cancel Order

```
DELETE /api/orders/{order_id}
X-Session-Id: <session_id>
```

Returns `{"cancelled": true}` on success. Only the order owner can cancel.

---

## Wallet

### Get Balance

```
GET /api/wallet/balance
X-Session-Id: <session_id>
```

Returns the AES-256-GCM encrypted balance blob and ZK tier proofs. The client decrypts using the balance key derived from their mnemonic.

```json
{
  "enc_balance": "<base64 encrypted blob>",
  "tier_proofs": [
    { "tier": 1000, "proof": "<base64 groth16 proof>" },
    { "tier": 10000, "proof": "<base64 groth16 proof>" }
  ]
}
```

---

### Generate Deposit Address

```
GET /api/wallet/deposit-address?chain={chain}
X-Session-Id: <session_id>
```

| Chain | Value |
|---|---|
| Ethereum (ERC-20) | `ERC20` |
| Tron (TRC-20) | `TRC20` |
| BSC (BEP-20) | `BEP20` |
| Polygon | `MATIC` |
| Avalanche | `AVAX` |
| Arbitrum | `ARB` |
| Optimism | `OP` |
| Solana | `SOL` |

**Response:**

```json
{
  "stealth_address": "0x742d35Cc6634C0532925a3b8D4C9b8F3e2A1B2C3",
  "ephemeral_pubkey": "a3f8c2d1e4b5...",
  "chain": "ERC20"
}
```

Each address is a fresh DLEQ stealth address, unlinkable to your account without your view key. Unconfirmed addresses expire after **48 hours**.

---

### Withdraw

```
POST /api/wallet/withdraw
X-Session-Id: <session_id>
Content-Type: application/json
```

**Request body:**

| Field | Type | Description |
|---|---|---|
| `dest_address` | `string` | Destination address for the target chain |
| `amount` | `string` | Withdrawal amount in USDT (string to avoid float precision loss) |
| `chain` | `string` | Chain identifier (see deposit chain table) |
| `zk_proof` | `string` (base64) | Groth16 proof that balance ≥ amount |

Withdrawals < 10,000 USDT are executed automatically from the hot wallet. Withdrawals ≥ 10,000 USDT require admin cold-wallet approval.

```json
{ "withdrawal_id": "wdl_..." }
```

---

## Fees

### Get Current Fee Tier

```
GET /api/fees
X-Session-Id: <session_id>
```

Returns the caller's current fee tier based on 30-day rolling volume.

```json
{
  "maker_bps": 16,
  "taker_bps": 26,
  "maker_pct": "0.16%",
  "taker_pct": "0.26%",
  "volume_30d": 4230.50,
  "next_tier_at": 10000.00
}
```

---

## WebSocket API

```
ws://[host]/ws?session_id={session_id}
```

On connect, the client performs a **Noise_XX handshake** followed by **Signal Double Ratchet key exchange**. All subsequent frames are double-ratchet encrypted.

### Subscribe

```json
{ "action": "subscribe", "channel": "orderbook", "pair": "BTC_USDT" }
{ "action": "subscribe", "channel": "trades",    "pair": "BTC_USDT" }
{ "action": "subscribe", "channel": "user" }
```

### Order Book Snapshot (push, every 100ms)

```json
{
  "type": "orderbook",
  "pair": "BTC_USDT",
  "bids": [[68420.50, 1.23], [68400.00, 0.50]],
  "asks": [[68421.00, 0.10], [68430.00, 2.50]],
  "timestamp": "2024-01-01T00:00:00Z"
}
```

### Trade Event (push, on each match)

```json
{
  "type": "trade",
  "pair": "BTC_USDT",
  "price": 68420.50,
  "quantity": 0.5,
  "side": "buy",
  "executed_at": "2024-01-01T00:00:00Z"
}
```

### User Order Update (push, authenticated channel)

```json
{
  "type": "order_update",
  "order_id": "550e8400-...",
  "status": "partially_filled",
  "filled": 0.25,
  "quantity": 0.5
}
```

---

## Admin API

All admin endpoints require `X-Admin-Session: <token>` obtained via admin login.

### Login

```
POST /admin/api/auth/login
Content-Type: application/json

{
  "username": "admin",
  "password": "adminpassword",
  "totp_code": "123456"   ← optional if TOTP not yet enrolled
}
```

If TOTP is enrolled, returns `{"totp_required": true}` on first call without `totp_code`. Submit again with the 6-digit code to receive the session token.

```json
{ "session_token": "a3f8c2..." }
```

---

### TOTP Setup

```
POST /admin/api/auth/totp/setup
X-Admin-Session: <token>
```

Returns a TOTP secret and QR code URL to scan with an authenticator app.

```
POST /admin/api/auth/totp/verify
X-Admin-Session: <token>

{ "code": "123456" }
```

Confirms TOTP enrollment.

---

### Passkey Registration

```
POST /admin/api/auth/passkey/register-start
POST /admin/api/auth/passkey/register-finish
POST /admin/api/auth/passkey/login-start
POST /admin/api/auth/passkey/login-finish
```

Standard WebAuthn ceremony. See [WebAuthn spec](https://www.w3.org/TR/webauthn-2/) for request/response shapes.

---

### Exchange Statistics

```
GET /admin/api/stats
X-Admin-Session: <token>
```

```json
{
  "total_users": 142,
  "active_orders": 831,
  "pending_withdrawals": 3,
  "volume_24h": 2841923.50,
  "trades_24h": 4821,
  "deposits_24h": 17,
  "ws_connections": 0,
  "healthy": true
}
```

---

### Volume by Pair

```
GET /admin/api/volume
X-Admin-Session: <token>
```

```json
{
  "volume": [
    {
      "pair": "BTC_USDT",
      "volume_24h": 1423000.00,
      "volume_7d": 9100000.00,
      "trade_count": 2341
    }
  ]
}
```

---

### List Users

```
GET /admin/api/users?limit=50&offset=0
X-Admin-Session: <token>
```

Returns user IDs, creation timestamps, and 30-day volumes. No PII is stored or returned.

```json
{
  "users": [
    {
      "user_id": "a3f8c2...",
      "created_at": "2024-01-01T00:00:00Z",
      "volume_30d": 4230.50,
      "order_count": 87
    }
  ],
  "limit": 50,
  "offset": 0
}
```

---

### List Withdrawals

```
GET /admin/api/withdrawals?status=pending
X-Admin-Session: <token>
```

| Status filter | Meaning |
|---|---|
| `pending` | Queued, not yet broadcast |
| `pending_cold_wallet` | Awaiting admin approval (≥ $10k) |
| `approved` | Approved, pending broadcast |
| `broadcast` | Sent to chain |
| `confirmed` | On-chain confirmed |

---

### Approve Withdrawal (Cold Wallet)

```
POST /admin/api/withdrawals/{withdrawal_id}/approve
X-Admin-Session: <token>
```

Only applies to withdrawals in `pending_cold_wallet` status.

---

### List Fee Tiers

```
GET /admin/api/fee-tiers
X-Admin-Session: <token>
```

```json
{
  "tiers": [
    {
      "id": 1,
      "min_volume": 0,
      "max_volume": 10000,
      "maker_bps": 16,
      "taker_bps": 26,
      "maker_pct": "0.16%",
      "taker_pct": "0.26%",
      "enabled": true
    }
  ]
}
```

---

### Update Fee Tier

```
PUT /admin/api/fee-tiers/{id}
X-Admin-Session: <token>
Content-Type: application/json

{
  "min_volume": 0,
  "max_volume": 10000,
  "maker_bps": 14,
  "taker_bps": 24
}
```

Changes take effect immediately for the next trade settled. `bps` values must be 0–1000.

---

### Fee Revenue

```
GET /admin/api/fees/revenue
X-Admin-Session: <token>
```

Returns aggregated fee revenue by pair, split into maker/taker.

---

## Error Responses

All errors follow the same shape:

```json
{ "error": "human-readable message" }
```

| HTTP Status | Meaning |
|---|---|
| `400 Bad Request` | Malformed input or validation failure |
| `401 Unauthorized` | Missing or invalid `X-Session-Id` / `X-Admin-Session` |
| `403 Forbidden` | Valid session but insufficient permissions |
| `404 Not Found` | Resource does not exist |
| `409 Conflict` | e.g. duplicate pubkey registration |
| `429 Too Many Requests` | Rate limit exceeded (20 req/s burst 40) |
| `500 Internal Server Error` | Unexpected server error |

---

## Health & Metrics

```
GET /health   → 200 OK  "OK"
GET /metrics  → Prometheus text format
```

`/metrics` is only reachable from within the Docker internal network. It is not proxied by nginx.
