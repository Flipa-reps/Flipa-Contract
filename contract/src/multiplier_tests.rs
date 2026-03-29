/// Comprehensive unit tests for `get_multiplier` and `calculate_payout` /
/// `calculate_payout_breakdown`.
///
/// These tests are additive to the baseline smoke tests in `lib.rs` and focus on:
/// - All four streak tiers with exact expected values
/// - All valid fee boundary values (200 bps min, 500 bps max)
/// - Minimum and maximum wager boundary values
/// - Arithmetic invariants (gross − fee == net, breakdown net == payout net)
/// - Overflow protection via checked arithmetic
/// - Edge cases: zero wager, near-overflow wager, streak 0, streak u32::MAX
///
/// # References
/// - Issue #338: Add unit tests for multiplier calculation logic
/// - Contract constants: MULTIPLIER_STREAK_1=19_000, _2=35_000, _3=60_000, _4+=100_000
/// - Fee range: 200–500 bps (2–5%)
use super::{calculate_payout, calculate_payout_breakdown, get_multiplier};

// ── get_multiplier ────────────────────────────────────────────────────────────

/// Streak 1 must return exactly 19_000 bps (1.9×).
#[test]
fn multiplier_streak_1_is_19000() {
    assert_eq!(get_multiplier(1), 19_000);
}

/// Streak 2 must return exactly 35_000 bps (3.5×).
#[test]
fn multiplier_streak_2_is_35000() {
    assert_eq!(get_multiplier(2), 35_000);
}

/// Streak 3 must return exactly 60_000 bps (6.0×).
#[test]
fn multiplier_streak_3_is_60000() {
    assert_eq!(get_multiplier(3), 60_000);
}

/// Streak 4 must return exactly 100_000 bps (10.0×).
#[test]
fn multiplier_streak_4_is_100000() {
    assert_eq!(get_multiplier(4), 100_000);
}

/// Streaks 5–10 must all return the 10× cap.
#[test]
fn multiplier_streak_5_through_10_all_return_cap() {
    for streak in 5..=10 {
        assert_eq!(
            get_multiplier(streak),
            100_000,
            "streak {streak} should return 100_000"
        );
    }
}

/// Streak u32::MAX must not panic and must return the 10× cap.
#[test]
fn multiplier_streak_u32_max_returns_cap() {
    assert_eq!(get_multiplier(u32::MAX), 100_000);
}

/// Streak 0 is not a valid game state but must not panic.
/// The wildcard arm returns the 10× cap.
#[test]
fn multiplier_streak_0_does_not_panic() {
    assert_eq!(get_multiplier(0), 100_000);
}

/// Multiplier values must be strictly increasing across the four tiers.
#[test]
fn multiplier_values_are_strictly_increasing() {
    assert!(get_multiplier(1) < get_multiplier(2));
    assert!(get_multiplier(2) < get_multiplier(3));
    assert!(get_multiplier(3) < get_multiplier(4));
}

/// All multipliers must be > 10_000 (i.e. > 1×) — the house always pays more than the wager.
#[test]
fn multiplier_always_greater_than_1x() {
    for streak in [1, 2, 3, 4, 5, u32::MAX] {
        assert!(
            get_multiplier(streak) > 10_000,
            "streak {streak}: multiplier must be > 10_000 (1×)"
        );
    }
}

// ── calculate_payout — all streak tiers ──────────────────────────────────────

/// Streak 1, fee 300 bps (3%).
/// gross = 10_000_000 × 19_000 / 10_000 = 19_000_000
/// fee   = 19_000_000 × 300   / 10_000 =    570_000
/// net   = 18_430_000
#[test]
fn payout_streak_1_fee_300() {
    assert_eq!(calculate_payout(10_000_000, 1, 300), Some(18_430_000));
}

/// Streak 2, fee 300 bps (3%).
/// gross = 10_000_000 × 35_000 / 10_000 = 35_000_000
/// fee   = 35_000_000 × 300   / 10_000 =  1_050_000
/// net   = 33_950_000
#[test]
fn payout_streak_2_fee_300() {
    assert_eq!(calculate_payout(10_000_000, 2, 300), Some(33_950_000));
}

/// Streak 3, fee 300 bps (3%).
/// gross = 10_000_000 × 60_000 / 10_000 = 60_000_000
/// fee   = 60_000_000 × 300   / 10_000 =  1_800_000
/// net   = 58_200_000
#[test]
fn payout_streak_3_fee_300() {
    assert_eq!(calculate_payout(10_000_000, 3, 300), Some(58_200_000));
}

/// Streak 4, fee 300 bps (3%).
/// gross = 10_000_000 × 100_000 / 10_000 = 100_000_000
/// fee   = 100_000_000 × 300   / 10_000 =   3_000_000
/// net   = 97_000_000
#[test]
fn payout_streak_4_fee_300() {
    assert_eq!(calculate_payout(10_000_000, 4, 300), Some(97_000_000));
}

/// Streak 5 must produce the same result as streak 4 (same multiplier cap).
#[test]
fn payout_streak_5_equals_streak_4() {
    let wager = 10_000_000_i128;
    let fee_bps = 300_u32;
    assert_eq!(
        calculate_payout(wager, 5, fee_bps),
        calculate_payout(wager, 4, fee_bps)
    );
}

// ── calculate_payout — fee boundary values ───────────────────────────────────

/// Minimum valid fee: 200 bps (2%).
/// gross = 10_000_000 × 19_000 / 10_000 = 19_000_000
/// fee   = 19_000_000 × 200   / 10_000 =    380_000
/// net   = 18_620_000
#[test]
fn payout_streak_1_fee_min_200() {
    assert_eq!(calculate_payout(10_000_000, 1, 200), Some(18_620_000));
}

/// Maximum valid fee: 500 bps (5%).
/// gross = 10_000_000 × 19_000 / 10_000 = 19_000_000
/// fee   = 19_000_000 × 500   / 10_000 =    950_000
/// net   = 18_050_000
#[test]
fn payout_streak_1_fee_max_500() {
    assert_eq!(calculate_payout(10_000_000, 1, 500), Some(18_050_000));
}

/// Higher fee must always produce a lower net payout for the same wager and streak.
#[test]
fn payout_higher_fee_produces_lower_net() {
    let wager = 10_000_000_i128;
    let streak = 1_u32;
    let net_low_fee = calculate_payout(wager, streak, 200).unwrap();
    let net_high_fee = calculate_payout(wager, streak, 500).unwrap();
    assert!(net_low_fee > net_high_fee);
}

/// Fee 0 bps: net must equal gross (no fee deducted).
/// gross = 10_000_000 × 19_000 / 10_000 = 19_000_000
#[test]
fn payout_fee_zero_net_equals_gross() {
    let (gross, fee, net) = calculate_payout_breakdown(10_000_000, 1, 0).unwrap();
    assert_eq!(fee, 0);
    assert_eq!(net, gross);
}

/// Fee 10_000 bps (100%): net must be 0 (all gross taken as fee).
#[test]
fn payout_fee_10000_net_is_zero() {
    let (gross, fee, net) = calculate_payout_breakdown(10_000_000, 1, 10_000).unwrap();
    assert_eq!(fee, gross);
    assert_eq!(net, 0);
}

// ── calculate_payout — wager boundary values ─────────────────────────────────

/// Minimum wager (1 stroop): must not panic and must return Some.
#[test]
fn payout_wager_1_stroop_returns_some() {
    assert!(calculate_payout(1, 1, 300).is_some());
}

/// Zero wager: gross, fee, and net must all be 0.
#[test]
fn payout_wager_zero_all_components_zero() {
    let (gross, fee, net) = calculate_payout_breakdown(0, 1, 300).unwrap();
    assert_eq!(gross, 0);
    assert_eq!(fee, 0);
    assert_eq!(net, 0);
}

/// Typical minimum wager (1 XLM = 10_000_000 stroops), all streak tiers.
#[test]
fn payout_min_wager_all_streaks_return_some() {
    let min_wager = 10_000_000_i128; // 1 XLM
    for streak in [1, 2, 3, 4] {
        assert!(
            calculate_payout(min_wager, streak, 300).is_some(),
            "streak {streak} should return Some for min wager"
        );
    }
}

/// Typical maximum wager (10_000 XLM = 100_000_000_000_000 stroops), all streak tiers.
#[test]
fn payout_max_wager_all_streaks_return_some() {
    let max_wager = 100_000_000_000_000_i128; // 10_000 XLM
    for streak in [1, 2, 3, 4] {
        assert!(
            calculate_payout(max_wager, streak, 300).is_some(),
            "streak {streak} should return Some for max wager"
        );
    }
}

// ── calculate_payout — overflow protection ───────────────────────────────────

/// i128::MAX wager must return None (overflow in first checked_mul).
#[test]
fn payout_i128_max_wager_returns_none() {
    assert_eq!(calculate_payout(i128::MAX, 1, 300), None);
}

/// i128::MAX wager breakdown must also return None.
#[test]
fn payout_breakdown_i128_max_wager_returns_none() {
    assert_eq!(calculate_payout_breakdown(i128::MAX, 1, 300), None);
}

/// Near-overflow: largest wager that does NOT overflow for streak 1 (multiplier 19_000).
/// i128::MAX / 19_000 = 17_340_... — use a safe value just below that.
#[test]
fn payout_near_overflow_boundary_returns_some() {
    // i128::MAX / 19_000 ≈ 1.734 × 10^34; use 10^33 as a safe large value
    let large_wager: i128 = 1_000_000_000_000_000_000_000_000_000_000_000; // 10^33
    assert!(
        calculate_payout(large_wager, 1, 300).is_some(),
        "large but non-overflowing wager should return Some"
    );
}

// ── calculate_payout_breakdown — all streak tiers ────────────────────────────

/// Streak 1 full breakdown.
#[test]
fn breakdown_streak_1_all_components() {
    let (gross, fee, net) = calculate_payout_breakdown(10_000_000, 1, 300).unwrap();
    assert_eq!(gross, 19_000_000);
    assert_eq!(fee, 570_000);
    assert_eq!(net, 18_430_000);
}

/// Streak 2 full breakdown.
/// gross = 10_000_000 × 35_000 / 10_000 = 35_000_000
/// fee   = 35_000_000 × 300   / 10_000 =  1_050_000
/// net   = 33_950_000
#[test]
fn breakdown_streak_2_all_components() {
    let (gross, fee, net) = calculate_payout_breakdown(10_000_000, 2, 300).unwrap();
    assert_eq!(gross, 35_000_000);
    assert_eq!(fee, 1_050_000);
    assert_eq!(net, 33_950_000);
}

/// Streak 3 full breakdown.
/// gross = 10_000_000 × 60_000 / 10_000 = 60_000_000
/// fee   = 60_000_000 × 300   / 10_000 =  1_800_000
/// net   = 58_200_000
#[test]
fn breakdown_streak_3_all_components() {
    let (gross, fee, net) = calculate_payout_breakdown(10_000_000, 3, 300).unwrap();
    assert_eq!(gross, 60_000_000);
    assert_eq!(fee, 1_800_000);
    assert_eq!(net, 58_200_000);
}

/// Streak 4 full breakdown.
/// gross = 10_000_000 × 100_000 / 10_000 = 100_000_000
/// fee   = 100_000_000 × 300   / 10_000 =   3_000_000
/// net   = 97_000_000
#[test]
fn breakdown_streak_4_all_components() {
    let (gross, fee, net) = calculate_payout_breakdown(10_000_000, 4, 300).unwrap();
    assert_eq!(gross, 100_000_000);
    assert_eq!(fee, 3_000_000);
    assert_eq!(net, 97_000_000);
}

// ── Arithmetic invariants ─────────────────────────────────────────────────────

/// Invariant: gross − fee == net for every (wager, streak, fee_bps) combination.
#[test]
fn invariant_gross_minus_fee_equals_net() {
    let cases: &[(i128, u32, u32)] = &[
        (10_000_000, 1, 200),
        (10_000_000, 1, 300),
        (10_000_000, 1, 500),
        (10_000_000, 2, 200),
        (10_000_000, 2, 300),
        (10_000_000, 2, 500),
        (10_000_000, 3, 200),
        (10_000_000, 3, 300),
        (10_000_000, 3, 500),
        (10_000_000, 4, 200),
        (10_000_000, 4, 300),
        (10_000_000, 4, 500),
        (1, 1, 300),
        (0, 1, 300),
        (5_000_000, 2, 400),
    ];
    for &(wager, streak, fee_bps) in cases {
        let (gross, fee, net) = calculate_payout_breakdown(wager, streak, fee_bps).unwrap();
        assert_eq!(
            gross - fee,
            net,
            "gross({gross}) - fee({fee}) != net({net}) for wager={wager} streak={streak} fee_bps={fee_bps}"
        );
    }
}

/// Invariant: `calculate_payout` net must equal `calculate_payout_breakdown` net.
#[test]
fn invariant_payout_net_matches_breakdown_net() {
    let cases: &[(i128, u32, u32)] = &[
        (10_000_000, 1, 200),
        (10_000_000, 2, 300),
        (10_000_000, 3, 400),
        (10_000_000, 4, 500),
        (1_000_000, 4, 200),
        (50_000_000, 3, 300),
    ];
    for &(wager, streak, fee_bps) in cases {
        let (_, _, breakdown_net) = calculate_payout_breakdown(wager, streak, fee_bps).unwrap();
        let payout_net = calculate_payout(wager, streak, fee_bps).unwrap();
        assert_eq!(
            payout_net,
            breakdown_net,
            "payout net mismatch for wager={wager} streak={streak} fee_bps={fee_bps}"
        );
    }
}

/// Invariant: net must always be <= gross (fee is never negative).
#[test]
fn invariant_net_never_exceeds_gross() {
    for streak in [1, 2, 3, 4] {
        for fee_bps in [200, 300, 400, 500] {
            let (gross, _fee, net) = calculate_payout_breakdown(10_000_000, streak, fee_bps).unwrap();
            assert!(
                net <= gross,
                "net({net}) > gross({gross}) for streak={streak} fee_bps={fee_bps}"
            );
        }
    }
}

/// Invariant: gross must always be > wager (multiplier is always > 1×).
#[test]
fn invariant_gross_always_greater_than_wager() {
    let wager = 10_000_000_i128;
    for streak in [1, 2, 3, 4] {
        let (gross, _fee, _net) = calculate_payout_breakdown(wager, streak, 300).unwrap();
        assert!(
            gross > wager,
            "gross({gross}) should be > wager({wager}) for streak={streak}"
        );
    }
}

/// Invariant: higher streak must always produce higher gross for the same wager.
#[test]
fn invariant_higher_streak_produces_higher_gross() {
    let wager = 10_000_000_i128;
    let fee_bps = 300_u32;
    let (gross1, _, _) = calculate_payout_breakdown(wager, 1, fee_bps).unwrap();
    let (gross2, _, _) = calculate_payout_breakdown(wager, 2, fee_bps).unwrap();
    let (gross3, _, _) = calculate_payout_breakdown(wager, 3, fee_bps).unwrap();
    let (gross4, _, _) = calculate_payout_breakdown(wager, 4, fee_bps).unwrap();
    assert!(gross1 < gross2);
    assert!(gross2 < gross3);
    assert!(gross3 < gross4);
}

/// Invariant: doubling the wager must exactly double gross, fee, and net
/// (linear scaling — no rounding artifacts at these values).
#[test]
fn invariant_payout_scales_linearly_with_wager() {
    let fee_bps = 300_u32;
    for streak in [1, 2, 3, 4] {
        let (g1, f1, n1) = calculate_payout_breakdown(10_000_000, streak, fee_bps).unwrap();
        let (g2, f2, n2) = calculate_payout_breakdown(20_000_000, streak, fee_bps).unwrap();
        assert_eq!(g2, g1 * 2, "gross should double for streak={streak}");
        assert_eq!(f2, f1 * 2, "fee should double for streak={streak}");
        assert_eq!(n2, n1 * 2, "net should double for streak={streak}");
    }
}
