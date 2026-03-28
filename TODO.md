# TODO: Complete Integration Tests for Game Flow (#340)

## Approved Plan Steps (Issue #340)

### 1. [x] Create branch `add-integration-tests-complete-game-flow` ✅
### 2. [] Extend contract/src/lib.rs integration_tests:
   - test_losses_at_streak_1_to_4_plus(): Inject Revealed(streak=1/2/3/4+), verify cash_out/continue_streak → NoWinningsToClaimOrContinue, reserves unchanged.
   - test_max_streak_4_wins_cash_out(): 4 consecutive wins via play_round+continue, verify 10x payout.
   - test_streak_losses_preserve_reserves(): Loss scenarios post-streak.
   - test_claim_winnings_token_transfers(): Verify player/treasury balances with StellarAssetClient.
### 3. [] cd contract && cargo test (all pass + new tests)
### 4. [] Commit: `test: add integration tests for complete game flow (#340)`
### 5. [] gh pr create --title "..." --body "..."
### 6. [] Update TODO.md: Mark complete

**Progress**: 1/6 complete  
**Next**: Edit lib.rs integration_tests

