# Tossd Contract Upgrade Strategy

## Overview

This document outlines the zero-downtime upgrade procedures and backward compatibility guarantees for the Tossd coinflip contract.

## Upgrade Principles

1. **Zero Downtime**: Contract remains operational during upgrades
2. **Backward Compatibility**: v2 contract supports v1 game states
3. **Data Integrity**: All player data preserved across upgrades
4. **Rollback Safety**: Quick rollback to previous version if needed

## State Migration Strategy

### v1 to v2 Migration

#### Game State Migration

v1 GameState structure:
```rust
pub struct GameState {
    pub player: Address,
    pub wager: i128,
    pub phase: GamePhase,
    pub commitment: BytesN<32>,
    pub contract_random: BytesN<32>,
    pub outcome: Side,
    pub payout: i128,
    pub streak: u32,
    pub last_reveal_ledger: u32,
}
```

v2 GameState structure (backward compatible):
```rust
pub struct GameState {
    pub player: Address,
    pub wager: i128,
    pub phase: GamePhase,
    pub commitment: BytesN<32>,
    pub contract_random: BytesN<32>,
    pub outcome: Side,
    pub payout: i128,
    pub streak: u32,
    pub last_reveal_ledger: u32,
    // v2 additions (optional fields with defaults)
}
```

#### Config Migration

v1 ContractConfig:
```rust
pub struct ContractConfig {
    pub min_wager: i128,
    pub max_wager: i128,
    pub fee_percentage: u32,
    pub paused: bool,
}
```

v2 ContractConfig (backward compatible):
```rust
pub struct ContractConfig {
    pub min_wager: i128,
    pub max_wager: i128,
    pub fee_percentage: u32,
    pub paused: bool,
    // v2 additions (optional fields with defaults)
}
```

#### Stats Migration

v1 ContractStats:
```rust
pub struct ContractStats {
    pub total_games: u64,
    pub total_wagers: i128,
    pub total_payouts: i128,
    pub total_fees: i128,
}
```

v2 ContractStats (backward compatible):
```rust
pub struct ContractStats {
    pub total_games: u64,
    pub total_wagers: i128,
    pub total_payouts: i128,
    pub total_fees: i128,
    // v2 additions (optional fields with defaults)
}
```

## Backward Compatibility Guarantees

### Error Code Stability

All error codes remain stable across upgrades:

| Code | Variant | Status |
|------|---------|--------|
| 1 | WagerBelowMinimum | Stable |
| 2 | WagerAboveMaximum | Stable |
| 3 | ActiveGameExists | Stable |
| 4 | InsufficientReserves | Stable |
| 5 | ContractPaused | Stable |
| 10 | NoActiveGame | Stable |
| 11 | InvalidPhase | Stable |
| 12 | CommitmentMismatch | Stable |
| 13 | RevealTimeout | Stable |
| 20 | NoWinningsToClaimOrContinue | Stable |
| 21 | InvalidCommitment | Stable |
| 30 | Unauthorized | Stable |
| 31 | InvalidFeePercentage | Stable |
| 32 | InvalidWagerLimits | Stable |
| 40 | TransferFailed | Stable |
| 50 | AdminTreasuryConflict | Stable |
| 51 | AlreadyInitialized | Stable |

### Function Signature Stability

All public function signatures remain compatible:

- `initialize(admin, token, treasury, min_wager, max_wager, fee_percentage)`
- `start_game(player, side, wager, commitment)`
- `reveal(player, secret)`
- `claim_winnings(player)`
- `continue_streak(player, side, wager, commitment)`
- `cash_out(player)`
- `set_paused(admin, paused)`
- `set_fee(admin, fee_percentage)`
- `set_wager_limits(admin, min_wager, max_wager)`
- `set_treasury(admin, treasury)`

### Storage Key Stability

All storage keys remain stable:

- `config` - Contract configuration
- `stats` - Contract statistics
- `player_game:{player}` - Player's current game
- `player_history:{player}` - Player's game history
- `treasury` - Treasury address

## Upgrade Procedure

### Pre-Upgrade Checklist

- [ ] All tests passing
- [ ] Backward compatibility verified
- [ ] Data migration scripts tested
- [ ] Rollback plan documented
- [ ] Stakeholders notified

### Upgrade Steps

1. **Deploy v2 Contract**
   - Deploy new contract code to testnet
   - Verify deployment successful

2. **Migrate State**
   - Read v1 state from storage
   - Transform to v2 format (if needed)
   - Write v2 state to storage

3. **Verify Migration**
   - Verify all data migrated correctly
   - Verify data integrity
   - Verify no data loss

4. **Activate v2**
   - Update contract reference
   - Verify v2 contract operational
   - Monitor for errors

5. **Post-Upgrade Validation**
   - Verify all games playable
   - Verify all queries working
   - Verify all admin functions working

### Rollback Procedure

If issues detected:

1. **Pause v2 Contract**
   - Call `set_paused(admin, true)` on v2

2. **Revert to v1**
   - Update contract reference back to v1
   - Verify v1 contract operational

3. **Investigate Issues**
   - Analyze v2 logs
   - Identify root cause
   - Fix issues

4. **Retry Upgrade**
   - Deploy fixed v2 contract
   - Repeat upgrade procedure

## Data Integrity Validation

### Pre-Migration Validation

- [ ] Verify all game states readable
- [ ] Verify all configs readable
- [ ] Verify all stats readable
- [ ] Verify no corrupted data

### Post-Migration Validation

- [ ] Verify all game states migrated
- [ ] Verify all configs migrated
- [ ] Verify all stats migrated
- [ ] Verify data consistency
- [ ] Verify no data loss

### Integrity Checks

```rust
// Verify game state integrity
assert!(game.wager > 0);
assert!(game.payout >= 0);
assert!(game.streak >= 0);

// Verify config integrity
assert!(config.min_wager > 0);
assert!(config.max_wager >= config.min_wager);
assert!(config.fee_percentage <= 10000);

// Verify stats integrity
assert!(stats.total_games >= 0);
assert!(stats.total_wagers >= 0);
assert!(stats.total_payouts >= 0);
assert!(stats.total_fees >= 0);
```

## Testing Strategy

### Unit Tests

- Test v1 to v2 state migration
- Test backward compatibility
- Test data integrity after migration
- Test rollback procedures

### Integration Tests

- Test full upgrade flow
- Test v2 contract with v1 data
- Test v1 games playable on v2
- Test new v2 features

### Property Tests

- Verify no data loss
- Verify data consistency
- Verify error codes stable
- Verify function signatures stable

## Monitoring During Upgrade

### Key Metrics

- Error rate
- Transaction latency
- Game completion rate
- Payout success rate

### Alerts

- Error rate spike
- Transaction latency spike
- Game failure rate spike
- Payout failure rate spike

## Rollback Criteria

Automatic rollback triggered if:

- Error rate > 5%
- Transaction latency > 10s
- Game failure rate > 1%
- Payout failure rate > 1%

## Documentation

- [x] Upgrade strategy documented
- [x] State migration documented
- [x] Backward compatibility documented
- [x] Rollback procedure documented
- [ ] Runbook created
- [ ] Team trained

## Maintenance

- Review upgrade strategy quarterly
- Update for new features
- Test rollback procedures monthly
- Document lessons learned
