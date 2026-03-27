# Deployment Notes — feature/deployment-guide
Closes #159

## Changes

- `contract/deploy.sh` — shell script that builds the WASM, deploys to testnet or
  mainnet, and calls `initialize` with configurable parameters via env vars.
- `README.md` — expanded Deployment section with:
  - Automated script usage
  - Manual step-by-step commands
  - Recommended mainnet parameter table
  - Mainnet rollout checklist (pre-deploy, deploy, post-deploy)
  - Security assumptions for mainnet (7 documented invariants)

## Test output

```
cargo test
test result: ok. 132 passed; 0 failed; 4 ignored
```

The 4 ignored tests require a deployed SAC token and are intentionally skipped
in the local environment. All other tests pass.

## Security assumptions documented

1. Commit-reveal integrity — neither party can bias outcomes unilaterally
2. Admin key compromise scope — fees/params only, not player funds
3. Treasury key compromise scope — accumulated fees only
4. Reserve solvency — protocol-enforced before every game start
5. Fee snapshot isolation — in-flight games unaffected by admin fee changes
6. Overflow safety — checked arithmetic throughout, wager guards prevent overflow path
7. Timeout recovery — abandoned games can be reclaimed after timeout window

## Deployment script parameters

| Env var            | Default       | Description                  |
| ------------------ | ------------- | ---------------------------- |
| `ADMIN_SECRET`     | (required)    | Admin Stellar secret key     |
| `TREASURY_ADDRESS` | (required)    | Treasury public key          |
| `FEE_BPS`          | `300`         | Protocol rake in basis points|
| `MIN_WAGER`        | `1000000`     | Minimum wager in stroops     |
| `MAX_WAGER`        | `100000000`   | Maximum wager in stroops     |
