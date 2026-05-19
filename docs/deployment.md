# Deployment Guide

This guide covers deploying TorEx in a production-hardened configuration on a single Linux server.

---

## Server Requirements

| Resource | Minimum | Recommended |
|---|---|---|
| CPU | 2 cores | 4+ cores |
| RAM | 4 GB | 8 GB |
| Disk | 40 GB SSD | 100 GB SSD |
| OS | Ubuntu 22.04 LTS | Ubuntu 24.04 LTS |
| Network | 100 Mbps | 1 Gbps |

The server should have **no public internet ports open except 22 (SSH)**. Tor users reach the exchange exclusively via the `.onion` address; clearnet access is not intended.

---

## Prerequisites

```bash
# Install Docker + Compose
curl -fsSL https://get.docker.com | sh
sudo usermod -aG docker $USER

# Install Flutter (for frontend build)
sudo snap install flutter --classic

# Install Rust (for local development only)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

---

## First-Time Setup

### 1. Configure environment

```bash
cp .env.example .env
chmod 600 .env
```

Edit `.env`:

```bash
# Strong random passwords
POSTGRES_PASSWORD=$(openssl rand -hex 32)
REDIS_PASSWORD=$(openssl rand -hex 32)

# Vault token — for production use a proper Vault setup, not dev mode
VAULT_TOKEN=$(openssl rand -hex 32)

# Ethereum node — use a private node or Infura/Alchemy
ETH_NODE_URL=wss://mainnet.infura.io/ws/v3/YOUR_PROJECT_ID

# Tron — TronGrid free tier works for monitoring
TRON_NODE_URL=https://api.trongrid.io

# Noise static key — 64 random bytes as hex
SECRET_NOISE_STATIC_KEY=$(openssl rand -hex 64)

# hCaptcha — register at hcaptcha.com
HCAPTCHA_SECRET=your_secret_key
HCAPTCHA_SITE_KEY=your_site_key
```

### 2. Build the Flutter web frontend

```bash
cd frontend
flutter pub get
flutter build web --release --web-renderer canvaskit
cd ..
```

The build output is placed in `frontend/build/web/`, which is mounted into the nginx container.

### 3. Start everything

```bash
docker compose up -d --build
```

Watch startup:

```bash
docker compose logs -f
```

Expected startup sequence:
1. `postgres` → healthy (~5s)
2. `redis` → healthy (~2s)
3. `kafka` → healthy (~15s)
4. `vault` → healthy (~3s)
5. `backend` → healthy (~10s after its deps)
6. `nginx` → healthy (~3s after backend)
7. `tor` → starts mkp224o vanity key generation (**5–30 minutes**)

Once the `.onion` address is generated:

```bash
docker compose logs tor | grep "Found:"
# [*] Found: torex7abc123xyz[...].onion
```

---

## Vault Setup (Production)

The Docker Compose uses Vault in **dev mode** with a fixed root token. For production, switch to server mode with proper storage:

```bash
# vault/config.hcl for server mode
storage "raft" {
  path = "/vault/data"
  node_id = "torex-vault"
}

listener "tcp" {
  address = "0.0.0.0:8200"
  tls_disable = true  # TLS handled by nginx internally
}

api_addr = "http://vault:8200"
cluster_addr = "http://vault:8201"
ui = false
```

After unsealing Vault, store the balance encryption key:

```bash
# Generate a 256-bit key
BALANCE_KEY=$(openssl rand -hex 32)

# Write to Vault
docker compose exec vault vault kv put secret/torex/balance_enc_key value=$BALANCE_KEY

# Also store hot wallet signing keys
docker compose exec vault vault kv put secret/torex/eth_signing_key value=$ETH_SIGNING_KEY
docker compose exec vault vault kv put secret/torex/tron_signing_key value=$TRON_SIGNING_KEY
```

---

## Change Default Admin Password

Immediately after first boot:

```bash
# Connect to the database
docker compose exec postgres psql -U postgres torex

-- Generate new bcrypt hash (use a strong password)
UPDATE admin_users
SET password_hash = '$2b$12$<your_bcrypt_hash>'
WHERE username = 'admin';
```

Or via the admin panel:

1. Navigate to `http://localhost/admin` (or the `.onion` address)
2. Log in with `admin` / `adminpassword`
3. Change the password immediately
4. Set up TOTP with an authenticator app
5. Optionally register a Passkey (FIDO2/WebAuthn)

---

## Blockchain Node Configuration

TorEx monitors deposits by polling blockchain nodes. For production:

### Ethereum
- Use a **WebSocket endpoint** (`wss://`) for event subscription
- Recommended: [Infura](https://infura.io), [Alchemy](https://alchemy.com), or a private geth/reth node
- Required: `eth_blockNumber`, `eth_getTransactionByHash`, `eth_call` methods

### Tron
- [TronGrid](https://developers.tron.network) free tier: 10,000 req/day
- For production: [NOWNodes](https://nownodes.io) or self-hosted tron-node
- Required: TronGrid REST API

### Additional chains (BEP-20, Polygon, etc.)
Add `BSC_NODE_URL`, `MATIC_NODE_URL`, etc. to `.env` and `docker-compose.yml`. The `chain_config` table already has entries for all 8 chains — just enable the nodes.

---

## Hot Wallet Setup

The hot wallet is used for automatic withdrawals < $10,000 USDT.

```bash
# Generate a new wallet keypair (example using cast from Foundry)
cast wallet new

# Store signing key in Vault
docker compose exec vault vault kv put secret/torex/eth_signing_key \
  value=0x<private_key_hex>

# Fund the hot wallet with USDT
# Recommended float: 2–5x your expected daily withdrawal volume
```

**Security recommendations:**
- Keep hot wallet balance at minimum necessary
- Monitor hot wallet balance; alert at < 1,000 USDT
- Rotate hot wallet keys monthly
- Never put more than $50,000 USDT in the hot wallet

---

## Database Backups

```bash
# Daily backup script (add to cron)
#!/bin/bash
DATE=$(date +%Y%m%d_%H%M%S)
docker compose exec -T postgres pg_dump -U postgres torex \
  | gzip > /backups/torex_${DATE}.sql.gz

# Retain 30 days
find /backups -name "torex_*.sql.gz" -mtime +30 -delete
```

Add to crontab:
```
0 3 * * * /opt/torex/backup.sh
```

**Restore:**
```bash
gunzip < /backups/torex_20240101_030000.sql.gz \
  | docker compose exec -T postgres psql -U postgres torex
```

---

## Updating

```bash
# Pull latest code
git pull

# Rebuild backend image
docker compose build backend

# Rebuild frontend
cd frontend && flutter pub get && flutter build web --release && cd ..

# Rolling restart (Tor key is preserved in the volume)
docker compose up -d
```

The Tor key in `tor_data` volume is preserved across restarts — the `.onion` address does not change.

---

## Adding Trading Pairs

New pairs are handled automatically — the in-memory order book creates a new `OrderBook` per pair on first order submission. No restart needed.

To add a new chain/token:

1. Insert into `chain_config`:
```sql
INSERT INTO chain_config VALUES (
  'NEWCHAIN', 'New Chain', '0xTokenContract', 'NEWCHAIN_NODE_URL', 12, TRUE
);
```

2. Add `NEWCHAIN_NODE_URL` to `.env` and `docker-compose.yml` environment section.

3. Add polling logic in `blockchain/src/lib.rs` mirroring `poll_eth` or `poll_tron`.

---

## Monitoring

Grafana is available internally. Access it via SSH tunnel:

```bash
ssh -L 3000:localhost:3000 user@your-server
# Then open http://localhost:3000 in your browser
```

Default Grafana credentials: `admin` / `admin` (change on first login).

Loki receives logs from all Docker containers via the `json-file` logging driver. The Grafana Loki datasource is pre-configured.

**Recommended alerts:**
- Hot wallet balance < 1,000 USDT
- Backend unhealthy for > 60s
- Kafka consumer lag > 1,000 messages
- `pending_cold_wallet` withdrawals pending > 1h

---

## Firewall

```bash
# UFW configuration
ufw default deny incoming
ufw default allow outgoing
ufw allow 22/tcp    # SSH
ufw allow 80/tcp    # nginx (Tor accesses this)
ufw enable
```

Port 80 is needed because Tor's hidden service connects to nginx locally. No other ports need to be public — Postgres, Redis, Kafka, Vault, and Grafana are all on the Docker internal bridge.

---

## Log Retention

Docker JSON log files are capped at 10 MB with 3 rotations (configured in `docker-compose.yml`):

```yaml
x-logging: &default-logging
  driver: json-file
  options:
    max-size: "10m"
    max-file: "3"
```

Loki is the persistent log store. Adjust `loki-config.yml` retention as needed.

---

## Disaster Recovery

| Scenario | Recovery |
|---|---|
| Backend container crash | `docker compose up -d backend` (auto-restart is already configured) |
| Postgres data loss | Restore from daily backup; replay Kafka topics if available |
| Tor key loss | Delete `tor_data` volume; `docker compose up tor` regenerates the key (new `.onion` address) |
| Vault seal (server mode) | Re-unseal with stored unseal keys; balance key and signing keys remain in Vault storage |
| Hot wallet key loss | Retrieve from Vault backup; hot wallet funds remain on-chain |

---

## Security Checklist

- [ ] Changed default admin password and enrolled TOTP
- [ ] Set strong random values for `POSTGRES_PASSWORD`, `REDIS_PASSWORD`, `VAULT_TOKEN`
- [ ] Stored `SECRET_NOISE_STATIC_KEY` securely (cannot be rotated without all clients reconnecting)
- [ ] Vault dev mode replaced with server mode for production
- [ ] Balance encryption key stored in Vault (not in `.env`)
- [ ] Hot wallet funded with minimum necessary balance
- [ ] Firewall configured (only 22 and 80 open)
- [ ] Daily database backups configured and tested
- [ ] Monitoring alerts configured
- [ ] Server OS fully patched and auto-updates enabled
