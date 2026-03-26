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
# Optimize the WASM binary
cargo build --target wasm32-unknown-unknown --release

# The optimized WASM will be at:
# target/wasm32-unknown-unknown/release/coinflip_contract.wasm
```

### Deploy to Stellar

```bash
# Deploy using Stellar CLI
stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/coinflip_contract.wasm \
  --source <YOUR_SECRET_KEY> \
  --network testnet

# Initialize the contract
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <ADMIN_SECRET_KEY> \
  --network testnet \
  -- initialize \
  --admin <ADMIN_ADDRESS> \
  --treasury <TREASURY_ADDRESS> \
  --fee_bps 300 \
  --min_wager 1000000 \
  --max_wager 100000000
```

### Recommended Mainnet Parameters

- **Fee**: 300-500 basis points (3-5%)
- **Min Wager**: 1,000,000 stroops (0.1 XLM)
- **Max Wager**: 100,000,000 stroops (10 XLM)
- **Initial Reserves**: 10x max wager × max multiplier

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
