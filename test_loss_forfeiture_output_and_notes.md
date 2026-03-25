# Loss Forfeiture Property Tests — Output & Notes

Closes #120

## Test Run Output

```
running 5 tests
test loss_forfeiture_tests::prop_loss_reserve_overflow_is_safe ... ok
test loss_forfeiture_tests::prop_loss_credits_exact_wager_to_reserves ... ok
test loss_forfeiture_tests::prop_loss_returns_false_and_clears_state ... ok
test loss_forfeiture_tests::prop_loss_frees_slot_and_resets_streak ... ok
test loss_forfeiture_tests::prop_loss_forfeiture_is_side_agnostic ... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 57 filtered out; finished in 11.34s
```

Full suite (all modules):
```
test result: ok. 62 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 20.02s
```

## Invariants Verified

| ID   | Invariant                                                                 | Test                                          |
|------|---------------------------------------------------------------------------|-----------------------------------------------|
| LF-1 | `reveal` returns `Ok(false)` on any loss                                  | `prop_loss_returns_false_and_clears_state`    |
| LF-2 | Player game state is fully deleted from storage after a loss              | `prop_loss_returns_false_and_clears_state`    |
| LF-3 | `reserve_balance` increases by exactly the forfeited wager                | `prop_loss_credits_exact_wager_to_reserves`   |
| LF-4 | Player slot is freed after a loss — new `start_game` succeeds immediately | `prop_loss_frees_slot_and_resets_streak`      |
| LF-5 | New game after a loss starts with `streak = 0` (no carry-over)           | `prop_loss_frees_slot_and_resets_streak`      |
| LF-6 | Forfeiture semantics are identical for both Heads and Tails losses        | `prop_loss_forfeiture_is_side_agnostic`       |
| LF-7 | Reserve overflow is handled safely near `i128::MAX` (no wrap/panic)      | `prop_loss_reserve_overflow_is_safe`          |

## Outcome Derivation Notes

In the test environment, `env.ledger().sequence()` defaults to `0`, so:

```
contract_random = sha256([0x00, 0x00, 0x00, 0x00])
contract_random[0] = 0xdf  (low bit = 1)

outcome_bit = (sha256(secret)[0] XOR contract_random[0]) & 1
  0 → Heads
  1 → Tails
```

Calibrated loss secrets used in the tests:

| Secret       | sha256[0] | XOR 0xdf | outcome | Loss when side = |
|--------------|-----------|----------|---------|------------------|
| `[3u8; 32]`  | 0x64      | 0x01     | Tails   | Heads            |
| `[2u8; 32]`  | 0x65      | 0x00     | Heads   | Tails            |

## Property Configuration

All proptest cases run with `ProptestConfig::with_cases(200)` for the parameterised tests.
The overflow edge case (`LF-7`) is a deterministic unit-style test (no random sampling needed).

## Wager Range Covered

`1_000_000` to `100_000_000` stroops (1–100 XLM equivalent), covering the full practical
range of the configured contract bounds.
