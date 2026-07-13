# Testnet Integration Tests

Tests in this directory run against a **real deployed Flipa contract** on Stellar testnet. They are skipped automatically in CI when the required environment variables are absent.

## Prerequisites

| Tool | Install |
|------|---------|
| Rust + `wasm32` target | `rustup target add wasm32-unknown-unknown` |
| stellar CLI | `cargo install --locked stellar-cli --features opt` |
| Funded testnet account | `stellar keys generate --network testnet mykey && stellar keys fund mykey --network testnet` |

## Deploy the contract

```bash
# From the repo root
IDENTITY=mykey ./scripts/deploy-testnet.sh
```

The script prints the deployed `CONTRACT_ID`. Copy it for the next step.

## Run the integration tests

```bash
export TESTNET_CONTRACT_ID=C...   # from deploy step above
export TESTNET_SECRET_KEY=S...    # funded testnet keypair secret
# optional — defaults to https://soroban-testnet.stellar.org
export TESTNET_RPC_URL=https://soroban-testnet.stellar.org

npx vitest run frontend/tests/integration/testnet.integration.test.ts
```

## Environment variables

| Variable | Required | Description |
|----------|----------|-------------|
| `TESTNET_CONTRACT_ID` | Yes | Deployed contract address (`C...`) |
| `TESTNET_SECRET_KEY` | Yes | Funded testnet keypair secret (`S...`) |
| `TESTNET_RPC_URL` | No | Soroban RPC endpoint (default: `https://soroban-testnet.stellar.org`) |

When either required variable is absent the entire suite is skipped via `describe.skip` — no test failures, no noise in CI.

## What is tested

| # | Test | Description |
|---|------|-------------|
| 1 | Connectivity | Account is funded and reachable on testnet |
| 2 | `start_game` | Submits a transaction, returns a 64-char hex tx hash |
| 3 | `reveal` | Full start → reveal flow; outcome is `win` or `loss` |
| 4 | `cash_out` | Payout after a win exceeds the original wager |
| 5 | `continue_streak` | Extends a winning streak, returns a tx hash |
| 6 | Network errors | Bad RPC URL surfaces a clear thrown error |
| 7 | Invalid commitment | Wrong secret is rejected by the contract |

## Notes

- Each test creates its own game to stay independent.
- `cash_out` and `continue_streak` retry up to 5 times to land a win; if all attempts lose they log a warning and pass (non-deterministic outcome is expected).
- Timeouts are generous (30–120 s) to account for testnet ledger time (~5 s/ledger).
- Never use mainnet credentials here. The `TESTNET_SECRET_KEY` is only ever used against `testnet` network passphrase.
