#!/usr/bin/env bash
# deploy.sh — Tossd coinflip contract deployment script
# Usage: ./deploy.sh [testnet|mainnet]
#
# Prerequisites:
#   - Rust + wasm32-unknown-unknown target installed
#   - Stellar CLI installed (https://developers.stellar.org/docs/tools/developer-tools)
#   - ADMIN_SECRET and TREASURY_ADDRESS env vars set (never commit these)
#
# Example:
#   export ADMIN_SECRET="S..."
#   export TREASURY_ADDRESS="G..."
#   ./deploy.sh mainnet

set -euo pipefail

NETWORK="${1:-testnet}"
WASM_PATH="target/wasm32-unknown-unknown/release/coinflip_contract.wasm"

# ── Parameter defaults (override via env vars) ────────────────────────────────
FEE_BPS="${FEE_BPS:-300}"                   # 3% rake
MIN_WAGER="${MIN_WAGER:-1000000}"           # 0.1 XLM in stroops
MAX_WAGER="${MAX_WAGER:-100000000}"         # 10 XLM in stroops

# ── Validate required env vars ────────────────────────────────────────────────
if [[ -z "${ADMIN_SECRET:-}" ]]; then
  echo "ERROR: ADMIN_SECRET is not set. Export your admin secret key before running."
  exit 1
fi

if [[ -z "${TREASURY_ADDRESS:-}" ]]; then
  echo "ERROR: TREASURY_ADDRESS is not set. Export the treasury public key before running."
  exit 1
fi

# ── Network RPC endpoints ─────────────────────────────────────────────────────
case "$NETWORK" in
  testnet)
    RPC_URL="https://soroban-testnet.stellar.org"
    NETWORK_PASSPHRASE="Test SDF Network ; September 2015"
    ;;
  mainnet)
    RPC_URL="https://soroban-mainnet.stellar.org"
    NETWORK_PASSPHRASE="Public Global Stellar Network ; September 2015"
    ;;
  *)
    echo "ERROR: Unknown network '$NETWORK'. Use 'testnet' or 'mainnet'."
    exit 1
    ;;
esac

echo "==> Building WASM for $NETWORK..."
cargo build --target wasm32-unknown-unknown --release

echo "==> Deploying contract to $NETWORK..."
CONTRACT_ID=$(stellar contract deploy \
  --wasm "$WASM_PATH" \
  --source "$ADMIN_SECRET" \
  --rpc-url "$RPC_URL" \
  --network-passphrase "$NETWORK_PASSPHRASE" \
  2>&1 | tail -n1)

echo "    Contract ID: $CONTRACT_ID"

# Derive admin address from secret key
ADMIN_ADDRESS=$(stellar keys address "$ADMIN_SECRET" 2>/dev/null || \
  stellar account show --source "$ADMIN_SECRET" --rpc-url "$RPC_URL" \
    --network-passphrase "$NETWORK_PASSPHRASE" | grep "Public Key" | awk '{print $3}')

echo "==> Initializing contract..."
stellar contract invoke \
  --id "$CONTRACT_ID" \
  --source "$ADMIN_SECRET" \
  --rpc-url "$RPC_URL" \
  --network-passphrase "$NETWORK_PASSPHRASE" \
  -- initialize \
  --admin "$ADMIN_ADDRESS" \
  --treasury "$TREASURY_ADDRESS" \
  --fee_bps "$FEE_BPS" \
  --min_wager "$MIN_WAGER" \
  --max_wager "$MAX_WAGER"

echo ""
echo "✓ Deployment complete."
echo "  Network:   $NETWORK"
echo "  Contract:  $CONTRACT_ID"
echo "  Admin:     $ADMIN_ADDRESS"
echo "  Treasury:  $TREASURY_ADDRESS"
echo "  Fee:       ${FEE_BPS} bps"
echo "  Min wager: ${MIN_WAGER} stroops"
echo "  Max wager: ${MAX_WAGER} stroops"
echo ""
echo "Next step: fund the contract reserve before accepting player wagers."
echo "  Minimum recommended reserve = MAX_WAGER × 100_000 (10x multiplier) = $((MAX_WAGER * 10)) stroops"
