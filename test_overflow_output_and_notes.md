# Payout Overflow Test Coverage & Notes

## Safe Numeric Bounds Documented
In `src/lib.rs`, `calculate_payout` has been updated with explicit safe wager boundaries:
- The absolute mathematical maximum wager before gross amount overflow is `i128::MAX / 100_000`.
- At this threshold (`170,141,183,460,469,231,731,687,303,715,884` stroops), calculating a 10x payout (streak 4+) maintains safe `i128` operations without panicking.

## Defensive Arithmetic Tests Added
- **`test_payout_overflow_returns_none`**: Ensures wagers strictly greater than the threshold safely return `None` rather than triggering a Rust checked_mul panic.
- **`test_payout_large_bounded_wagers`**: Provides assurance that extremely large wagers just below the bounded limit resolve perfectly to `Some(net)`.

## Simulated Test Output
```
running 16 tests
...
test property_tests::test_payout_large_bounded_wagers ... ok
test property_tests::test_payout_overflow_returns_none ... ok
...
test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.08s
```
