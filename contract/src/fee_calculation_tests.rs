/// Comprehensive unit tests for fee calculation edge cases and boundary conditions.
///
/// # Coverage
/// - Fee at 2% (200 bps) minimum boundary
/// - Fee at 5% (500 bps) maximum boundary
/// - Very small wager amounts (1 stroop)
/// - Very large wager amounts (near overflow)
/// - Rounding / truncation behavior (integer division floors)
/// - Fee never exceeds configured percentage
/// - Fee calculation across all four multiplier tiers (streak 1–4+)
///
/// # Accounting formulas (mirrors contract logic)
///   gross = wager × multiplier_bps / 10_000
///   fee   = gross × fee_bps / 10_000
///   net   = gross − fee
///
/// # References
/// - Issue: add-unit-tests-fee-calculation-edge-cases
/// - Contract constants: MULTIPLIER_STREAK_1=19_000, _2=35_000, _3=60_000, _4+=100_000
/// - Valid fee range: 200–500 bps (2–5%)
use super::{calculate_payout, calculate_payout_breakdown};

// ── Fee boundary: 2% (200 bps) minimum ───────────────────────────────────────

/// At the minimum fee boundary (200 bps), fee = gross × 200 / 10_000 = gross × 2%.
/// gross = 10_000_000 × 19_000 / 10_000 = 19_000_000
/// fee   = 19_000_000 × 200   / 10_000 =    380_000
/// net   = 18_620_000
#[test]
fn fee_min_200bps_streak1_exact_value() {
    let (gross, fee, net) = calculate_payout_breakdown(10_000_000, 1, 200).unwrap().unwrap();
    assert_eq!(gross, 19_000_000);
    assert_eq!(fee, 380_000);
    assert_eq!(net, 18_620_000);
}

/// At 200 bps, fee is exactly 2% of gross — no more, no less.
#[test]
fn fee_min_200bps_is_exactly_2_percent_of_gross() {
    for streak in [1, 2, 3, 4] {
        let (gross, fee, _) = calculate_payout_breakdown(10_000_000, streak, 200).unwrap().unwrap();
        // fee / gross == 200 / 10_000 == 1/50
        assert_eq!(fee * 10_000, gross * 200,
            "fee must be exactly 2% of gross at streak {streak}");
    }
}

/// At 200 bps, net = gross × (1 − 0.02) = gross × 0.98.
#[test]
fn fee_min_200bps_net_is_98_percent_of_gross() {
    let (gross, _, net) = calculate_payout_breakdown(10_000_000, 1, 200).unwrap().unwrap();
    // net * 10_000 == gross * 9_800
    assert_eq!(net * 10_000, gross * 9_800);
}

// ── Fee boundary: 5% (500 bps) maximum ───────────────────────────────────────

/// At the maximum fee boundary (500 bps), fee = gross × 500 / 10_000 = gross × 5%.
/// gross = 10_000_000 × 19_000 / 10_000 = 19_000_000
/// fee   = 19_000_000 × 500   / 10_000 =    950_000
/// net   = 18_050_000
#[test]
fn fee_max_500bps_streak1_exact_value() {
    let (gross, fee, net) = calculate_payout_breakdown(10_000_000, 1, 500).unwrap().unwrap();
    assert_eq!(gross, 19_000_000);
    assert_eq!(fee, 950_000);
    assert_eq!(net, 18_050_000);
}

/// At 500 bps, fee is exactly 5% of gross.
#[test]
fn fee_max_500bps_is_exactly_5_percent_of_gross() {
    for streak in [1, 2, 3, 4] {
        let (gross, fee, _) = calculate_payout_breakdown(10_000_000, streak, 500).unwrap().unwrap();
        assert_eq!(fee * 10_000, gross * 500,
            "fee must be exactly 5% of gross at streak {streak}");
    }
}

/// At 500 bps, net = gross × 0.95.
#[test]
fn fee_max_500bps_net_is_95_percent_of_gross() {
    let (gross, _, net) = calculate_payout_breakdown(10_000_000, 1, 500).unwrap().unwrap();
    assert_eq!(net * 10_000, gross * 9_500);
}

/// 500 bps always produces a lower net than 200 bps for the same wager and streak.
#[test]
fn fee_max_always_lower_net_than_fee_min() {
    for streak in [1, 2, 3, 4] {
        let net_min = calculate_payout(10_000_000, streak, 200).unwrap().unwrap();
        let net_max = calculate_payout(10_000_000, streak, 500).unwrap().unwrap();
        assert!(net_min > net_max,
            "net at 200 bps must exceed net at 500 bps for streak {streak}");
    }
}

// ── Fee with very small wager amounts ────────────────────────────────────────

/// Wager of 1 stroop: all components must be Some (no panic, no None).
#[test]
fn fee_wager_1_stroop_returns_some() {
    assert!(calculate_payout_breakdown(1, 1, 200).unwrap().is_some());
    assert!(calculate_payout_breakdown(1, 1, 500).unwrap().is_some());
}

/// Wager of 1 stroop at streak 1, fee 200 bps.
/// gross = 1 × 19_000 / 10_000 = 1  (integer division floors 1.9 → 1)
/// fee   = 1 × 200   / 10_000 = 0   (floors 0.02 → 0)
/// net   = 1
#[test]
fn fee_wager_1_stroop_streak1_fee200_floors_correctly() {
    let (gross, fee, net) = calculate_payout_breakdown(1, 1, 200).unwrap().unwrap();
    assert_eq!(gross, 1);
    assert_eq!(fee, 0);   // 1 × 200 / 10_000 = 0 (truncated)
    assert_eq!(net, 1);
}

/// Wager of 1 stroop at streak 4 (10x), fee 500 bps.
/// gross = 1 × 100_000 / 10_000 = 10
/// fee   = 10 × 500   / 10_000 = 0  (floors 0.5 → 0)
/// net   = 10
#[test]
fn fee_wager_1_stroop_streak4_fee500_floors_correctly() {
    let (gross, fee, net) = calculate_payout_breakdown(1, 4, 500).unwrap().unwrap();
    assert_eq!(gross, 10);
    assert_eq!(fee, 0);   // 10 × 500 / 10_000 = 0 (truncated)
    assert_eq!(net, 10);
}

/// Wager of 0 stroops: gross, fee, and net must all be 0.
#[test]
fn fee_wager_zero_all_components_zero() {
    let (gross, fee, net) = calculate_payout_breakdown(0, 1, 300).unwrap().unwrap();
    assert_eq!(gross, 0);
    assert_eq!(fee, 0);
    assert_eq!(net, 0);
}

/// Minimum contract wager (1_000_000 stroops = 0.1 XLM) at both fee boundaries.
#[test]
fn fee_min_contract_wager_both_fee_boundaries() {
    let wager = 1_000_000i128;
    // 200 bps: gross=1_900_000, fee=38_000, net=1_862_000
    let (g200, f200, n200) = calculate_payout_breakdown(wager, 1, 200).unwrap().unwrap();
    assert_eq!(g200, 1_900_000);
    assert_eq!(f200, 38_000);
    assert_eq!(n200, 1_862_000);
    // 500 bps: gross=1_900_000, fee=95_000, net=1_805_000
    let (g500, f500, n500) = calculate_payout_breakdown(wager, 1, 500).unwrap().unwrap();
    assert_eq!(g500, 1_900_000);
    assert_eq!(f500, 95_000);
    assert_eq!(n500, 1_805_000);
}

// ── Fee with very large wager amounts ────────────────────────────────────────

/// Maximum contract wager (100_000_000 stroops = 10 XLM) at both fee boundaries.
#[test]
fn fee_max_contract_wager_both_fee_boundaries() {
    let wager = 100_000_000i128;
    // 200 bps: gross=190_000_000, fee=3_800_000, net=186_200_000
    let (g200, f200, n200) = calculate_payout_breakdown(wager, 1, 200).unwrap().unwrap();
    assert_eq!(g200, 190_000_000);
    assert_eq!(f200, 3_800_000);
    assert_eq!(n200, 186_200_000);
    // 500 bps: gross=190_000_000, fee=9_500_000, net=180_500_000
    let (g500, f500, n500) = calculate_payout_breakdown(wager, 1, 500).unwrap().unwrap();
    assert_eq!(g500, 190_000_000);
    assert_eq!(f500, 9_500_000);
    assert_eq!(n500, 180_500_000);
}

/// Large wager (10_000 XLM = 100_000_000_000_000 stroops) must return Some for all streaks.
#[test]
fn fee_large_wager_all_streaks_return_some() {
    let wager = 100_000_000_000_000i128; // 10_000 XLM
    for streak in [1, 2, 3, 4] {
        assert!(calculate_payout_breakdown(wager, streak, 300).unwrap().is_some(),
            "streak {streak} must return Some for large wager");
    }
}

/// i128::MAX wager must return None (overflow protection).
#[test]
fn fee_i128_max_wager_returns_none() {
    assert!(calculate_payout_breakdown(i128::MAX, 1, 200).unwrap().is_none());
    assert!(calculate_payout_breakdown(i128::MAX, 1, 500).unwrap().is_none());
}

/// Near-overflow wager (10^33) must return Some for streak 1 (multiplier 1.9x).
/// i128::MAX / 19_000 ≈ 1.734 × 10^34; 10^33 is safely below that.
#[test]
fn fee_near_overflow_wager_returns_some() {
    let large: i128 = 1_000_000_000_000_000_000_000_000_000_000_000; // 10^33
    assert!(calculate_payout_breakdown(large, 1, 300).unwrap().is_some());
}

// ── Rounding / truncation behavior ───────────────────────────────────────────

/// Integer division always floors (truncates toward zero).
/// Wager chosen so gross × fee_bps is not divisible by 10_000.
///
/// wager=1, streak=4 → gross = 1 × 100_000 / 10_000 = 10
/// fee at 300 bps: 10 × 300 / 10_000 = 3000 / 10_000 = 0  (floors)
#[test]
fn fee_rounding_floors_toward_zero() {
    let (_, fee, _) = calculate_payout_breakdown(1, 4, 300).unwrap().unwrap();
    assert_eq!(fee, 0, "fee must floor to 0 when gross × fee_bps < 10_000");
}

/// Rounding invariant: fee + net == gross (no stroop is lost or created).
#[test]
fn fee_rounding_gross_equals_fee_plus_net() {
    // Use a wager that produces a non-divisible intermediate to exercise rounding.
    let cases: &[(i128, u32, u32)] = &[
        (1, 1, 300),
        (3, 2, 200),
        (7, 3, 500),
        (11, 4, 300),
        (999, 1, 200),
        (1_000_001, 2, 300),
        (9_999_999, 3, 500),
    ];
    for &(wager, streak, fee_bps) in cases {
        let (gross, fee, net) = calculate_payout_breakdown(wager, streak, fee_bps).unwrap().unwrap();
        assert_eq!(gross, fee + net,
            "gross({gross}) must equal fee({fee}) + net({net}) for wager={wager} streak={streak} fee_bps={fee_bps}");
    }
}

/// Rounding is consistent: calling breakdown twice gives identical results.
#[test]
fn fee_rounding_is_deterministic() {
    let cases: &[(i128, u32, u32)] = &[
        (1_234_567, 1, 200),
        (9_876_543, 3, 500),
        (50_000_001, 4, 300),
    ];
    for &(wager, streak, fee_bps) in cases {
        let first  = calculate_payout_breakdown(wager, streak, fee_bps).unwrap();
        let second = calculate_payout_breakdown(wager, streak, fee_bps).unwrap();
        assert_eq!(first, second,
            "fee calculation must be deterministic for wager={wager} streak={streak} fee_bps={fee_bps}");
    }
}

/// Fee truncation: fee is always the floor of (gross × fee_bps / 10_000).
/// Verify by checking fee × 10_000 <= gross × fee_bps < (fee + 1) × 10_000.
#[test]
fn fee_is_floor_of_exact_fraction() {
    let cases: &[(i128, u32, u32)] = &[
        (1_000_001, 1, 300),
        (3_333_333, 2, 200),
        (7_777_777, 3, 500),
        (9_999_999, 4, 300),
    ];
    for &(wager, streak, fee_bps) in cases {
        let (gross, fee, _) = calculate_payout_breakdown(wager, streak, fee_bps).unwrap().unwrap();
        let numerator = gross * fee_bps as i128;
        // floor(numerator / 10_000) == fee
        assert!(fee * 10_000 <= numerator,
            "fee({fee}) × 10_000 must be <= gross({gross}) × fee_bps({fee_bps})");
        assert!(numerator < (fee + 1) * 10_000,
            "fee({fee}) must be the floor of gross({gross}) × fee_bps({fee_bps}) / 10_000");
    }
}

// ── Fee never exceeds configured percentage ───────────────────────────────────

/// Fee must never exceed gross × fee_bps / 10_000 (no over-charging).
#[test]
fn fee_never_exceeds_configured_percentage() {
    let wagers = [1i128, 100, 1_000, 10_000, 100_000, 1_000_000, 10_000_000, 100_000_000];
    for &wager in &wagers {
        for streak in [1, 2, 3, 4] {
            for fee_bps in [200u32, 300, 400, 500] {
                let (gross, fee, _) = calculate_payout_breakdown(wager, streak, fee_bps).unwrap().unwrap();
                let max_fee = gross * fee_bps as i128 / 10_000;
                assert!(fee <= max_fee,
                    "fee({fee}) must not exceed max_fee({max_fee}) for wager={wager} streak={streak} fee_bps={fee_bps}");
            }
        }
    }
}

/// Fee is always non-negative (never a negative deduction).
#[test]
fn fee_is_always_non_negative() {
    let wagers = [0i128, 1, 1_000_000, 100_000_000];
    for &wager in &wagers {
        for streak in [1, 2, 3, 4] {
            for fee_bps in [200u32, 300, 500] {
                let (_, fee, _) = calculate_payout_breakdown(wager, streak, fee_bps).unwrap().unwrap();
                assert!(fee >= 0,
                    "fee must be non-negative for wager={wager} streak={streak} fee_bps={fee_bps}");
            }
        }
    }
}

/// Net is always <= gross (fee deduction never increases the payout).
#[test]
fn fee_net_never_exceeds_gross() {
    let wagers = [1i128, 1_000_000, 100_000_000];
    for &wager in &wagers {
        for streak in [1, 2, 3, 4] {
            for fee_bps in [200u32, 300, 500] {
                let (gross, _, net) = calculate_payout_breakdown(wager, streak, fee_bps).unwrap().unwrap();
                assert!(net <= gross,
                    "net({net}) must not exceed gross({gross}) for wager={wager} streak={streak} fee_bps={fee_bps}");
            }
        }
    }
}

// ── Fee calculation across all multiplier tiers ───────────────────────────────

/// Streak 1 (1.9x) — full breakdown at both fee boundaries.
/// gross = 10_000_000 × 19_000 / 10_000 = 19_000_000
#[test]
fn fee_streak1_both_boundaries_full_breakdown() {
    let wager = 10_000_000i128;
    let (g200, f200, n200) = calculate_payout_breakdown(wager, 1, 200).unwrap().unwrap();
    assert_eq!(g200, 19_000_000);
    assert_eq!(f200, 380_000);   // 19_000_000 × 200 / 10_000
    assert_eq!(n200, 18_620_000);

    let (g500, f500, n500) = calculate_payout_breakdown(wager, 1, 500).unwrap().unwrap();
    assert_eq!(g500, 19_000_000);
    assert_eq!(f500, 950_000);   // 19_000_000 × 500 / 10_000
    assert_eq!(n500, 18_050_000);
}

/// Streak 2 (3.5x) — full breakdown at both fee boundaries.
/// gross = 10_000_000 × 35_000 / 10_000 = 35_000_000
#[test]
fn fee_streak2_both_boundaries_full_breakdown() {
    let wager = 10_000_000i128;
    let (g200, f200, n200) = calculate_payout_breakdown(wager, 2, 200).unwrap().unwrap();
    assert_eq!(g200, 35_000_000);
    assert_eq!(f200, 700_000);   // 35_000_000 × 200 / 10_000
    assert_eq!(n200, 34_300_000);

    let (g500, f500, n500) = calculate_payout_breakdown(wager, 2, 500).unwrap().unwrap();
    assert_eq!(g500, 35_000_000);
    assert_eq!(f500, 1_750_000); // 35_000_000 × 500 / 10_000
    assert_eq!(n500, 33_250_000);
}

/// Streak 3 (6.0x) — full breakdown at both fee boundaries.
/// gross = 10_000_000 × 60_000 / 10_000 = 60_000_000
#[test]
fn fee_streak3_both_boundaries_full_breakdown() {
    let wager = 10_000_000i128;
    let (g200, f200, n200) = calculate_payout_breakdown(wager, 3, 200).unwrap().unwrap();
    assert_eq!(g200, 60_000_000);
    assert_eq!(f200, 1_200_000); // 60_000_000 × 200 / 10_000
    assert_eq!(n200, 58_800_000);

    let (g500, f500, n500) = calculate_payout_breakdown(wager, 3, 500).unwrap().unwrap();
    assert_eq!(g500, 60_000_000);
    assert_eq!(f500, 3_000_000); // 60_000_000 × 500 / 10_000
    assert_eq!(n500, 57_000_000);
}

/// Streak 4+ (10.0x) — full breakdown at both fee boundaries.
/// gross = 10_000_000 × 100_000 / 10_000 = 100_000_000
#[test]
fn fee_streak4_both_boundaries_full_breakdown() {
    let wager = 10_000_000i128;
    let (g200, f200, n200) = calculate_payout_breakdown(wager, 4, 200).unwrap().unwrap();
    assert_eq!(g200, 100_000_000);
    assert_eq!(f200, 2_000_000); // 100_000_000 × 200 / 10_000
    assert_eq!(n200, 98_000_000);

    let (g500, f500, n500) = calculate_payout_breakdown(wager, 4, 500).unwrap().unwrap();
    assert_eq!(g500, 100_000_000);
    assert_eq!(f500, 5_000_000); // 100_000_000 × 500 / 10_000
    assert_eq!(n500, 95_000_000);
}

/// Streak 5 produces the same fee as streak 4 (multiplier cap applies).
#[test]
fn fee_streak5_equals_streak4_fee() {
    let wager = 10_000_000i128;
    for fee_bps in [200u32, 300, 500] {
        let (_, fee4, _) = calculate_payout_breakdown(wager, 4, fee_bps).unwrap().unwrap();
        let (_, fee5, _) = calculate_payout_breakdown(wager, 5, fee_bps).unwrap().unwrap();
        assert_eq!(fee4, fee5,
            "fee at streak 5 must equal fee at streak 4 (cap) for fee_bps={fee_bps}");
    }
}

/// Fee grows with streak (higher multiplier → higher gross → higher fee).
#[test]
fn fee_grows_with_streak_for_same_wager_and_fee_bps() {
    let wager = 10_000_000i128;
    for fee_bps in [200u32, 300, 500] {
        let (_, f1, _) = calculate_payout_breakdown(wager, 1, fee_bps).unwrap().unwrap();
        let (_, f2, _) = calculate_payout_breakdown(wager, 2, fee_bps).unwrap().unwrap();
        let (_, f3, _) = calculate_payout_breakdown(wager, 3, fee_bps).unwrap().unwrap();
        let (_, f4, _) = calculate_payout_breakdown(wager, 4, fee_bps).unwrap().unwrap();
        assert!(f1 < f2, "fee at streak 2 must exceed streak 1 for fee_bps={fee_bps}");
        assert!(f2 < f3, "fee at streak 3 must exceed streak 2 for fee_bps={fee_bps}");
        assert!(f3 < f4, "fee at streak 4 must exceed streak 3 for fee_bps={fee_bps}");
    }
}

// ── calculate_payout wrapper consistency ─────────────────────────────────────

/// calculate_payout net must match calculate_payout_breakdown net at both boundaries.
#[test]
fn fee_payout_wrapper_matches_breakdown_at_boundaries() {
    let cases: &[(i128, u32)] = &[
        (10_000_000, 1),
        (10_000_000, 2),
        (10_000_000, 3),
        (10_000_000, 4),
        (1_000_000, 1),
        (100_000_000, 4),
    ];
    for &(wager, streak) in cases {
        for fee_bps in [200u32, 500] {
            let (_, _, breakdown_net) = calculate_payout_breakdown(wager, streak, fee_bps).unwrap().unwrap();
            let payout_net = calculate_payout(wager, streak, fee_bps).unwrap().unwrap();
            assert_eq!(payout_net, breakdown_net,
                "payout wrapper must match breakdown net for wager={wager} streak={streak} fee_bps={fee_bps}");
        }
    }
}
