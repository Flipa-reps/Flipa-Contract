# Reveal Loss Cleanup — Test Output & Notes

## Branch
`feature/reveal-loss-cleanup`

## What was implemented

### `reveal()` in `contract/src/lib.rs`

Added the `reveal` entry point to `CoinflipContract`. It:

1. Requires player auth.
2. Guards: `NoActiveGame` → `InvalidPhase` → `CommitmentMismatch` (in that order, no state mutation on failure).
3. Derives the outcome by XOR-ing the first byte of the player's secret hash with the first byte of the contract's stored random contribution, then taking the low bit (`0` = Heads, `1` = Tails).
4. **Win path**: increments `streak`, advances phase to `Revealed`, persists updated game state.
5. **Loss path (forfeiture)**:
   - Credits `wager` back to `reserve_balance` (house keeps the bet).
   - Calls `delete_player_game` to remove the player's storage slot entirely.
   - Returns `Ok(false)` — no further action needed from the player.

### Forfeiture semantics

On a loss the wager is **not transferred** — it was already held by the contract when `start_game` was called. The loss path simply credits it to `reserve_balance` so the accounting stays consistent, then wipes the game state. The player's slot is freed immediately, allowing a new game to start without any cleanup step.

## Test output

```
running 57 tests
test property_tests::prop_wager_below_minimum_rejected ... ok
test property_tests::prop_wager_above_maximum_rejected ... ok
test property_tests::prop_wager_at_minimum_boundary_accepted ... ok
test property_tests::test_config_storage_accuracy ... ok
test property_tests::test_multiplier_always_greater_than_1x ... ok
test property_tests::test_multiplier_cap_boundary ... ok
test property_tests::test_multiplier_exact_values_streaks_1_to_3 ... ok
test property_tests::test_multiplier_monotonically_increasing ... ok
test property_tests::test_multiplier_never_exceeds_cap ... ok
test property_tests::test_multiplier_streak_4_plus_is_constant ... ok
test property_tests::test_payout_always_positive ... ok
test property_tests::test_payout_fee_boundaries ... ok
test property_tests::test_payout_increases_with_streak ... ok
test property_tests::test_payout_linear_in_wager ... ok
test property_tests::test_payout_net_less_than_gross ... ok
test property_tests::test_payout_non_negative ... ok
test property_tests::prop_wager_at_maximum_boundary_accepted ... ok
test property_tests::test_distinct_addresses_always_accepted ... ok
test property_tests::test_wager_at_maximum_boundary_explicit ... ok
test property_tests::test_wager_at_minimum_boundary_explicit ... ok
test property_tests::test_wager_exactly_one_above_maximum_rejected ... ok
test property_tests::test_wager_exactly_one_below_minimum_rejected ... ok
test property_tests::test_wager_midpoint_in_bounds_accepted ... ok
test property_tests::test_wager_rejection_independent_of_side_choice ... ok
test property_tests::test_wager_validation_guards_before_state_mutation ... ok
test tests::test_calculate_payout_basic ... ok
test tests::test_calculate_payout_overflow_returns_none ... ok
test tests::test_calculate_payout_streak_4_plus ... ok
test tests::test_calculate_payout_zero_wager ... ok
test tests::test_error_codes_defined ... ok
test tests::test_game_phase_variants ... ok
test tests::test_get_multiplier_streak_0_returns_max ... ok
test tests::test_get_multiplier_streak_4_plus ... ok
test tests::test_get_multiplier_streak_levels ... ok
test tests::test_initialize_contract ... ok
test tests::test_initialize_invalid_fee ... ok
test tests::test_initialize_invalid_wager_limits ... ok
test tests::test_initialize_rejects_reinitialization ... ok
test tests::test_initialize_rejects_same_admin_and_treasury ... ok
test tests::test_reveal_loss_allows_new_game_after ... ok
test tests::test_reveal_loss_clears_game_state ... ok
test tests::test_reveal_loss_credits_wager_to_reserves ... ok
test tests::test_reveal_rejects_no_active_game ... ok
test tests::test_reveal_rejects_wrong_phase ... ok
test tests::test_reveal_rejects_wrong_secret ... ok
test tests::test_reveal_win_increments_streak_and_advances_phase ... ok
test tests::test_side_enum_values ... ok
test tests::test_start_game_rejects_active_game ... ok
test tests::test_start_game_rejects_insufficient_reserves ... ok
test tests::test_start_game_rejects_wager_above_maximum ... ok
test tests::test_start_game_rejects_wager_below_minimum ... ok
test tests::test_start_game_rejects_when_paused ... ok
test tests::test_start_game_succeeds_with_valid_inputs ... ok
test tests::test_verify_commitment ... ok
test property_tests::test_start_game_state_persistence_and_stats ... ok
test property_tests::test_stats_initialization ... ok
test property_tests::prop_wagers_within_bounds_accepted ... ok

test result: ok. 57 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 29.18s
```

## New tests added (reveal loss cleanup)

| Test | What it covers |
|------|---------------|
| `test_reveal_rejects_no_active_game` | Guard: player with no game gets `NoActiveGame` |
| `test_reveal_rejects_wrong_phase` | Guard: game not in `Committed` phase gets `InvalidPhase` |
| `test_reveal_rejects_wrong_secret` | Guard: wrong pre-image gets `CommitmentMismatch` |
| `test_reveal_loss_clears_game_state` | Loss: `PlayerGame` storage entry is deleted |
| `test_reveal_loss_credits_wager_to_reserves` | Loss: `reserve_balance` increases by exact wager amount |
| `test_reveal_win_increments_streak_and_advances_phase` | Win: streak becomes 1, phase becomes `Revealed` |
| `test_reveal_loss_allows_new_game_after` | Loss: player can immediately start a new game |

## Edge cases covered

- Commitment mismatch (wrong secret) → rejected before any state change
- Wrong phase (already `Revealed` or `Completed`) → rejected
- No game at all → rejected
- Loss with exact wager accounting (reserve delta == wager)
- Post-loss slot is fully cleared (no stale state)
- Win path does not touch reserves
