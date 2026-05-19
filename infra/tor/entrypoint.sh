#!/bin/sh
set -e

ONION_DIR=/var/lib/tor/hidden_service
KEY_DIR=/var/lib/tor/vanity_key

# If we don't have a key yet, generate a vanity one
if [ ! -f "$ONION_DIR/hs_ed25519_secret_key" ]; then
  echo "=== Generating vanity .onion address (prefix: torex) ==="
  echo "=== This may take several minutes... ==="

  mkdir -p "$KEY_DIR"

  # mkp224o must be installed in the image; run with prefix 'torex'
  /usr/local/bin/mkp224o -d "$KEY_DIR" -n 1 -t 4 torex

  # mkp224o creates a subdirectory named <onionaddr>.onion
  ONION_SUBDIR=$(ls "$KEY_DIR" | head -1)
  ONION_ADDR="$ONION_SUBDIR"

  mkdir -p "$ONION_DIR"
  cp "$KEY_DIR/$ONION_SUBDIR/hs_ed25519_secret_key" "$ONION_DIR/hs_ed25519_secret_key"
  cp "$KEY_DIR/$ONION_SUBDIR/hs_ed25519_public_key" "$ONION_DIR/hs_ed25519_public_key"
  echo "$ONION_ADDR" > "$ONION_DIR/hostname"

  echo ""
  echo "╔══════════════════════════════════════════════════════╗"
  echo "║  TorEx .onion address: ${ONION_ADDR}  ║"
  echo "╚══════════════════════════════════════════════════════╝"
  echo ""
else
  ONION_ADDR=$(cat "$ONION_DIR/hostname")
  echo "=== Using existing .onion address: ${ONION_ADDR} ==="
fi

exec tor -f /etc/tor/torrc
