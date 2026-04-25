#!/usr/bin/env bash
# scripts/deploy-testnet.sh
#
# Builds the Tossd Soroban contract and deploys it to Stellar testnet.
# Outputs the deployed CONTRACT_ID for use in integration tests.
#
# Prerequisites:
#   - Rust + wasm32-unknown-unknown target  (rustup target add wasm32-unknown-unknown)
#   - stellar CLI  (cargo install --locked stellar-cli --features opt)
#   - A funded testnet keypair in ~/.config/stellar/identity/<IDENTITY>.toml
#     or passed via TESTNET_SECRET_KEY env var
#
# Usage:
#   IDENTITY=mykey ./scripts/deploy-testnet.sh
#   # or
#   TESTNET_SECRET_KEY=S... ./scripts/deploy-testnet.sh

set -euo pipefail

IDENTITY="${IDENTITY:-tossd-deployer}"
NETWORK="testnet"
MANIFEST="contract/Cargo.toml"
WASM_PATH="contract/target/wasm32-unknown-unknown/release/tossd_contract.wasm"

# ── 1. Build ──────────────────────────────────────────────────────────────────
echo "==> Building contract (release)…"
cargo build --manifest-path "$MANIFEST" \
  --target wasm32-unknown-unknown \
  --release

# ── 2. Optimise (optional but recommended) ────────────────────────────────────
if command -v stellar &>/dev/null; then
  echo "==> Optimising WASM…"
  stellar contract optimize --wasm "$WASM_PATH" || true
fi

# ── 3. Deploy ─────────────────────────────────────────────────────────────────
echo "==> Deploying to $NETWORK…"

if [[ -n "${TESTNET_SECRET_KEY:-}" ]]; then
  # CI path: use secret key directly
  CONTRACT_ID=$(stellar contract deploy \
    --wasm "$WASM_PATH" \
    --source-account "$TESTNET_SECRET_KEY" \
    --network "$NETWORK")
else
  # Local path: use named identity
  CONTRACT_ID=$(stellar contract deploy \
    --wasm "$WASM_PATH" \
    --source-account "$IDENTITY" \
    --network "$NETWORK")
fi

echo ""
echo "✅  Contract deployed: $CONTRACT_ID"
echo ""
echo "Export for integration tests:"
echo "  export TESTNET_CONTRACT_ID=$CONTRACT_ID"
echo "  export TESTNET_SECRET_KEY=<your-funded-secret>"
echo ""
echo "Run integration tests:"
echo "  TESTNET_CONTRACT_ID=$CONTRACT_ID TESTNET_SECRET_KEY=\$TESTNET_SECRET_KEY \\"
echo "    npx vitest run frontend/tests/integration/testnet.integration.test.ts"
