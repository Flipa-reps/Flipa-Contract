# Streak Increment Property Tests — Notes & Output

## Branch
`feature/streak-increment-tests`

## Closes
#121

## What was added

Three new test modules appended to `contract/src/lib.rs`:

### `streak_increment_tests` (primary — 9 tests)

Property and unit tests covering all six streak increment invariants:

| Test | Invariant | Cases |
|------|-----------|-------|
| `test_streak_starts_at_zero_and_first_win_reaches_tier_1` | I-4: fresh game starts at 0, first win → tier 1 | deterministic |
| `test_no_tier_is_skipped_across_all_transitions` | I-3: every tier reachable in exactly one step | deterministic |
| `test_streak_increments_past_tier_4_without_reset` | I-5: counter keeps going past cap, no reset | deterministic |
| `test_single_win_increments_by_exactly_one_deterministic` | I-1: +1 per win, spot-checked values | deterministic |
| `prop_single_win_increments_streak_by_exactly_one` | I-1: ∀ streak ∈ [0, u32::MAX), win → streak+1 | 500 |
| `prop_streak_progression_is_strictly_monotonic` | I-2: k wins from any start → start+k, no gaps | 500 |
| `prop_no_multiplier_tier_is_skipped` | I-3: tier advances by exactly 1 for streaks 0–3 | 500 |
| `prop_streak_past_tier_4_stays_capped` | I-5: multiplier stays at 10x cap for streak ≥ 4 | 500 |
| `prop_payout_strictly_increases_with_streak_tier` | I-6: payout(streak+1) > payout(streak) for tiers 1–3 | 500 |
| `prop_new_game_streak_always_initializes_to_zero` | streak=0 on every new game regardless of inputs | 500 |
| `prop_k_wins_from_zero_yields_streak_k_and_correct_tier` | k wins from 0 → streak k, tier min(k,4) | 500 |

### `outcome_determinism_tests` (6 tests)

Validates all pure helpers are referentially transparent:

- `prop_multiplier_is_deterministic`
- `prop_payout_is_deterministic`
- `prop_commitment_verification_is_deterministic`
- `prop_wrong_secret_never_verifies`
- `prop_multiplier_tier_boundaries_are_stable`
- `prop_zero_wager_payout_is_zero`

### `randomness_regression_tests` (5 tests)

Validates the commit-reveal scheme resists unilateral manipulation:

- `prop_commitment_round_trip`
- `prop_distinct_secrets_produce_distinct_commitments`
- `prop_tampered_commitment_fails_verification`
- `prop_tampered_secret_fails_verification`
- `prop_commitment_verification_is_not_symmetric`

## Invariants documented

```
I-1  A single win increments streak by exactly 1 (never 0, never 2+).
I-2  Streak progression is strictly monotonic: streak_n+1 == streak_n + 1.
I-3  No multiplier tier is skipped: every tier 1→2→3→4 is reachable in
     exactly one step from the previous tier.
I-4  Streak starts at 0 on a fresh game and reaches tier 1 on the first win.
I-5  Streak saturates at tier 4+ — the multiplier is capped but the counter
     continues to increment (no overflow, no reset).
I-6  Payout at streak N+1 is strictly greater than payout at streak N for
     any fixed wager and fee.
```

## Bug fixes included

The `cargo check --tests` pass also surfaced a pre-existing type mismatch:
`env.crypto().sha256()` returns `Hash<32>`, not `BytesN<32>`, in soroban-sdk 22.0.11.
Fixed in:
- `verify_commitment` (production code)
- `start_game` — `contract_random` field assignment
- `dummy_commitment` / `dummy_commitment_prop` test helpers
- `test_verify_commitment` unit test

## cargo check output

```
warning: unused import: `token`   (pre-existing)
warning: associated function `delete_player_game` is never used   (pre-existing)
warning: hiding a lifetime that's elided elsewhere   (pre-existing)
Finished `dev` profile [unoptimized + debuginfo] target(s) in 12.18s
```

Zero new warnings introduced. Zero errors.

## Running the tests

```bash
# Full suite
cargo test

# New module only
cargo test --lib streak_increment_tests::
cargo test --lib outcome_determinism_tests::
cargo test --lib randomness_regression_tests::
```

> Note: a complete native linker (`link.exe` on MSVC or `dlltool.exe` on GNU)
> must be present to link the test binary. The code passes `cargo check --tests`
> cleanly on the current machine; run `cargo test` in a CI environment or after
> installing the missing linker component.
