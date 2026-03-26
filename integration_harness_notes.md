# Integration Harness Notes

Branch: `feature/integration-harness`
Commit: `test: add integration harness for core game flows`

## What was added

A new `integration_tests` module appended to `contract/src/lib.rs`.

### Harness (`Harness` struct)

| Helper | Purpose |
|---|---|
| `Harness::new()` | Registers contract, mocks all auths, initialises with default config |
| `player()` | Generates a fresh deterministic player address |
| `make_secret(seed)` | Returns `[seed; 32]` bytes — deterministic, reviewable |
| `make_commitment(seed)` | SHA-256 of `make_secret(seed)` |
| `fund(amount)` | Sets `reserve_balance` directly in storage |
| `inject_game(...)` | Writes a `GameState` at any phase/streak, bypassing `start_game` |
| `stats()` | Reads current `ContractStats` |
| `game_state(player)` | Reads current `GameState` for a player |
| `play_round(player, side, wager, seed)` | Full `start_game → reveal` cycle |
| `play_win_round(player, wager)` | Seed 1 + Heads → win |
| `play_loss_round(player, wager)` | Seed 3 + Heads → loss |
| `probe_outcome(seed)` | Predicts outcome at current ledger sequence |

### Seed convention

| seed | outcome | use with `Side::Heads` |
|------|---------|------------------------|
| 1    | Heads   | WIN                    |
| 3    | Tails   | LOSE                   |

Calibrated from `loss_forfeiture_tests`: `[3u8;32]` → sha256[0]=0x64 XOR contract_random[0]=0xdf → bit 1 → Tails.

## Tests added (14 total)

| Test | Covers |
|---|---|
| `test_full_win_then_cash_out` | Happy path: start → reveal win → cash_out, payout math, stats |
| `test_full_loss_forfeits_wager_to_reserves` | Loss path: reveal false, game deleted, wager credited to reserves |
| `test_win_continue_win_cash_out_streak_2` | Streak 2 multiplier, continue_streak phase reset |
| `test_streak_4_uses_max_multiplier` | Streak 4+ cap (10x), four consecutive wins |
| `test_paused_contract_rejects_start_game` | ContractPaused guard, no state mutation |
| `test_double_start_rejected_while_game_active` | ActiveGameExists guard, original state intact |
| `test_reveal_wrong_secret_rejected` | CommitmentMismatch guard, phase unchanged |
| `test_start_game_rejected_when_reserves_insufficient` | InsufficientReserves guard |
| `test_new_game_allowed_after_completion` | Post-completion re-entry, streak resets to 0 |
| `test_stats_accumulate_across_multiple_players` | total_games and total_volume across two players |
| `test_wager_boundary_inclusive` | min_wager and max_wager are inclusive bounds |
| `test_cash_out_rejects_zero_streak_revealed` | NoWinningsToClaimOrContinue guard |
| `test_continue_streak_rejects_committed_phase` | InvalidPhase guard on continue_streak |
| `test_probe_outcome_matches_reveal` | Validates harness probe_outcome helper against actual reveal |

## Test output

```
running 14 tests
test integration_tests::test_continue_streak_rejects_committed_phase ... ok
test integration_tests::test_cash_out_rejects_zero_streak_revealed ... ok
test integration_tests::test_double_start_rejected_while_game_active ... ok
test integration_tests::test_full_loss_forfeits_wager_to_reserves ... ok
test integration_tests::test_new_game_allowed_after_completion ... ok
test integration_tests::test_full_win_then_cash_out ... ok
test integration_tests::test_paused_contract_rejects_start_game ... ok
test integration_tests::test_reveal_wrong_secret_rejected ... ok
test integration_tests::test_probe_outcome_matches_reveal ... ok
test integration_tests::test_start_game_rejected_when_reserves_insufficient ... ok
test integration_tests::test_stats_accumulate_across_multiple_players ... ok
test integration_tests::test_wager_boundary_inclusive ... ok
test integration_tests::test_win_continue_win_cash_out_streak_2 ... ok
test integration_tests::test_streak_4_uses_max_multiplier ... ok

test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 122 filtered out
```

Full suite: **132 passed; 0 failed; 4 ignored** (the 4 ignored are pre-existing token-transfer tests requiring a deployed SAC).
