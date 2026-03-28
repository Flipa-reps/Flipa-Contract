# Task #341: Add property tests for fund conservation

## Steps (1/6 complete)

### 1. [ ] Create branch: `git checkout -b add-property-tests-fund-conservation`
### 2. [x] Add `prop_fund_conservation` property test to `contract/integration_tests.rs`
### 3. [ ] `cargo test --lib integration` - verify passes 200+ cases
### 4. [ ] Commit: `git add . && git commit -m "test: add property tests for fund conservation (#341)"`
### 5. [ ] `gh pr create --title "Add property tests for fund conservation (#341)" --body "Implements Property 21 verifying total funds conserved across game lifecycle"`
### 6. [ ] Verify PR tests pass

**Property 21 verifies**: total_funds = player_balance + contract_reserve + treasury_balance remains constant through complete game lifecycles (start→reveal→cash_out/claim/continue) across randomized wagers/fees/streaks, 200+ iterations. ✓ Added to integration_tests.rs

