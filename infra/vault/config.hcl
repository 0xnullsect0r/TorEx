storage "inmem" {}

listener "tcp" {
  address     = "0.0.0.0:8200"
  tls_disable = true
}

api_addr = "http://vault:8200"
ui       = false

# Secret paths used by TorEx:
# secret/torex/hot_wallet_eth_key   - ERC-20 hot wallet private key (hex)
# secret/torex/hot_wallet_tron_key  - TRC-20 hot wallet private key (hex)
# secret/torex/balance_enc_key      - AES-256 key for balance encryption (32 bytes hex)
# secret/torex/noise_static_key     - Noise protocol static keypair (hex)
