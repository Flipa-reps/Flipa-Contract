# Cash-Out Validation Flow — Notes & Output

## Branch
`feature/cash-out-validation`

## Closes
#122

## What was implemented

### `cash_out(env, player) -> Result<i128, Error>`

Added to `impl CoinflipContract` in `contract/src/lib.rs`.

Returns the net payout in stroops on success.

### Eligibility rules (preconditions, in order)

| # | Guard | Error | Condition |
|---|-------|-------|-----------|
| 1 | Player has a game record | `NoActiveGame` | No `PlayerGame` entry in storage |
| 2 | Game is in `Revealed` phase | `InvalidPhase` | Phase is `Committed` or `Completed` |
| 3 | Last reveal was a win | `NoWinningsToClaimOrContinue` | `streak == 0` after reveal |

All guards execute **before** any state mutation — failed calls leave storage unchanged.

### Payout formula

```
gross      = wager × get_multiplier(streak) / 10_000
fee        = gross × fee_bps / 10_000
net_payout = gross − fee
```

### State transitions on success

```
game.phase          → Completed
stats.total_fees    += fee
stats.reserve_balance -= net_payout
```

Token transfer: `token::Client::transfer(contract → player, net_payout)`

---

## Tests added (12 unit tests in `mod tests`)

| Test | What it covers |
|------|----------------|
| `test_cash_out_rejects_no_active_game` | Guard 1: no game record |
| `test_cash_out_rejects_committed_phase` | Guard 2: Committed phase |
| `test_cash_out_rejects_completed_phase` | Guard 2: Completed phase |
| `test_cash_out_rejects_losing_state_streak_zero` | Guard 3: streak == 0 |
| `test_cash_out_succeeds_streak_1` | Happy path, 1.9x, verifies net + fee + phase + stats |
| `test_cash_out_succeeds_streak_2` | Happy path, 3.5x |
| `test_cash_out_succeeds_streak_4_plus` | Happy path, 10x cap |
| `test_cash_out_allows_new_game_after_completion` | Completed game unblocks start_game |
| `test_cash_out_no_state_mutation_on_invalid_phase` | Atomicity: no partial write on phase error |
| `test_cash_out_no_state_mutation_on_losing_state` | Atomicity: no fee/reserve mutation on loss |

### Payout arithmetic spot-checks

| Streak | Wager | Multiplier | Gross | Fee (3%) | Net |
|--------|-------|-----------|-------|----------|-----|
| 1 | 10,000,000 | 1.9x | 19,000,000 | 570,000 | 18,430,000 |
| 2 | 5,000,000 | 3.5x | 17,500,000 | 525,000 | 16,975,000 |
| 4 | 1,000,000 | 10x | 10,000,000 | 300,000 | 9,700,000 |

---

## Bug fixes included

- `verify_commitment`: `Hash<32>` → `BytesN<32>` via `.into()` (soroban-sdk 22.0.11)
- `start_game`: `contract_random` field type annotation added
- `dummy_commitment` / `dummy_commitment_prop` test helpers: `.into()` added
- `test_verify_commitment`: commitment binding type annotated

---

## cargo check output

```
warning: associated function `delete_player_game` is never used   (pre-existing)
warning: hiding a lifetime that's elided elsewhere                 (pre-existing)
Finished `dev` profile [unoptimized + debuginfo] target(s) in 11.13s
```

Zero errors. Zero new warnings.

---

## Running the tests

```bash
cargo test --lib tests::test_cash_out
cargo test  # full suite
```
