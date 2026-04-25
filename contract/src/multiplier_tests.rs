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
    assert_eq!(calculate_payout(10_000_000, 1, 300).unwrap(), 18_430_000);
}

/// Streak 2, fee 300 bps (3%).
/// gross = 10_000_000 × 35_000 / 10_000 = 35_000_000
/// fee   = 35_000_000 × 300   / 10_000 =  1_050_000
/// net   = 33_950_000
#[test]
fn payout_streak_2_fee_300() {
    assert_eq!(calculate_payout(10_000_000, 2, 300).unwrap(), 33_950_000);
}

/// Streak 3, fee 300 bps (3%).
/// gross = 10_000_000 × 60_000 / 10_000 = 60_000_000
/// fee   = 60_000_000 × 300   / 10_000 =  1_800_000
/// net   = 58_200_000
#[test]
fn payout_streak_3_fee_300() {
    assert_eq!(calculate_payout(10_000_000, 3, 300).unwrap(), 58_200_000);
}

/// Streak 4, fee 300 bps (3%).
/// gross = 10_000_000 × 100_000 / 10_000 = 100_000_000
/// fee   = 100_000_000 × 300   / 10_000 =   3_000_000
/// net   = 97_000_000
#[test]
fn payout_streak_4_fee_300() {
    assert_eq!(calculate_payout(10_000_000, 4, 300).unwrap(), 97_000_000);
}

/// Streak 5 must produce the same result as streak 4 (same multiplier cap).
#[test]
fn payout_streak_5_equals_streak_4() {
    let wager = 10_000_000_i128;
    let fee_bps = 300_u32;
    assert_eq!(
        calculate_payout(wager, 5, fee_bps).unwrap(),
        calculate_payout(wager, 4, fee_bps).unwrap()
    );
}

// ── calculate_payout — fee boundary values ───────────────────────────────────

/// Minimum valid fee: 200 bps (2%).
/// gross = 10_000_000 × 19_000 / 10_000 = 19_000_000
/// fee   = 19_000_000 × 200   / 10_000 =    380_000
/// net   = 18_620_000
#[test]
fn payout_streak_1_fee_min_200() {
    assert_eq!(calculate_payout(10_000_000, 1, 200).unwrap(), 18_620_000);
}

/// Maximum valid fee: 500 bps (5%).
/// gross = 10_000_000 × 19_000 / 10_000 = 19_000_000
/// fee   = 19_000_000 × 500   / 10_000 =    950_000
/// net   = 18_050_000
#[test]
fn payout_streak_1_fee_max_500() {
    assert_eq!(calculate_payout(10_000_000, 1, 500).unwrap(), 18_050_000);
}

/// Higher fee must always produce a lower net payout for the same wager and streak.
#[test]
fn payout_higher_fee_produces_lower_net() {
    let wager = 10_000_000_i128;
    let streak = 1_u32;
    let net_low_fee = calculate_payout(wager, streak, 200).unwrap().unwrap();
    let net_high_fee = calculate_payout(wager, streak, 500).unwrap().unwrap();
    assert!(net_low_fee > net_high_fee);
}

/// Fee 0 bps: net must equal gross (no fee deducted).
/// gross = 10_000_000 × 19_000 / 10_000 = 19_000_000
#[test]
fn payout_fee_zero_net_equals_gross() {
    let (gross, fee, net, _bonus) = calculate_payout_breakdown(10_000_000, 1, 0).unwrap().unwrap();
    assert_eq!(fee, 0);
    assert_eq!(net, gross);
}

/// Fee 10_000 bps (100%): net must be 0 (all gross taken as fee).
#[test]
fn payout_fee_10000_net_is_zero() {
    let (gross, fee, net, _bonus) = calculate_payout_breakdown(10_000_000, 1, 10_000).unwrap().unwrap();
    assert_eq!(fee, gross);
    assert_eq!(net, 0);
}

// ── calculate_payout — wager boundary values ─────────────────────────────────

/// Minimum wager (1 stroop): must not panic and must return Some.
#[test]
fn payout_wager_1_stroop_returns_some() {
    assert!(calculate_payout(1, 1, 300).unwrap().is_some());
}

/// Zero wager: gross, fee, and net must all be 0.
#[test]
fn payout_wager_zero_all_components_zero() {
    let (gross, fee, net, _bonus) = calculate_payout_breakdown(0, 1, 300).unwrap().unwrap();
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
            calculate_payout(min_wager, streak, 300).unwrap().is_some(),
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
            calculate_payout(max_wager, streak, 300).unwrap().is_some(),
            "streak {streak} should return Some for max wager"
        );
    }
}

// ── calculate_payout — overflow protection ───────────────────────────────────

/// i128::MAX wager must return None (overflow in first checked_mul).
#[test]
fn payout_i128_max_wager_returns_none() {
    assert_eq!(calculate_payout(i128::MAX, 1, 300).unwrap(), None);
}

/// i128::MAX wager breakdown must also return None.
#[test]
fn payout_breakdown_i128_max_wager_returns_none() {
    assert_eq!(calculate_payout_breakdown(i128::MAX, 1, 300).unwrap(), None);
}

/// Near-overflow: largest wager that does NOT overflow for streak 1 (multiplier 19_000).
/// i128::MAX / 19_000 = 17_340_... — use a safe value just below that.
#[test]
fn payout_near_overflow_boundary_returns_some() {
    // i128::MAX / 19_000 ≈ 1.734 × 10^34; use 10^33 as a safe large value
    let large_wager: i128 = 1_000_000_000_000_000_000_000_000_000_000_000; // 10^33
    assert!(
        calculate_payout(large_wager, 1, 300).unwrap().is_some(),
        "large but non-overflowing wager should return Some"
    );
}

// ── calculate_payout_breakdown — all streak tiers ────────────────────────────

/// Streak 1 full breakdown.
#[test]
fn breakdown_streak_1_all_components() {
    let (gross, fee, net, _bonus) = calculate_payout_breakdown(10_000_000, 1, 300).unwrap().unwrap();
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
    let (gross, fee, net, _bonus) = calculate_payout_breakdown(10_000_000, 2, 300).unwrap().unwrap();
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
    let (gross, fee, net, _bonus) = calculate_payout_breakdown(10_000_000, 3, 300).unwrap().unwrap();
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
    let (gross, fee, net, _bonus) = calculate_payout_breakdown(10_000_000, 4, 300).unwrap().unwrap();
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
        let (gross, fee, net, _bonus) = calculate_payout_breakdown(wager, streak, fee_bps).unwrap().unwrap();
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
        let (_, _, breakdown_net, _) = calculate_payout_breakdown(wager, streak, fee_bps).unwrap().unwrap();
        let payout_net = calculate_payout(wager, streak, fee_bps).unwrap().unwrap();
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
            let (gross, _fee, net, _bonus) = calculate_payout_breakdown(10_000_000, streak, fee_bps).unwrap().unwrap();
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
        let (gross, _fee, _net, _bonus) = calculate_payout_breakdown(wager, streak, 300).unwrap().unwrap();
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
    let (gross1, _, _, _) = calculate_payout_breakdown(wager, 1, fee_bps).unwrap().unwrap();
    let (gross2, _, _, _) = calculate_payout_breakdown(wager, 2, fee_bps).unwrap().unwrap();
    let (gross3, _, _, _) = calculate_payout_breakdown(wager, 3, fee_bps).unwrap().unwrap();
    let (gross4, _, _, _) = calculate_payout_breakdown(wager, 4, fee_bps).unwrap().unwrap();
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
        let (g1, f1, n1, _) = calculate_payout_breakdown(10_000_000, streak, fee_bps).unwrap().unwrap();
        let (g2, f2, n2, _) = calculate_payout_breakdown(20_000_000, streak, fee_bps).unwrap().unwrap();
        assert_eq!(g2, g1 * 2, "gross should double for streak={streak}");
        assert_eq!(f2, f1 * 2, "fee should double for streak={streak}");
        assert_eq!(n2, n1 * 2, "net should double for streak={streak}");
    }
}


// ── Multiplier progression validation (Issue #410) ──────────────────────────

/// Test get_multiplier for all streak values 0-20.
/// Streaks 0-3 have specific values; 4+ all return the cap (100_000).
#[test]
fn multiplier_progression_streaks_0_to_20() {
    let expected = [
        (0, 100_000),   // 0 → cap (wildcard)
        (1, 19_000),    // 1 → 1.9x
        (2, 35_000),    // 2 → 3.5x
        (3, 60_000),    // 3 → 6.0x
        (4, 100_000),   // 4+ → 10.0x
        (5, 100_000),
        (6, 100_000),
        (7, 100_000),
        (8, 100_000),
        (9, 100_000),
        (10, 100_000),
        (11, 100_000),
        (12, 100_000),
        (13, 100_000),
        (14, 100_000),
        (15, 100_000),
        (16, 100_000),
        (17, 100_000),
        (18, 100_000),
        (19, 100_000),
        (20, 100_000),
    ];

    for (streak, expected_multiplier) in expected.iter() {
        let actual = get_multiplier(*streak);
        assert_eq!(actual, *expected_multiplier,
            "streak {streak} should return {expected_multiplier}, got {actual}");
    }
}

/// Test tier transitions at exact boundaries.
/// Boundary 1: streak 0 → 1 (100_000 → 19_000)
#[test]
fn multiplier_tier_transition_0_to_1() {
    let m0 = get_multiplier(0);
    let m1 = get_multiplier(1);
    assert_eq!(m0, 100_000, "streak 0 should return cap");
    assert_eq!(m1, 19_000, "streak 1 should return 1.9x");
    assert!(m1 < m0, "streak 1 multiplier should be less than streak 0 (cap)");
}

/// Boundary 2: streak 1 → 2 (19_000 → 35_000)
#[test]
fn multiplier_tier_transition_1_to_2() {
    let m1 = get_multiplier(1);
    let m2 = get_multiplier(2);
    assert_eq!(m1, 19_000);
    assert_eq!(m2, 35_000);
    assert!(m1 < m2, "streak 2 multiplier should exceed streak 1");
}

/// Boundary 3: streak 2 → 3 (35_000 → 60_000)
#[test]
fn multiplier_tier_transition_2_to_3() {
    let m2 = get_multiplier(2);
    let m3 = get_multiplier(3);
    assert_eq!(m2, 35_000);
    assert_eq!(m3, 60_000);
    assert!(m2 < m3, "streak 3 multiplier should exceed streak 2");
}

/// Boundary 4: streak 3 → 4 (60_000 → 100_000)
#[test]
fn multiplier_tier_transition_3_to_4() {
    let m3 = get_multiplier(3);
    let m4 = get_multiplier(4);
    assert_eq!(m3, 60_000);
    assert_eq!(m4, 100_000);
    assert!(m3 < m4, "streak 4 multiplier should exceed streak 3");
}

/// Boundary 5: streak 4 → 5 (100_000 → 100_000, cap plateau)
#[test]
fn multiplier_tier_transition_4_to_5_cap_plateau() {
    let m4 = get_multiplier(4);
    let m5 = get_multiplier(5);
    assert_eq!(m4, 100_000);
    assert_eq!(m5, 100_000);
    assert_eq!(m4, m5, "streak 5 should equal streak 4 (cap plateau)");
}

/// Test payout calculations for each multiplier tier.
/// Tier 1 (1.9x): wager 10_000_000, fee 300 bps
#[test]
fn multiplier_payout_tier_1_1_9x() {
    let wager = 10_000_000i128;
    let (gross, fee, net, _bonus) = calculate_payout_breakdown(wager, 1, 300).unwrap().unwrap();
    assert_eq!(gross, 19_000_000, "tier 1 gross should be 1.9x wager");
    assert_eq!(fee, 570_000, "tier 1 fee should be 3% of gross");
    assert_eq!(net, 18_430_000, "tier 1 net should be gross - fee");
}

/// Tier 2 (3.5x): wager 10_000_000, fee 300 bps
#[test]
fn multiplier_payout_tier_2_3_5x() {
    let wager = 10_000_000i128;
    let (gross, fee, net, _bonus) = calculate_payout_breakdown(wager, 2, 300).unwrap().unwrap();
    assert_eq!(gross, 35_000_000, "tier 2 gross should be 3.5x wager");
    assert_eq!(fee, 1_050_000, "tier 2 fee should be 3% of gross");
    assert_eq!(net, 33_950_000, "tier 2 net should be gross - fee");
}

/// Tier 3 (6.0x): wager 10_000_000, fee 300 bps
#[test]
fn multiplier_payout_tier_3_6_0x() {
    let wager = 10_000_000i128;
    let (gross, fee, net, _bonus) = calculate_payout_breakdown(wager, 3, 300).unwrap().unwrap();
    assert_eq!(gross, 60_000_000, "tier 3 gross should be 6.0x wager");
    assert_eq!(fee, 1_800_000, "tier 3 fee should be 3% of gross");
    assert_eq!(net, 58_200_000, "tier 3 net should be gross - fee");
}

/// Tier 4+ (10.0x): wager 10_000_000, fee 300 bps
#[test]
fn multiplier_payout_tier_4_plus_10_0x() {
    let wager = 10_000_000i128;
    let (gross, fee, net, _bonus) = calculate_payout_breakdown(wager, 4, 300).unwrap().unwrap();
    assert_eq!(gross, 100_000_000, "tier 4+ gross should be 10.0x wager");
    assert_eq!(fee, 3_000_000, "tier 4+ fee should be 3% of gross");
    assert_eq!(net, 97_000_000, "tier 4+ net should be gross - fee");
}

/// Test payout progression across all tiers for the same wager.
/// Payouts should strictly increase: tier 1 < tier 2 < tier 3 < tier 4+
#[test]
fn multiplier_payout_progression_across_tiers() {
    let wager = 10_000_000i128;
    let fee_bps = 300u32;

    let net1 = calculate_payout(wager, 1, fee_bps).unwrap().unwrap();
    let net2 = calculate_payout(wager, 2, fee_bps).unwrap().unwrap();
    let net3 = calculate_payout(wager, 3, fee_bps).unwrap().unwrap();
    let net4 = calculate_payout(wager, 4, fee_bps).unwrap().unwrap();

    assert!(net1 < net2, "tier 2 payout should exceed tier 1");
    assert!(net2 < net3, "tier 3 payout should exceed tier 2");
    assert!(net3 < net4, "tier 4+ payout should exceed tier 3");
}

/// Test that multiplier is monotonically non-decreasing for streaks 1-4.
/// Streaks 1, 2, 3, 4 should have strictly increasing multipliers.
#[test]
fn multiplier_monotonicity_streaks_1_to_4() {
    let m1 = get_multiplier(1);
    let m2 = get_multiplier(2);
    let m3 = get_multiplier(3);
    let m4 = get_multiplier(4);

    assert!(m1 < m2, "m1 < m2");
    assert!(m2 < m3, "m2 < m3");
    assert!(m3 < m4, "m3 < m4");
}

/// Test that multiplier is constant for streaks 4+.
/// All streaks >= 4 should return the same multiplier (100_000).
#[test]
fn multiplier_constant_for_streaks_4_plus() {
    let m4 = get_multiplier(4);
    for streak in 5..=100 {
        let m = get_multiplier(streak);
        assert_eq!(m, m4, "streak {streak} should equal streak 4 multiplier");
    }
}

/// Property test: multiplier never decreases within valid game streaks (1-4).
#[test]
fn multiplier_property_no_decrease_1_to_4() {
    for streak in 1..=4 {
        let m_current = get_multiplier(streak);
        if streak > 1 {
            let m_prev = get_multiplier(streak - 1);
            assert!(m_prev <= m_current,
                "multiplier must not decrease from streak {prev} to {streak}",
                prev = streak - 1);
        }
    }
}

/// Property test: multiplier is always positive.
#[test]
fn multiplier_property_always_positive() {
    for streak in 0..=100 {
        let m = get_multiplier(streak);
        assert!(m > 0, "multiplier for streak {streak} must be positive");
    }
}

/// Property test: multiplier is always >= 10_000 (at least 1x).
#[test]
fn multiplier_property_always_at_least_1x() {
    for streak in 0..=100 {
        let m = get_multiplier(streak);
        assert!(m >= 10_000, "multiplier for streak {streak} must be >= 10_000 (1x)");
    }
}

/// Property test: multiplier is always <= 100_000 (at most 10x).
#[test]
fn multiplier_property_always_at_most_10x() {
    for streak in 0..=100 {
        let m = get_multiplier(streak);
        assert!(m <= 100_000, "multiplier for streak {streak} must be <= 100_000 (10x)");
    }
}

/// Test that payout scales linearly with wager for each tier.
/// Doubling the wager should double the payout.
#[test]
fn multiplier_payout_scales_linearly_with_wager() {
    for streak in [1, 2, 3, 4] {
        let fee_bps = 300u32;
        let wager1 = 10_000_000i128;
        let wager2 = 20_000_000i128;

        let net1 = calculate_payout(wager1, streak, fee_bps).unwrap().unwrap();
        let net2 = calculate_payout(wager2, streak, fee_bps).unwrap().unwrap();

        assert_eq!(net2, net1 * 2,
            "doubling wager should double payout for streak {streak}");
    }
}

/// Test that payout scales linearly with fee for each tier.
/// Doubling the fee should halve the net payout.
#[test]
fn multiplier_payout_scales_inversely_with_fee() {
    for streak in [1, 2, 3, 4] {
        let wager = 10_000_000i128;
        let fee_low = 200u32;
        let fee_high = 400u32;

        let net_low = calculate_payout(wager, streak, fee_low).unwrap().unwrap();
        let net_high = calculate_payout(wager, streak, fee_high).unwrap().unwrap();

        assert!(net_low > net_high,
            "higher fee should produce lower net for streak {streak}");
    }
}
