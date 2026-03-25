# Cash-Out Transfer Accounting Documentation

## Overview

This document specifies the accounting expectations and balance transfer validation for the Tossd coinflip contract's cash-out and settlement mechanisms. The contract maintains strict separation between player winnings, protocol fees, and treasury funds to ensure fund safety and transparent accounting.

## Transfer Flow Architecture

### Core Transfer Operations

1. **Player Winnings Transfer**: Net payout (gross - fee) from contract to player
2. **Protocol Fee Transfer**: Fee amount from contract to treasury  
3. **Reserve Accounting**: Contract reserves decrease by gross payout amount
4. **Statistics Tracking**: Total fees and volume updated atomically

### Transfer Atomicity

All transfers in a single settlement operation are **atomic**:
- Either all transfers succeed together, or none succeed
- No partial settlement states are possible
- Contract state is only updated after all transfers complete successfully

## Accounting Formulas

### Payout Calculations

```
gross_payout = wager × multiplier_bps / 10_000
fee_amount   = gross_payout × fee_bps / 10_000  
net_payout   = gross_payout - fee_amount
```

### Balance Changes

```
contract_balance_post = contract_balance_pre - gross_payout
treasury_balance_post = treasury_balance_pre + fee_amount
player_balance_post   = player_balance_pre + net_payout
reserve_balance_post  = reserve_balance_pre - gross_payout
```

### Statistics Updates

```
total_fees_post = total_fees_pre + fee_amount
total_volume_post = total_volume_pre + wager  (updated at game start)
```

## Transfer Validation Properties

### Property 1: Exact Balance Changes

**Invariant**: After successful claim_winnings:
- Contract balance decreases by exactly `gross_payout`
- Treasury balance increases by exactly `fee_amount`  
- Player balance increases by exactly `net_payout`
- Reserve balance decreases by exactly `gross_payout`

**Validation**: `gross_payout = net_payout + fee_amount`

### Property 2: Fee Separation Integrity

**Invariant**: Fee calculation is mathematically sound:
- `fee_amount = gross_payout × fee_bps / 10_000`
- `fee_amount < gross_payout` for all valid fee_bps (2-5%)
- `net_payout > 0` for all valid wagers and fee_bps

**Edge Cases**:
- `fee_bps = 0` → `fee_amount = 0`, `net_payout = gross_payout`
- `fee_bps = 10_000` → `fee_amount = gross_payout`, `net_payout = 0`

### Property 3: Reserve Solvency

**Invariant**: Contract never pays out more than it holds:
- `reserve_balance_pre ≥ gross_payout` (pre-transfer check)
- `reserve_balance_post = reserve_balance_pre - gross_payout ≥ 0`
- Transfer fails with `InsufficientReserves` if solvency check fails

### Property 4: Multi-Claim Independence

**Invariant**: Sequential claims from different players are independent:
- Each claim affects only the claiming player's balance
- Treasury accumulates fees from all claims cumulatively
- Contract reserves decrease by each claim's gross payout
- No cross-player interference or double-spending

### Property 5: Continue Streak Transfer Neutrality

**Invariant**: `continue_streak` operation involves no transfers:
- All balances (contract, treasury, player) remain unchanged
- Only game state transitions from `Revealed` → `Committed`
- Reserve requirements are checked but no funds moved

## Error Handling and Rollback

### Transfer Failure Scenarios

1. **TransferFailed**: Token transfer to player or treasury fails
   - All partial transfers are automatically rolled back
   - Contract state remains unchanged
   - Game stays in `Revealed` phase for retry

2. **InsufficientReserves**: Contract lacks funds for gross payout
   - Operation rejected before any transfers
   - No state changes occur
   - Player can try again after reserves are replenished

3. **InvalidPhase**: Game not in `Revealed` phase
   - Operation rejected without transfers
   - Common for `continue_streak` on losing games

### Accounting Consistency Guarantees

- **No Partial Settlement**: Either all transfers succeed or none
- **State Synchronization**: Contract stats updated only after successful transfers
- **Balance Integrity**: Total system value is conserved (contract + treasury + player)

## Test Coverage Matrix

| Property | Test Function | Coverage | Validation |
|----------|---------------|----------|------------|
| Exact Balance Changes | `test_claim_winnings_balance_transfers` | Property-based (50 cases) | Pre/post balance verification |
| Fee Separation | `test_fee_net_payout_separation` | Property-based (50 cases) | Mathematical invariants |
| Multi-Claim Independence | `test_multiple_claims_balance_tracking` | Property-based (50 cases) | Sequential claim isolation |
| Continue Neutrality | `test_continue_streak_no_transfers` | Property-based (50 cases) | No transfer verification |
| Reserve Solvency | `test_reserve_solvency_during_settlement` | Property-based (50 cases) | Solvency maintenance |

## Edge Case Handling

### Zero Wager Edge Case
- `wager = 0` → `gross_payout = 0`, `fee_amount = 0`, `net_payout = 0`
- All balance changes are zero but operation succeeds
- Statistics updated with zero values

### Maximum Values Edge Case
- `wager = max_wager`, `streak = 4+`, `fee_bps = 500`
- Tests overflow safety with `checked_mul` and `checked_div`
- Returns `None` on overflow, handled as `InsufficientReserves`

### Rounding Behavior
- Integer division truncates fractional stroops
- Fee calculation may round down (beneficial to player)
- Net payout may be 1 stroop higher due to rounding

## Security Considerations

### Reentrancy Protection
- All state updates happen after external token transfers
- Contract follows checks-effects-interactions pattern
- No state changes before transfer completion

### Integer Overflow Protection
- All arithmetic uses `checked_*` operations
- Overflow results in `InsufficientReserves` error
- No silent overflow or wrapping behavior

### Access Control
- Only game winner can claim winnings (`player.require_auth()`)
- Only player with `Revealed` phase game can claim
- Treasury address is immutable after initialization

## Monitoring and Auditing

### Key Metrics to Monitor
- `total_fees`: Cumulative protocol revenue
- `reserve_balance`: Contract solvency indicator  
- `total_volume`: Economic activity measure
- Transfer success/failure rates

### Audit Trail
- Every successful claim updates immutable statistics
- Balance changes are verifiable on-chain
- Failed attempts leave traceable error logs

## Implementation Notes

### Token Interface Requirements
- Contract uses standard Soroban `token::Client`
- Transfer failures detected via `InvokeError` return values
- Balance queries use standard `balance()` method

### Gas Optimization
- Balance queries minimized through pre/post comparison
- Batch operations where possible to reduce transaction costs
- Efficient integer arithmetic with overflow checks

### Upgrade Compatibility
- Accounting formulas are deterministic and version-stable
- Test properties provide regression protection for upgrades
- Interface changes maintain accounting invariants

---

**Document Version**: 1.0  
**Last Updated**: 2024-03-24  
**Related Issues**: #126 - Cash out transfer property coverage  
**Test Coverage**: 5 property tests, 250+ randomized cases
