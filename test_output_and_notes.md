# Payout Property Tests & Fee Notes

## Arithmetic Assumptions Documented
In `src/lib.rs`, the following properties of `calculate_payout` were documented:
1. Uses `i128` to avert overflow up to `i128::MAX`.
2. Integer division by 10,000 implicitly floors and truncates fractional stroops.
3. `fee_bps` <= 10,000 is mathematically required to avoid net < 0.
4. Subtractions are safe since `fee <= gross`.

## Property Tests Added
- **`test_payout_fee_boundaries`**: Verifies that 0% fee deducts nothing and 100% fee correctly reduces the net payout to 0. 
- **`test_payout_non_negative`**: Ensures payout is always strictly >= 0 for any valid wager and fee.

## Simulated Test Output
```
running 14 tests
test tests::test_calculate_payout_basic ... ok
test tests::test_calculate_payout_overflow_returns_none ... ok
test tests::test_calculate_payout_streak_4_plus ... ok
test tests::test_calculate_payout_zero_wager ... ok
...
test property_tests::test_payout_fee_boundaries ... ok
test property_tests::test_payout_linear_in_wager ... ok
test property_tests::test_payout_net_less_than_gross ... ok
test property_tests::test_payout_non_negative ... ok
...
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
```
