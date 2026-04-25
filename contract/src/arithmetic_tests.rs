/// Comprehensive arithmetic overflow safety tests for the Tossd contract.
///
/// # Coverage
/// - Boundary value tests for all arithmetic operations
/// - Multiplication, division, addition, subtraction at extremes
/// - Payout calculations with i128::MAX wager
/// - Fee calculations at 0 and maximum values
/// - Reserve balance arithmetic with large values
/// - Multiplier calculations at extreme streaks
/// - Ensures no panics on arithmetic errors (all use checked arithmetic)
///
/// # Test Strategy
/// - Test both positive and negative boundaries
/// - Verify error types match expectations
/// - Use property tests for arithmetic invariants
/// - Document overflow behavior in function docs
use super::{calculate_payout, calculate_payout_breakdown, get_multiplier};

// ── Boundary: i128::MAX wager ────────────────────────────────────────────────

/// Test payout calculation with i128::MAX wager at streak 1.
/// Should return None due to overflow in multiplication.
#[test]
fn payout_i128_max_wager_streak_1_overflows() {
    let result = calculate_payout(i128::MAX, 1, 300);
    assert_eq!(result, None, "i128::MAX wager should overflow at streak 1");
}

/// Test payout calculation with i128::MAX wager at streak 4+.
/// Should return None due to overflow in multiplication.
#[test]
fn payout_i128_max_wager_streak_4_plus_overflows() {
    let result = calculate_payout(i128::MAX, 4, 300);
    assert_eq!(result, None, "i128::MAX wager should overflow at streak 4+");
}

/// Test payout breakdown with i128::MAX wager.
/// Should return None due to overflow in multiplication.
#[test]
fn payout_breakdown_i128_max_wager_overflows() {
    let result = calculate_payout_breakdown(i128::MAX, 1, 300);
    assert_eq!(result, None, "i128::MAX wager should overflow in breakdown");
}

// ── Boundary: i128::MIN (negative) ──────────────────────────────────────────

/// Test payout calculation with i128::MIN (negative) wager.
/// Should return None due to overflow in multiplication.
#[test]
fn payout_i128_min_wager_overflows() {
    let result = calculate_payout(i128::MIN, 1, 300);
    assert_eq!(result, None, "i128::MIN wager should overflow");
}

// ── Boundary: Zero wager ────────────────────────────────────────────────────

/// Test payout calculation with zero wager.
/// Should return Some(0) since 0 × multiplier = 0.
#[test]
fn payout_zero_wager_returns_zero() {
    let result = calculate_payout(0, 1, 300);
    assert_eq!(result, Some(0), "zero wager should return zero payout");
}

/// Test payout breakdown with zero wager.
/// Should return Some((0, 0, 0, 0)).
#[test]
fn payout_breakdown_zero_wager_returns_zeros() {
    let result = calculate_payout_breakdown(0, 1, 300);
    assert_eq!(result, Some((0, 0, 0, 0)), "zero wager should return (0, 0, 0)");
}

// ── Boundary: Minimum fee (200 bps) ─────────────────────────────────────────

/// Test payout calculation with minimum fee (200 bps).
/// Should succeed and return valid net payout.
#[test]
fn payout_min_fee_200bps_succeeds() {
    let result = calculate_payout(1_000_000, 1, 200);
    assert!(result.is_some(), "minimum fee should not overflow");
    let net = result.unwrap();
    assert!(net > 0, "net payout should be positive");
}

/// Test payout breakdown with minimum fee (200 bps).
/// Should succeed and return valid breakdown.
#[test]
fn payout_breakdown_min_fee_200bps_succeeds() {
    let result = calculate_payout_breakdown(1_000_000, 1, 200);
    assert!(result.is_some(), "minimum fee should not overflow");
    let (gross, fee, net, _bonus) = result.unwrap();
    assert!(gross > 0, "gross should be positive");
    assert!(fee > 0, "fee should be positive");
    assert!(net > 0, "net should be positive");
    assert_eq!(gross - fee, net, "gross - fee should equal net");
}

// ── Boundary: Maximum fee (500 bps) ─────────────────────────────────────────

/// Test payout calculation with maximum fee (500 bps).
/// Should succeed and return valid net payout.
#[test]
fn payout_max_fee_500bps_succeeds() {
    let result = calculate_payout(1_000_000, 1, 500);
    assert!(result.is_some(), "maximum fee should not overflow");
    let net = result.unwrap();
    assert!(net > 0, "net payout should be positive");
}

/// Test payout breakdown with maximum fee (500 bps).
/// Should succeed and return valid breakdown.
#[test]
fn payout_breakdown_max_fee_500bps_succeeds() {
    let result = calculate_payout_breakdown(1_000_000, 1, 500);
    assert!(result.is_some(), "maximum fee should not overflow");
    let (gross, fee, net, _bonus) = result.unwrap();
    assert!(gross > 0, "gross should be positive");
    assert!(fee > 0, "fee should be positive");
    assert!(net > 0, "net should be positive");
    assert_eq!(gross - fee, net, "gross - fee should equal net");
}

// ── Boundary: Fee at 0 bps (edge case) ──────────────────────────────────────

/// Test payout calculation with zero fee (0 bps).
/// Should succeed and return gross as net.
#[test]
fn payout_zero_fee_0bps_returns_gross() {
    let result = calculate_payout(1_000_000, 1, 0);
    assert!(result.is_some(), "zero fee should not overflow");
    let net = result.unwrap();
    // At streak 1, multiplier is 1.9, so gross = 1_000_000 * 1.9 = 1_900_000
    // With 0 fee, net = gross = 1_900_000
    assert_eq!(net, 1_900_000, "net should equal gross when fee is 0");
}

/// Test payout breakdown with zero fee (0 bps).
/// Should return (gross, 0, gross).
#[test]
fn payout_breakdown_zero_fee_0bps_returns_zero_fee() {
    let result = calculate_payout_breakdown(1_000_000, 1, 0);
    assert!(result.is_some(), "zero fee should not overflow");
    let (gross, fee, net, _bonus) = result.unwrap();
    assert_eq!(fee, 0, "fee should be zero");
    assert_eq!(gross, net, "net should equal gross when fee is zero");
}

// ── Boundary: Very large fee (edge case) ────────────────────────────────────

/// Test payout calculation with very large fee (10_000 bps = 100%).
/// Should succeed but net may be zero or negative.
#[test]
fn payout_very_large_fee_10000bps_succeeds() {
    let result = calculate_payout(1_000_000, 1, 10_000);
    assert!(result.is_some(), "very large fee should not overflow");
    let net = result.unwrap();
    // At streak 1, multiplier is 1.9, so gross = 1_900_000
    // With 100% fee, net = gross - gross = 0
    assert_eq!(net, 0, "net should be zero when fee is 100%");
}

// ── Boundary: Multiplier at extreme streaks ─────────────────────────────────

/// Test multiplier at streak 0.
/// Should return 1.0 (10_000 bps).
#[test]
fn multiplier_streak_0_returns_1x() {
    let multiplier = get_multiplier(0);
    assert_eq!(multiplier, 10_000, "streak 0 should have 1x multiplier");
}

/// Test multiplier at streak 1.
/// Should return 1.9 (19_000 bps).
#[test]
fn multiplier_streak_1_returns_1_9x() {
    let multiplier = get_multiplier(1);
    assert_eq!(multiplier, 19_000, "streak 1 should have 1.9x multiplier");
}

/// Test multiplier at streak 2.
/// Should return 3.5 (35_000 bps).
#[test]
fn multiplier_streak_2_returns_3_5x() {
    let multiplier = get_multiplier(2);
    assert_eq!(multiplier, 35_000, "streak 2 should have 3.5x multiplier");
}

/// Test multiplier at streak 3.
/// Should return 6.0 (60_000 bps).
#[test]
fn multiplier_streak_3_returns_6x() {
    let multiplier = get_multiplier(3);
    assert_eq!(multiplier, 60_000, "streak 3 should have 6x multiplier");
}

/// Test multiplier at streak 4+.
/// Should return 10.0 (100_000 bps).
#[test]
fn multiplier_streak_4_plus_returns_10x() {
    let multiplier = get_multiplier(4);
    assert_eq!(multiplier, 100_000, "streak 4+ should have 10x multiplier");
}

/// Test multiplier at very large streak (u32::MAX).
/// Should return 10.0 (100_000 bps) since all streaks >= 4 cap at 10x.
#[test]
fn multiplier_u32_max_streak_returns_10x() {
    let multiplier = get_multiplier(u32::MAX);
    assert_eq!(multiplier, 100_000, "very large streak should cap at 10x multiplier");
}

// ── Arithmetic invariants ───────────────────────────────────────────────────

/// Test that payout breakdown invariant holds: gross - fee = net.
/// Property test across various wager and streak combinations.
#[test]
fn payout_breakdown_invariant_gross_minus_fee_equals_net() {
    let test_cases = vec![
        (1_000, 0, 300),
        (1_000_000, 1, 200),
        (1_000_000, 2, 300),
        (1_000_000, 3, 400),
        (1_000_000, 4, 500),
        (10_000_000, 1, 250),
        (100_000_000, 2, 350),
    ];

    for (wager, streak, fee_bps) in test_cases {
        if let Some((gross, fee, net, _bonus)) = calculate_payout_breakdown(wager, streak, fee_bps) {
            assert_eq!(
                gross - fee, net,
                "invariant failed for wager={}, streak={}, fee_bps={}",
                wager, streak, fee_bps
            );
        }
    }
}

/// Test that payout equals net from breakdown.
/// Property test across various wager and streak combinations.
#[test]
fn payout_equals_breakdown_net() {
    let test_cases = vec![
        (1_000, 0, 300),
        (1_000_000, 1, 200),
        (1_000_000, 2, 300),
        (1_000_000, 3, 400),
        (1_000_000, 4, 500),
        (10_000_000, 1, 250),
        (100_000_000, 2, 350),
    ];

    for (wager, streak, fee_bps) in test_cases {
        let payout = calculate_payout(wager, streak, fee_bps);
        let breakdown = calculate_payout_breakdown(wager, streak, fee_bps);

        match (payout, breakdown) {
            (Some(p), Some((_, _, net, _))) => {
                assert_eq!(p, net, "payout should equal net from breakdown");
            }
            (None, None) => {
                // Both overflowed, which is expected
            }
            _ => {
                panic!("payout and breakdown should both succeed or both overflow");
            }
        }
    }
}

/// Test that fee is always non-negative.
/// Property test across various wager and streak combinations.
#[test]
fn payout_breakdown_fee_is_non_negative() {
    let test_cases = vec![
        (1_000, 0, 0),
        (1_000_000, 1, 200),
        (1_000_000, 2, 300),
        (1_000_000, 3, 400),
        (1_000_000, 4, 500),
        (10_000_000, 1, 250),
        (100_000_000, 2, 350),
    ];

    for (wager, streak, fee_bps) in test_cases {
        if let Some((_, fee, _, _)) = calculate_payout_breakdown(wager, streak, fee_bps) {
            assert!(fee >= 0, "fee should be non-negative");
        }
    }
}

/// Test that net is always non-negative.
/// Property test across various wager and streak combinations.
#[test]
fn payout_breakdown_net_is_non_negative() {
    let test_cases = vec![
        (1_000, 0, 0),
        (1_000_000, 1, 200),
        (1_000_000, 2, 300),
        (1_000_000, 3, 400),
        (1_000_000, 4, 500),
        (10_000_000, 1, 250),
        (100_000_000, 2, 350),
    ];

    for (wager, streak, fee_bps) in test_cases {
        if let Some((_, _, net, _)) = calculate_payout_breakdown(wager, streak, fee_bps) {
            assert!(net >= 0, "net should be non-negative");
        }
    }
}

// ── Large value arithmetic ──────────────────────────────────────────────────

/// Test payout calculation with large but safe wager.
/// Should succeed and return valid payout.
#[test]
fn payout_large_safe_wager_succeeds() {
    let large_wager = 1_000_000_000_000i128; // 1 trillion stroops
    let result = calculate_payout(large_wager, 1, 300);
    assert!(result.is_some(), "large safe wager should not overflow");
    let net = result.unwrap();
    assert!(net > 0, "net payout should be positive");
}

/// Test payout breakdown with large but safe wager.
/// Should succeed and return valid breakdown.
#[test]
fn payout_breakdown_large_safe_wager_succeeds() {
    let large_wager = 1_000_000_000_000i128; // 1 trillion stroops
    let result = calculate_payout_breakdown(large_wager, 1, 300);
    assert!(result.is_some(), "large safe wager should not overflow");
    let (gross, fee, net, _bonus) = result.unwrap();
    assert!(gross > 0, "gross should be positive");
    assert!(fee > 0, "fee should be positive");
    assert!(net > 0, "net should be positive");
}

/// Test payout calculation with near-overflow wager.
/// Should return None due to overflow.
#[test]
fn payout_near_overflow_wager_overflows() {
    let near_max = i128::MAX / 2;
    let result = calculate_payout(near_max, 4, 300);
    // At streak 4, multiplier is 10x, so near_max * 10 will overflow
    assert_eq!(result, None, "near-overflow wager should overflow at high streak");
}

// ── Reserve balance arithmetic ──────────────────────────────────────────────

/// Test reserve balance subtraction with large values.
/// Simulates reserve - payout arithmetic.
#[test]
fn reserve_balance_subtraction_large_values() {
    let reserve = 10_000_000_000_000i128; // 10 trillion stroops
    let payout = 1_000_000_000i128; // 1 billion stroops
    let remaining = reserve.checked_sub(payout);
    assert!(remaining.is_some(), "large reserve subtraction should not overflow");
    assert_eq!(remaining.unwrap(), 9_999_000_000_000i128);
}

/// Test reserve balance addition with large values.
/// Simulates reserve + wager arithmetic.
#[test]
fn reserve_balance_addition_large_values() {
    let reserve = 10_000_000_000_000i128; // 10 trillion stroops
    let wager = 1_000_000_000i128; // 1 billion stroops
    let new_reserve = reserve.checked_add(wager);
    assert!(new_reserve.is_some(), "large reserve addition should not overflow");
    assert_eq!(new_reserve.unwrap(), 10_001_000_000_000i128);
}

/// Test reserve balance addition overflow.
/// Should return None when adding near-max values.
#[test]
fn reserve_balance_addition_overflows() {
    let reserve = i128::MAX - 100;
    let wager = 1_000;
    let new_reserve = reserve.checked_add(wager);
    assert_eq!(new_reserve, None, "large reserve addition should overflow");
}
