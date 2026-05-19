CREATE EXTENSION IF NOT EXISTS pgcrypto;
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- Per-service roles
CREATE ROLE auth_user LOGIN PASSWORD 'auth_pass';
CREATE ROLE wallet_user LOGIN PASSWORD 'wallet_pass';
CREATE ROLE matching_user LOGIN PASSWORD 'matching_pass';
CREATE ROLE settlement_user LOGIN PASSWORD 'settlement_pass';
CREATE ROLE blockchain_user LOGIN PASSWORD 'blockchain_pass';
CREATE ROLE admin_user LOGIN PASSWORD 'admin_pass';

CREATE TABLE users (
  user_id      BYTEA PRIMARY KEY,
  pubkey       BYTEA NOT NULL UNIQUE,
  view_pubkey  BYTEA NOT NULL,
  spend_pubkey BYTEA NOT NULL,
  created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE balances (
  user_id      BYTEA PRIMARY KEY REFERENCES users(user_id),
  enc_amount   BYTEA NOT NULL,
  last_updated TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE orders (
  order_id   UUID PRIMARY KEY,
  user_id    BYTEA NOT NULL REFERENCES users(user_id),
  pair       TEXT NOT NULL,
  side       TEXT NOT NULL CHECK (side IN ('buy','sell')),
  order_type TEXT NOT NULL CHECK (order_type IN ('limit','market')),
  price      NUMERIC(30,6),
  quantity   NUMERIC(30,6) NOT NULL,
  filled     NUMERIC(30,6) NOT NULL DEFAULT 0,
  status     TEXT NOT NULL DEFAULT 'open',
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX ON orders(user_id, status);
CREATE INDEX ON orders(pair, status, side, price);

CREATE TABLE trades (
  trade_id        UUID PRIMARY KEY,
  pair            TEXT NOT NULL,
  price           NUMERIC(30,6) NOT NULL,
  quantity        NUMERIC(30,6) NOT NULL,
  buyer_order_id  UUID REFERENCES orders(order_id),
  seller_order_id UUID REFERENCES orders(order_id),
  executed_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX ON trades(pair, executed_at DESC);

CREATE TABLE deposits (
  id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id          BYTEA NOT NULL REFERENCES users(user_id),
  stealth_addr     TEXT NOT NULL,
  ephemeral_pubkey BYTEA NOT NULL,
  amount           NUMERIC(30,6),
  tx_hash          TEXT,
  chain            TEXT NOT NULL,
  confirmed_at     TIMESTAMPTZ,
  created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX ON deposits(stealth_addr);
CREATE INDEX ON deposits(user_id, confirmed_at);

CREATE TABLE withdrawals (
  id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id    BYTEA NOT NULL REFERENCES users(user_id),
  dest_addr  TEXT NOT NULL,
  amount     NUMERIC(30,6) NOT NULL,
  tx_hash    TEXT,
  chain      TEXT NOT NULL,
  status     TEXT NOT NULL DEFAULT 'pending',
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE chain_config (
  chain_id        TEXT PRIMARY KEY,
  chain_name      TEXT NOT NULL,
  usdt_contract   TEXT,
  rpc_env_key     TEXT NOT NULL,
  confirmations   INT NOT NULL DEFAULT 12,
  enabled         BOOLEAN NOT NULL DEFAULT TRUE
);

INSERT INTO chain_config VALUES
  ('ERC20',  'Ethereum',   '0xdAC17F958D2ee523a2206206994597C13D831ec7', 'ETH_NODE_URL',   12, TRUE),
  ('TRC20',  'Tron',       'TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t',       'TRON_NODE_URL',  20, TRUE),
  ('BEP20',  'BSC',        '0x55d398326f99059fF775485246999027B3197955', 'BSC_NODE_URL',    15, TRUE),
  ('MATIC',  'Polygon',    '0xc2132D05D31c914a87C6611C10748AEb04B58e8F', 'MATIC_NODE_URL',  64, TRUE),
  ('AVAX',   'Avalanche',  '0x9702230A8Ea53601f5cD2dc00fDBc13d4dF4A8c7', 'AVAX_NODE_URL',   64, TRUE),
  ('ARB',    'Arbitrum',   '0xFd086bC7CD5C481DCC9C85ebE478A1C0b69FCbb9', 'ARB_NODE_URL',    64, TRUE),
  ('OP',     'Optimism',   '0x94b008aA00579c1307B0EF2c499aD98a8ce58e58', 'OP_NODE_URL',     64, TRUE),
  ('SOL',    'Solana',     'Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB','SOL_NODE_URL',  32, TRUE);

CREATE TABLE admin_users (
  id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  username     TEXT NOT NULL UNIQUE,
  password_hash TEXT NOT NULL,
  totp_secret  TEXT,
  passkey_cred JSONB,
  created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Row-level security
ALTER TABLE balances ENABLE ROW LEVEL SECURITY;
ALTER TABLE orders   ENABLE ROW LEVEL SECURITY;

-- Grant table access by role
GRANT SELECT, INSERT, UPDATE ON users     TO auth_user;
GRANT SELECT, INSERT, UPDATE ON balances  TO wallet_user;
GRANT SELECT, INSERT, UPDATE ON deposits  TO wallet_user, blockchain_user;
GRANT SELECT, INSERT, UPDATE ON withdrawals TO wallet_user, blockchain_user;
GRANT SELECT ON users                     TO wallet_user, matching_user, settlement_user;
GRANT SELECT, INSERT, UPDATE ON orders    TO matching_user;
GRANT SELECT ON orders                    TO settlement_user;
GRANT SELECT, INSERT ON trades            TO matching_user, settlement_user;
GRANT SELECT, UPDATE ON balances          TO settlement_user;
GRANT SELECT ON chain_config              TO blockchain_user, wallet_user, matching_user;
GRANT SELECT, INSERT, UPDATE, DELETE ON admin_users TO admin_user;
