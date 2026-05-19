CREATE EXTENSION IF NOT EXISTS pgcrypto;
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- Schema matches backend crates/app/src/main.rs run_migrations() exactly.
-- All columns are TEXT/UUID/BYTEA/DOUBLE PRECISION to match sqlx queries.

CREATE TABLE users (
  user_id      TEXT PRIMARY KEY,
  pubkey       TEXT NOT NULL,
  view_pubkey  TEXT NOT NULL,
  spend_pubkey TEXT NOT NULL,
  created_at   TIMESTAMPTZ NOT NULL
);

CREATE TABLE admin_users (
  username           TEXT PRIMARY KEY,
  password_hash      TEXT NOT NULL,
  totp_secret        TEXT,
  totp_enrolled      BOOLEAN NOT NULL DEFAULT false,
  passkey_credential TEXT
);

CREATE TABLE balances (
  user_id     TEXT PRIMARY KEY,
  enc_balance BYTEA NOT NULL,
  updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE deposits (
  id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id          TEXT NOT NULL,
  stealth_addr     TEXT NOT NULL UNIQUE,
  ephemeral_pubkey TEXT NOT NULL,
  chain            TEXT NOT NULL,
  amount           NUMERIC,
  tx_hash          TEXT,
  created_at       TIMESTAMPTZ NOT NULL,
  confirmed_at     TIMESTAMPTZ
);
CREATE INDEX ON deposits(user_id);
CREATE INDEX ON deposits(stealth_addr);

CREATE TABLE withdrawals (
  withdrawal_id TEXT PRIMARY KEY,
  user_id       TEXT NOT NULL,
  dest_address  TEXT NOT NULL,
  amount        NUMERIC NOT NULL,
  chain         TEXT NOT NULL,
  status        TEXT NOT NULL,
  tx_hash       TEXT,
  created_at    TIMESTAMPTZ NOT NULL,
  broadcast_at  TIMESTAMPTZ
);
CREATE INDEX ON withdrawals(user_id, status);

CREATE TABLE orders (
  order_id   UUID PRIMARY KEY,
  user_id    TEXT NOT NULL,
  pair       TEXT NOT NULL,
  side       TEXT NOT NULL,
  order_type TEXT NOT NULL,
  price      DOUBLE PRECISION NOT NULL,
  quantity   DOUBLE PRECISION NOT NULL,
  filled     DOUBLE PRECISION NOT NULL,
  status     TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL
);
CREATE INDEX ON orders(user_id, status);
CREATE INDEX ON orders(pair, status, side, price);

CREATE TABLE trades (
  trade_id        UUID PRIMARY KEY,
  pair            TEXT NOT NULL,
  price           DOUBLE PRECISION NOT NULL,
  quantity        DOUBLE PRECISION NOT NULL,
  buyer_order_id  UUID NOT NULL,
  seller_order_id UUID NOT NULL,
  buyer_user_id   TEXT NOT NULL,
  seller_user_id  TEXT NOT NULL,
  executed_at     TIMESTAMPTZ NOT NULL
);
CREATE INDEX ON trades(pair, executed_at DESC);

CREATE TABLE chain_config (
  chain_id      TEXT PRIMARY KEY,
  chain_name    TEXT NOT NULL,
  usdt_contract TEXT,
  rpc_env_key   TEXT NOT NULL,
  confirmations INT NOT NULL DEFAULT 12,
  enabled       BOOLEAN NOT NULL DEFAULT TRUE
);

INSERT INTO chain_config VALUES
  ('ERC20', 'Ethereum',  '0xdAC17F958D2ee523a2206206994597C13D831ec7',  'ETH_NODE_URL',   12, TRUE),
  ('TRC20', 'Tron',      'TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t',         'TRON_NODE_URL',  20, TRUE),
  ('BEP20', 'BSC',       '0x55d398326f99059fF775485246999027B3197955',   'BSC_NODE_URL',   15, TRUE),
  ('MATIC', 'Polygon',   '0xc2132D05D31c914a87C6611C10748AEb04B58e8F',   'MATIC_NODE_URL', 64, TRUE),
  ('AVAX',  'Avalanche', '0x9702230A8Ea53601f5cD2dc00fDBc13d4dF4A8c7',   'AVAX_NODE_URL',  64, TRUE),
  ('ARB',   'Arbitrum',  '0xFd086bC7CD5C481DCC9C85ebE478A1C0b69FCbb9',   'ARB_NODE_URL',   64, TRUE),
  ('OP',    'Optimism',  '0x94b008aA00579c1307B0EF2c499aD98a8ce58e58',   'OP_NODE_URL',    64, TRUE),
  ('SOL',   'Solana',    'Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB', 'SOL_NODE_URL',   32, TRUE);
