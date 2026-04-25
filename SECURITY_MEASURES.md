# Tossd Security Measures

## Overview

This document outlines the security measures implemented in the Tossd coinflip contract to prevent common vulnerabilities and attacks.

## Threat Model

### Attack Vectors

1. **Reentrancy Attacks**
   - Player attempts to call claim_winnings multiple times
   - Mitigation: State machine prevents invalid phase transitions

2. **Authorization Bypass**
   - Attacker attempts to call admin functions without authorization
   - Mitigation: All admin functions require admin signature

3. **Input Validation Exploits**
   - Attacker submits invalid wager amounts
   - Attacker submits invalid commitments
   - Mitigation: Strict input validation on all parameters

4. **Timing Attacks**
   - Player attempts to reveal after timeout
   - Attacker attempts to manipulate ledger sequence
   - Mitigation: Timeout enforcement and ledger sequence validation

5. **State Corruption**
   - Attacker attempts invalid phase transitions
   - Attacker attempts to corrupt game state
   - Mitigation: Strict state machine validation

6. **Transfer Failures**
   - Attacker attempts to cause transfer failures
   - Mitigation: Safe transfer handling with error recovery

## Security Controls

### 1. Reentrancy Prevention

**Mechanism**: State Machine Validation

```rust
// Game phases prevent reentrancy
pub enum GamePhase {
    Completed,  // No active game
    Committed,  // Waiting for reveal
    Revealed,   // Waiting for claim
}

// claim_winnings only works in Revealed phase
// After claim, phase becomes Completed
// Prevents multiple claims
```

**Tests**:
- `test_reentrancy_prevention_claim_winnings`
- Verify phase transitions prevent multiple claims

### 2. Authorization Control

**Mechanism**: Admin Signature Verification

```rust
// All admin functions require admin address
pub fn set_paused(admin: Address, paused: bool) {
    admin.require_auth();  // Verify admin signature
    // ... update state
}
```

**Protected Functions**:
- `set_paused` - Pause/unpause contract
- `set_fee` - Update fee percentage
- `set_wager_limits` - Update wager limits
- `set_treasury` - Update treasury address

**Tests**:
- `test_authorization_bypass_prevention`
- Verify non-admin cannot call admin functions

### 3. Input Validation

**Wager Validation**:
```rust
// Validate wager is within limits
assert!(wager >= config.min_wager, Error::WagerBelowMinimum);
assert!(wager <= config.max_wager, Error::WagerAboveMaximum);
```

**Commitment Validation**:
```rust
// Commitment must be 32 bytes (enforced by type system)
pub commitment: BytesN<32>
```

**Fee Validation**:
```rust
// Fee must be <= 100% (10000 basis points)
assert!(fee_percentage <= 10000, Error::InvalidFeePercentage);
```

**Tests**:
- `test_input_validation_malicious_wagers`
- `test_commitment_validation_prevents_invalid_input`
- `test_fee_percentage_validation`
- `test_wager_limit_enforcement`

### 4. Timing Attack Prevention

**Reveal Timeout**:
```rust
// Enforce reveal timeout
const REVEAL_TIMEOUT: u32 = 1000;  // blocks

// Check timeout on reveal
let elapsed = env.ledger().sequence() - game.last_reveal_ledger;
assert!(elapsed < REVEAL_TIMEOUT, Error::RevealTimeout);
```

**Ledger Sequence Validation**:
```rust
// Use ledger sequence for randomness
let contract_random = SHA256(ledger_sequence);
// Attacker cannot manipulate ledger sequence
```

**Tests**:
- `test_timing_attack_prevention_reveal_timeout`
- Verify timeout enforcement

### 5. State Machine Validation

**Valid Transitions**:
```
Completed -> Committed (start_game)
Committed -> Revealed (reveal)
Revealed -> Completed (claim_winnings or cash_out)
```

**Invalid Transitions Prevented**:
- Completed -> Revealed (invalid)
- Revealed -> Committed (invalid)
- Committed -> Completed (invalid)

**Tests**:
- `test_invalid_phase_transition_prevention`
- Verify only valid transitions allowed

### 6. Transfer Safety

**Safe Transfer Pattern**:
```rust
// Verify reserves before transfer
assert!(reserves >= payout, Error::InsufficientReserves);

// Attempt transfer
let result = token.transfer(&player, &payout);

// Handle transfer failure
if result.is_err() {
    return Err(Error::TransferFailed);
}
```

**Tests**:
- `test_transfer_failure_handling`
- `test_reserve_validation_before_payout`

## Vulnerability Coverage

### OWASP Top 10 Mapping

| OWASP | Vulnerability | Tossd Mitigation |
|-------|---|---|
| A01 | Broken Access Control | Admin signature verification |
| A02 | Cryptographic Failures | SHA-256 commitment-reveal |
| A03 | Injection | Input validation, type system |
| A04 | Insecure Design | State machine validation |
| A05 | Security Misconfiguration | Initialization validation |
| A06 | Vulnerable Components | Soroban SDK security |
| A07 | Authentication Failures | Admin signature verification |
| A08 | Data Integrity Failures | State machine validation |
| A09 | Logging Failures | Event emission |
| A10 | SSRF | N/A (blockchain context) |

## Security Testing

### Test Categories

1. **Reentrancy Tests**
   - Verify state machine prevents multiple claims
   - Verify phase transitions are enforced

2. **Authorization Tests**
   - Verify admin functions require authorization
   - Verify non-admin cannot call admin functions

3. **Input Validation Tests**
   - Test boundary values
   - Test invalid inputs
   - Test malicious payloads

4. **Timing Tests**
   - Test timeout enforcement
   - Test ledger sequence validation

5. **State Machine Tests**
   - Test valid transitions
   - Test invalid transitions
   - Test state consistency

6. **Transfer Tests**
   - Test successful transfers
   - Test transfer failures
   - Test reserve validation

## Regression Testing

All security findings are added as regression tests:

```rust
#[test]
fn test_regression_<vulnerability>() {
    // Test that vulnerability is fixed
    // Prevent regression in future updates
}
```

## Security Audit Checklist

- [x] Reentrancy prevention
- [x] Authorization control
- [x] Input validation
- [x] Timing attack prevention
- [x] State machine validation
- [x] Transfer safety
- [x] Error handling
- [x] Logging and monitoring
- [ ] External audit
- [ ] Formal verification

## Known Limitations

1. **Ledger Sequence Predictability**
   - Ledger sequence is predictable within a block
   - Mitigated by commit-reveal scheme

2. **Player Collusion**
   - Multiple players could collude
   - Mitigated by randomness from ledger sequence

3. **Contract Pause**
   - Admin can pause contract
   - Mitigated by governance and transparency

## Future Improvements

1. **Multi-signature Admin**
   - Require multiple signatures for admin actions
   - Reduce single point of failure

2. **Formal Verification**
   - Formally verify state machine
   - Prove security properties

3. **External Audit**
   - Third-party security audit
   - Identify additional vulnerabilities

4. **Bug Bounty Program**
   - Incentivize security researchers
   - Discover vulnerabilities early

## Incident Response

### Security Incident Procedure

1. **Detect**: Monitor for security events
2. **Respond**: Pause contract if needed
3. **Investigate**: Analyze logs and events
4. **Remediate**: Fix vulnerability
5. **Deploy**: Deploy patched contract
6. **Verify**: Verify fix effective
7. **Communicate**: Notify stakeholders

### Escalation

- Critical: Pause contract immediately
- High: Fix within 24 hours
- Medium: Fix within 1 week
- Low: Fix in next release

## References

- [OWASP Top 10](https://owasp.org/www-project-top-ten/)
- [Soroban Security](https://developers.stellar.org/docs/learn/security)
- [Smart Contract Security](https://ethereum.org/en/developers/docs/smart-contracts/security/)
