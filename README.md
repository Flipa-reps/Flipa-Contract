# Tossd

Soroban smart contract and frontend for Tossd, a provably fair coinflip game on Stellar with a streak-based payout system.

## Current Status

This repository is in the early implementation stage.

Implemented today:
- Core Soroban contract scaffold
- Contract error enum
- Core data types for game/config/stats
- Persistent storage keys and helper functions
- Contract initialization with fee and wager validation
- Basic unit tests
- Basic property tests for config and stats storage
- Timeout-based wager reclaim (forfeiture recovery)

Not implemented yet:
- Multiplier and payout logic
- Commit-reveal verification and outcome generation
- Game lifecycle functions (`start_game`, `reveal`, `cash_out`, `continue_game`)
- Admin update functions
- Query functions
- Integration flow tests
- Frontend and backend integration

The source of truth for planned work is:
- [`tasks.md`](../.Tossd/specs/soroban-coinflip-game/tasks.md)
- [`requirements.md`](../.Tossd/specs/soroban-coinflip-game/requirements.md)
- [`design.md`](../.Tossd/specs/soroban-coinflip-game/design.md)

## Project Layout

```text
Tossd-contract/
├── contract/
│   ├── Cargo.toml
│   └── src/
│       └── lib.rs
└── frontend/
    ├── package.json
    └── components/
```

## Contract Build

```bash
cargo build --manifest-path contract/Cargo.toml
```

For a release build:

```bash
cargo build --manifest-path contract/Cargo.toml --target wasm32-unknown-unknown --release
```

## Contract Test

```bash
cargo test --manifest-path contract/Cargo.toml
```

Current test coverage is limited to setup, initialization, and early storage behavior. It does not yet cover the full game flow.

## Frontend Start

```bash
npm --prefix frontend start
```

## Implemented Contract Surface

The current contract exposes:
- `initialize`

Internal helpers currently exist for:
- config storage
- stats storage
- player game storage

## Roadmap Summary

The next major milestones are:
1. Finish storage round-trip coverage
2. Add multiplier and payout logic
3. Implement commit-reveal randomness
4. Build the full game flow
5. Add admin and query functions
6. Add timeout recovery and safety checks
7. Expand unit, property, and integration test coverage
8. Prepare deployment and product integration

## Contributor Backlog

Issue drafts for contributors are tracked in:
- [`contributor-issues.md`](../.Tossd/specs/soroban-coinflip-game/contributor-issues.md)

## Notes

- The README intentionally reflects the code that exists today.
- If product behavior and code disagree, use the spec documents as the implementation target and the contract source as the implementation status.
