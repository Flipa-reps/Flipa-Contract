# Tossd - Provably Fair Coinflip Game on Stellar

A production-ready, trustless coinflip game built on Stellar's Soroban smart contract platform with a unique "Double-or-Nothing Streak" mechanic.

## 🎯 Overview

Tossd is a decentralized gambling application that implements a provably fair coinflip game where players can wager XLM on heads or tails. Winners can choose to cash out their winnings or risk them for exponentially higher multipliers through consecutive wins.

### Key Features

- **Provably Fair Randomness**: Commit-reveal pattern ensures neither player nor contract can manipulate outcomes
- **Streak Multiplier System**: Exponential payouts for consecutive wins (1.9x → 3.5x → 6x → 10x)
- **Cash-Out Anytime**: Players can secure profits after any win
- **Transparent Protocol Fees**: 2-5% configurable rake on winnings
- **Secure Fund Management**: All wagers held in contract custody with reserve solvency checks
- **Property-Based Testing**: Comprehensive test coverage with 30+ correctness properties
- **Emergency Pause Control**: Admin can pause only new game creation during incidents

### Pause Behavior and Scope

The contract includes an admin-only pause switch (`set_paused`) for emergency response.

- `set_paused(true)` blocks only `start_game` calls.
- In-flight games can still settle while paused (`reveal`, `continue_streak`, `cash_out`, `claim_winnings`).
- `set_paused(false)` re-enables new game creation.
- Unauthorized callers are rejected with `Unauthorized`.

## 🎮 Game Mechanics

### Multiplier Structure

| Streak  | Multiplier | House Edge |
| ------- | ---------- | ---------- |
| 1st win | 1.9x       | ~5%        |
| 2nd win | 3.5x       | ~6.25%     |
| 3rd win | 6.0x       | ~6.25%     |
| 4+ wins | 10.0x      | ~6.25%     |

### How to Play

1. **Start Game**: Choose heads or tails, submit your wager with a commitment hash
2. **Reveal**: Reveal your random value to determine the outcome
3. **Win**: If you win, choose to:
   - **Cash Out**: Claim your winnings (wager × multiplier - fee)
   - **Continue**: Risk your winnings for the next multiplier level
4. **Lose**: All winnings are forfeited, game ends

## 🚀 Getting Started

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (latest stable)
- [Stellar CLI](https://developers.stellar.org/docs/tools/developer-tools) for Soroban
- Cargo (comes with Rust)

### Installation

```bash
# Clone the repository
git clone https://github.com/Tossd-Org/Tossd.git
cd Tossd/coinflip-contract

# Build the contract
cargo build --target wasm32-unknown-unknown --release

# Run tests
cargo test
```

### Running Tests

```bash
# Run all tests
cargo test

# Run only unit tests
cargo test --lib tests::

# Run only property-based tests
cargo test --lib property_tests::

# Run with verbose output
cargo test -- --nocapture
```

## 📦 Deployment

### Build for Production

```bash
cargo build --target wasm32-unknown-unknown --release
# Output: target/wasm32-unknown-unknown/release/coinflip_contract.wasm
```

### Automated Deployment Script

A deployment script is provided at `contract/deploy.sh`. It builds the WASM,
deploys the contract, and calls `initialize` in one step.

```bash
# Set credentials — never commit these
export ADMIN_SECRET="S..."          # admin Stellar secret key
export TREASURY_ADDRESS="G..."      # treasury public key (must differ from admin)

# Optional overrides (defaults shown)
export FEE_BPS=300                  # 3% rake
export MIN_WAGER=1000000            # 0.1 XLM
export MAX_WAGER=100000000          # 10 XLM

# Deploy to testnet
./contract/deploy.sh testnet

# Deploy to mainnet
./contract/deploy.sh mainnet
```

The script prints the contract ID and a reserve-funding reminder on success.

### Manual Deployment (step-by-step)

```bash
# 1. Deploy WASM
stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/coinflip_contract.wasm \
  --source <ADMIN_SECRET_KEY> \
  --network mainnet

# 2. Initialize
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <ADMIN_SECRET_KEY> \
  --network mainnet \
  -- initialize \
  --admin <ADMIN_ADDRESS> \
  --treasury <TREASURY_ADDRESS> \
  --fee_bps 300 \
  --min_wager 1000000 \
  --max_wager 100000000

# 3. Fund reserves (minimum: MAX_WAGER × 10 = 1,000,000,000 stroops for defaults)
# Transfer XLM to the contract address via the Stellar network before opening to players.
```

### Recommended Mainnet Parameters

| Parameter      | Recommended Value          | Notes                                      |
| -------------- | -------------------------- | ------------------------------------------ |
| `fee_bps`      | 300–500 (3–5%)             | Must be in range enforced by contract      |
| `min_wager`    | 1,000,000 stroops (0.1 XLM)| Prevents dust spam                         |
| `max_wager`    | 100,000,000 stroops (10 XLM)| Cap exposure per game                     |
| Initial reserve| ≥ max_wager × 10           | Covers worst-case 10x streak-4+ payout     |

### Mainnet Rollout Checklist

Pre-deploy:
- [ ] Run full test suite: `cargo test` → `132 passed; 0 failed; 4 ignored`
- [ ] Build succeeds with zero errors and zero new warnings
- [ ] Admin and treasury keys are separate accounts (contract rejects same address)
- [ ] Admin key is a hardware wallet or multisig — never a hot key
- [ ] Treasury address is a cold wallet or protocol-controlled account
- [ ] `ADMIN_SECRET` is stored in a secrets manager, not in shell history or `.env` files

Deploy:
- [ ] Deploy to testnet first and run a full game flow end-to-end
- [ ] Verify contract ID matches expected WASM hash
- [ ] Confirm `initialize` parameters match intended values via `get_stats()`
- [ ] Fund contract reserve to at least `max_wager × 10` before opening to players
- [ ] Verify `reserve_balance` via `get_stats()` after funding

Post-deploy:
- [ ] Monitor `reserve_balance` — top up before it falls below `max_wager × 10`
- [ ] Set up alerting on `ContractPaused` / `InsufficientReserves` errors
- [ ] Document the deployed contract ID and block height in your ops runbook
- [ ] Test `set_paused(true)` and `set_paused(false)` from the admin key

### Security Assumptions for Mainnet

1. Commit-reveal integrity — The contract cannot manipulate outcomes because the
   player's secret is unknown until `reveal`. The player cannot manipulate outcomes
   because the commitment is locked in `start_game`. Neither party can bias the
   XOR-based outcome without controlling both secrets simultaneously.

2. Admin key compromise — A compromised admin key can pause new games and change
   fee/wager parameters, but cannot steal player funds or alter in-flight game
   state. Rotate the admin key immediately if compromise is suspected using
   `set_treasury` / `set_wager_limits` / `set_fee` from a new admin address
   (requires re-initialization if the admin address itself must change).

3. Treasury key compromise — A compromised treasury key exposes accumulated fees
   only. Player wagers and reserves are held by the contract, not the treasury.
   Update the treasury address via `set_treasury` from the admin key.

4. Reserve solvency — The contract enforces a solvency check before every
   `start_game` and `continue_streak`. If reserves fall below the worst-case
   payout for the current `max_wager` at streak 4+ (10x multiplier), new games
   are rejected with `InsufficientReserves`. This is a protocol-level guarantee,
   not an operational one — keep reserves funded.

5. Fee snapshot isolation — The `fee_bps` stored in each `GameState` at
   `start_game` time is immutable for that game. Admin fee changes via `set_fee`
   only affect games started after the change. In-flight games settle at the
   fee rate they were opened with.

6. Overflow safety — All arithmetic uses `checked_*` operations. `calculate_payout`
   returns `None` for wagers above `i128::MAX / 100_000`, which the wager
   validation guards prevent from ever reaching settlement.

7. Timeout recovery — If a player abandons a game after `start_game` without
   calling `reveal`, the `RevealTimeout` path allows the wager to be reclaimed
   after the timeout window. Ensure the timeout window is appropriate for your
   expected block times.

## 🧪 Testing Strategy

The project employs a dual testing approach:

### Unit Tests

- Specific examples and edge cases
- Error condition validation
- State transition verification
- Boundary value testing

### Property-Based Tests

- 100+ iterations per property
- Randomized input generation
- Universal correctness validation
- 30 correctness properties covering all requirements

## ✅ Task 5 Checkpoint — Test Matrix

All 43 tests must pass before proceeding to game-flow implementation.

```bash
cargo test                                      # run full suite (43 tests)
cargo test --lib tests::                        # unit tests (15)
cargo test --lib property_tests::               # core property tests (13)
cargo test --lib outcome_determinism_tests::    # determinism tests (6)
cargo test --lib randomness_regression_tests::  # randomness regression tests (5)
```

| Module                        | Count  | What it covers                                                                       |
| ----------------------------- | ------ | ------------------------------------------------------------------------------------ |
| `tests`                       | 15     | Multipliers, payout arithmetic, initialization, error codes, enums                   |
| `property_tests`              | 13     | Payout correctness, multiplier monotonicity, commitment verification, config storage |
| `outcome_determinism_tests`   | 6      | Identical inputs → identical outputs for all helpers                                 |
| `randomness_regression_tests` | 5      | Commit-reveal unilateral control paths                                               |
| **Total**                     | **43** |                                                                                      |

### Expected output

```
test result: ok. 43 passed; 0 failed; 0 ignored
```

Any failure at this checkpoint indicates a regression in core logic and must be resolved before game-flow work continues.

---

## ✅ Task 18 — Final Verification Checklist

This checklist must be completed in full before merging any PR that touches game-flow logic, settlement, or the test suite.

### 1. Build

```bash
cargo build --target wasm32-unknown-unknown --release
```

- [ ] Build completes with zero errors
- [ ] Zero new warnings introduced (pre-existing warnings are documented and acceptable)
- [ ] `target/wasm32-unknown-unknown/release/coinflip_contract.wasm` is produced

### 2. Full Test Suite

Run the complete suite and confirm the expected totals:

```bash
cargo test
```

Expected output:

```
test result: ok. 132 passed; 0 failed; 4 ignored
```

The 4 ignored tests require a deployed SAC token and are intentionally skipped in the local environment.

### 3. Module-Level Test Commands

Each module can be run in isolation to pinpoint regressions:

```bash
cargo test --lib tests::                          # unit tests
cargo test --lib property_tests::                 # core property + wager boundary tests
cargo test --lib streak_increment_tests::         # streak increment invariants
cargo test --lib outcome_determinism_tests::      # pure helper determinism
cargo test --lib randomness_regression_tests::    # commit-reveal security
cargo test --lib loss_forfeiture_tests::          # loss path accounting
cargo test --lib integration_tests::              # end-to-end game flows
```

### 4. Test Suite Breakdown

| Module                        | Count   | What it covers                                                                 |
| ----------------------------- | ------- | ------------------------------------------------------------------------------ |
| `tests`                       | 57      | Unit tests: multipliers, payout math, init, error codes, reveal, cash_out      |
| `property_tests`              | 25      | Payout correctness, wager boundaries, multiplier monotonicity, config storage  |
| `streak_increment_tests`      | 11      | Streak +1 invariant, monotonicity, tier transitions, payout ordering           |
| `outcome_determinism_tests`   | 6       | Identical inputs → identical outputs for all pure helpers                      |
| `randomness_regression_tests` | 5       | Commit-reveal: round-trip, distinct secrets, tamper resistance                 |
| `loss_forfeiture_tests`       | 5       | Loss returns false, state deleted, reserve credited, slot freed, side-agnostic |
| `integration_tests`           | 14      | Full game flows: win/loss/streak/pause/guards/stats/boundary                   |
| **Total**                     | **123** | (132 with transfer tests; 4 ignored require deployed SAC)                      |

### 5. Invariant Coverage

Confirm each invariant class has passing tests before merge:

**Wager Validation**
- [ ] `wager < min_wager` → `WagerBelowMinimum` (off-by-one: `min - 1` rejected, `min` accepted)
- [ ] `wager > max_wager` → `WagerAboveMaximum` (off-by-one: `max + 1` rejected, `max` accepted)
- [ ] Guards execute before any state mutation — no partial writes on rejection

**Commit-Reveal Security**
- [ ] Wrong secret → `CommitmentMismatch`, phase unchanged
- [ ] Distinct secrets produce distinct commitments
- [ ] Tampered commitment or tampered secret both fail verification
- [ ] Verification is not symmetric (hash(A) ≠ hash(B) even if A ≈ B)

**Streak Mechanics**
- [ ] Fresh game always starts at `streak = 0`
- [ ] Each win increments streak by exactly 1 (never 0, never 2+)
- [ ] Streak progression is strictly monotonic
- [ ] No multiplier tier is skipped (1 → 2 → 3 → 4 in single steps)
- [ ] Multiplier caps at 10x for streak ≥ 4; counter continues without reset
- [ ] Payout at streak N+1 is strictly greater than payout at streak N

**Loss Forfeiture**
- [ ] `reveal` returns `Ok(false)` on any loss
- [ ] Player game state is fully deleted from storage after a loss
- [ ] `reserve_balance` increases by exactly the forfeited wager
- [ ] Player slot is freed immediately — new `start_game` succeeds without cleanup
- [ ] New game after loss starts with `streak = 0` (no carry-over)
- [ ] Forfeiture semantics are identical for Heads and Tails losses
- [ ] Reserve overflow near `i128::MAX` is handled safely (no wrap or panic)

**Settlement Accounting**
- [ ] `gross = wager × multiplier_bps / 10_000`
- [ ] `fee = gross × fee_bps / 10_000`
- [ ] `net = gross − fee`
- [ ] Contract balance decreases by exactly `gross`
- [ ] Treasury balance increases by exactly `fee`
- [ ] Player balance increases by exactly `net`
- [ ] `continue_streak` involves zero token transfers
- [ ] Reserve solvency check fires before any transfer

**Reserve Solvency**
- [ ] `start_game` rejected when `reserve_balance < worst_case_payout` (streak 4+ multiplier)
- [ ] `continue_streak` rejected when reserves are insufficient
- [ ] Reserve balance never goes negative

**Admin Controls**
- [ ] `set_paused(true)` blocks `start_game`; in-flight games still settle
- [ ] `set_paused(false)` re-enables new game creation
- [ ] Unauthorized callers rejected with `Unauthorized`
- [ ] `fee_bps` snapshot in `GameState` isolates in-flight games from admin fee changes

**Overflow Safety**
- [ ] `calculate_payout` returns `None` for wagers above `i128::MAX / 100_000`
- [ ] All arithmetic uses `checked_*` operations — no silent wraps

### 6. Error Code Stability

Confirm no error discriminant values have changed (breaking protocol change):

| Code | Variant                      |
| ---- | ---------------------------- |
| 1    | `WagerBelowMinimum`          |
| 2    | `WagerAboveMaximum`          |
| 3    | `ActiveGameExists`           |
| 4    | `InsufficientReserves`       |
| 5    | `ContractPaused`             |
| 10   | `NoActiveGame`               |
| 11   | `InvalidPhase`               |
| 12   | `CommitmentMismatch`         |
| 13   | `RevealTimeout`              |
| 20   | `NoWinningsToClaimOrContinue`|
| 21   | `InvalidCommitment`          |
| 30   | `Unauthorized`               |
| 31   | `InvalidFeePercentage`       |
| 32   | `InvalidWagerLimits`         |
| 40   | `TransferFailed`             |
| 50   | `AdminTreasuryConflict`      |
| 51   | `AlreadyInitialized`         |

- [ ] All 17 variants present with correct `u32` discriminants
- [ ] No variant has been renumbered or removed

### 7. Documentation

- [ ] All public API functions have `///` doc comments
- [ ] All `Error` variants have doc comments referencing their error code constant
- [ ] `GamePhase` state-transition diagram is accurate
- [ ] `CoinflipContract` public API table is up to date
- [ ] `error_codes` module table matches the `Error` enum

### 8. Pre-Merge Sign-Off

- [ ] `cargo test` passes with 0 failures
- [ ] Zero new compiler warnings introduced
- [ ] PR targets the correct branch (`feature/final-verification-checklist` → `main` or `develop`)
- [ ] Commit message follows convention: `docs: add final verification checklist for comprehensive testing`
- [ ] All checklist items above are checked

## 🔒 Security Features

1. **Commit-Reveal Pattern**: Prevents outcome manipulation by either party
2. **Checked Arithmetic**: All calculations use overflow-safe operations
3. **Access Control**: Admin functions restricted to authorized addresses
4. **Reserve Solvency**: Contract rejects games if reserves are insufficient
5. **Atomic Operations**: State changes and transfers succeed or fail together
6. **Timeout Recovery**: Players can reclaim wagers if reveal times out

## 📊 Contract Statistics

The contract tracks:

- Total games played
- Total volume wagered
- Total fees collected
- Current reserve balance

Query these stats using the `get_stats()` function.

## 🛠️ Development

### Project Structure

```
coinflip-contract/
├── src/
│   └── lib.rs              # Main contract implementation
├── Cargo.toml              # Dependencies and build config
├── .gitignore              # Git ignore rules
└── README.md               # This file
```

### Contributing

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## 📝 License

This project is licensed under the MIT License.

## 🤝 Support

- **Issues**: [GitHub Issues](https://github.com/Tossd-Org/Tossd/issues)
- **Documentation**: [Stellar Soroban Docs](https://developers.stellar.org/docs/smart-contracts)

## 🎯 Roadmap

- [x] Core contract implementation
- [x] Commit-reveal randomness
- [x] Streak multiplier system
- [x] Property-based testing
- [ ] Frontend integration
- [ ] Mainnet deployment
- [ ] Tournament mode
- [ ] NFT cosmetics

## ⚠️ Disclaimer

This is a gambling application. Please gamble responsibly. The house edge is built into the multipliers, and the protocol collects fees on winnings. Never wager more than you can afford to lose.

---

**Made for Stellar Blockchain** | **Powered by Soroban Smart Contracts**

## 🎨 Frontend Brand System

- Visual system guide: `frontend/DESIGN.md`
- Design tokens (JSON): `frontend/tokens/tossd.tokens.json`
- Design tokens (CSS): `frontend/tokens/tossd.tokens.css`
- Usage examples: `frontend/examples/brand-system-examples.md`

## 📐 Landing Page Screen Specs

- High-fidelity desktop + mobile spec: `frontend/LANDING_SCREENS.md`
