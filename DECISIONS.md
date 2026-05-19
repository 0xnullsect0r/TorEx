# TorEx Implementation Decisions

## Authentication

**Decision**: Noise Protocol XX-pattern sessions replace JWT entirely.  
**Rationale**: After a Noise_XX handshake, both parties have authenticated each other's static keys. The resulting transport keys replace bearer tokens. No token serialization, no JWT secret management.

**Decision**: User identity = BLAKE2b(ed25519_pubkey)[0..32].  
**Rationale**: Deterministic from the public key. No UUID generation. Collisions practically impossible with 256-bit output.

## Database

**Decision**: user_id stored as TEXT (hex-encoded) not BYTEA.  
**Rationale**: Rust SQLx macro-checked queries work cleanly with TEXT. BYTEA requires explicit casting everywhere. Hex encoding adds ~2x storage overhead which is negligible for user IDs.

**Decision**: Balances stored as AES-256-GCM encrypted BYTEA blobs.  
**Rationale**: Even with DB admin access, balances are unreadable without the Vault-held encryption key. The matching engine decrypts at runtime via the Vault key.

**Decision**: Run-time CREATE TABLE IF NOT EXISTS migrations instead of sqlx migrate.  
**Rationale**: Keeps the dev loop fast (no migration files to manage). Production upgrade path: add ALTER TABLE statements to the migration list.

## Order Book

**Decision**: Price stored as integer ticks (price * 1e6, rounded) in the in-memory book.  
**Rationale**: Avoids floating-point comparison issues in BTreeMap keys. The tick size is 1 micro-USDT = $0.000001.

**Decision**: Single in-memory book per process, not sharded.  
**Rationale**: For a compose single-node deployment, a single Tokio task with a Mutex is sufficient. Sharding adds complexity with no benefit until horizontal scaling is needed.

## Privacy

**Decision**: Nginx rate-limits by $server_addr, not $binary_remote_addr.  
**Rationale**: Tor exit nodes share IP addresses. Per-IP rate limiting would DOS all Tor users simultaneously. Per-server-address limits apply uniformly.

**Decision**: No user_id, pubkey, or IP logged anywhere.  
**Rationale**: Logs are often the first thing subpoenaed. Structured logs use event string enums and order/trade IDs only — both are unlinkable without also having the orders table.

**Decision**: Stealth addresses expire after 48 hours if unconfirmed.  
**Rationale**: Limits blockchain address enumeration. An attacker polling deposit-address cannot build a list of live addresses indefinitely.

## Fee Engine

**Decision**: Maker/taker tiered fees matching Kraken/Bitstamp/Crypto.com schedule.  
**Rationale**: Competitive with major exchanges. Maker incentive (lower fee) rewards liquidity provision. 30-day rolling volume resets anonymously — no PII attached.

**Decision**: Fees deducted in settlement, not at order placement.  
**Rationale**: Prevents fee-gaming (place and cancel) and ensures fees are only collected on executed trades.

## Tor Vanity Address

**Decision**: mkp224o runs at first container startup, blocking until a `torex*` prefix is found.  
**Rationale**: Vanity generation is CPU-intensive but one-time. The key is persisted in a Docker volume. Subsequent starts reuse the existing key.

**Decision**: 4-thread generation (-t 4) with prefix "torex" (~6 chars).  
**Rationale**: A 5-character v3 onion prefix takes ~minutes on modern hardware. 6 characters takes ~hours. We use "torex" (5 chars) as a balance between identity and generation time.

## Monitoring

**Decision**: /metrics endpoint not proxied externally by nginx.  
**Rationale**: Prometheus metrics could reveal trading activity patterns if public. Internal-only scraping ensures metrics are only accessible within the Docker network.

## Frontend

**Decision**: Flutter Web with WASM renderer.  
**Rationale**: Single codebase for web + mobile. WASM renderer enables the Rust crypto_primitives crate to be compiled to WASM and called from Dart via js_interop — no native bridge needed.

**Decision**: Direct-to-login for admin panel (no homepage).  
**Rationale**: The clearnet site is separate. The exchange interface itself has no marketing page. Users arrive knowing what they want.
