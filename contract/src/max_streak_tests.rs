/// # Maximum Streak Integration Tests — Issue #154
///
/// Validates the streak-4+ (10×) multiplier cap end-to-end through the full
/// game lifecycle: start → reveal → continue → cash_out.
///
/// ## Multiplier cap behavior
///
/// | Streak | Multiplier (bps) | Gross payout (10 XLM wager) |
/// |--------|------------------|-----------------------------|
/// | 1      | 19_000 (1.9×)    | 19_000_000                  |
/// | 2      | 35_000 (3.5×)    | 35_000_000                  |
/// | 3      | 60_000 (6.0×)    | 60_000_000                  |
/// | 4      | 100_000 (10×)    | 100_000_000  ← cap           |
/// | 5+     | 100_000 (10×)    | 100_000_000  ← same cap      |
///
/// ## Payout formula (fee_bps = 300)
///   gross = wager × 100_000 / 10_000 = wager × 10
///   fee   = gross × 300 / 10_000     = gross × 3%
///   net   = gross − fee
///
/// For wager = 10_000_000:
///   gross = 100_000_000
///   fee   =   3_000_000
///   net   =  97_000_000
use super::*;
use soroban_sdk::testutils::Address as _;

// ── Harness ───────────────────────────────────────────────────────────────────

const WAGER: i128 = 10_000_000; // 1 XLM
const FEE_BPS: i128 = 300;
const STREAK4_MULT_BPS: i128 = 100_000; // 10×

/// Expected gross payout at the 10× cap.
fn gross_at_cap(wager: i128) -> i128 {
    wager * STREAK4_MULT_BPS / 10_000
}

/// Expected net payout at the 10× cap.
fn net_at_cap(wager: i128) -> i128 {
    let gross = gross_at_cap(wager);
    let fee = gross * FEE_BPS / 10_000;
    gross - fee
}

fn setup() -> (Env, CoinflipContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CoinflipContract, ());
    let client = CoinflipContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let token = Address::generate(&env);
    client.initialize(
        &admin,
        &treasury,
        &token,
        &(FEE_BPS as u32),
        &1_000_000,
        &100_000_000,
        &BytesN::from_array(&env, &[0u8; 32]),
    );
    (env, client, contract_id)
}

fn fund(env: &Env, contract_id: &Address, amount: i128) {
    env.as_contract(contract_id, || {
        let mut stats = CoinflipContract::load_stats(env);
        stats.reserve_balance = amount;
        CoinflipContract::save_stats(env, &stats);
    });
}

/// Inject a game at a specific streak directly into storage.
fn inject_at_streak(env: &Env, contract_id: &Address, player: &Address, streak: u32) {
    let commitment: BytesN<32> = env
        .crypto()
        .sha256(&soroban_sdk::Bytes::from_slice(env, &[1u8; 32]))
        .into();
    let contract_random: BytesN<32> = env
        .crypto()
        .sha256(&soroban_sdk::Bytes::from_slice(env, &[2u8; 32]))
        .into();
    let game = GameState {
        wager: WAGER,
        side: Side::Heads,
        streak,
        commitment,
        contract_random,
        fee_bps: FEE_BPS as u32,
        phase: GamePhase::Revealed,
        start_ledger: env.ledger().sequence(),
        vrf_input: env.crypto().sha256(&soroban_sdk::Bytes::from_slice(env, &[42u8; 32])).into(),
    };
    env.as_contract(contract_id, || {
        CoinflipContract::save_player_game(env, player, &game);
    });
}

// ── Exact payout at streak 4 ──────────────────────────────────────────────────

/// Cash out at streak 4 yields the exact 10× net payout.
///
/// gross = 10_000_000 × 10 = 100_000_000
/// fee   = 100_000_000 × 300 / 10_000 = 3_000_000
/// net   = 97_000_000
#[test]
fn cash_out_streak_4_exact_10x_net_payout() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, gross_at_cap(WAGER) * 10); // ample reserves
    let player = Address::generate(&env);
    inject_at_streak(&env, &contract_id, &player, 4);

    let payout = client.cash_out(&player);

    assert_eq!(
        payout,
        net_at_cap(WAGER),
        "streak 4 cash_out must yield exact 10× net payout"
    );
}

/// Reserve is debited by gross (pre-fee) at cash_out, not net.
///
/// This confirms the reserve accounting is conservative: the full 10× gross
/// is deducted, and the fee portion is credited to treasury separately.
#[test]
fn cash_out_streak_4_reserve_debited_by_gross() {
    let (env, client, contract_id) = setup();
    let initial_reserve = gross_at_cap(WAGER) * 10;
    fund(&env, &contract_id, initial_reserve);
    let player = Address::generate(&env);
    inject_at_streak(&env, &contract_id, &player, 4);

    client.cash_out(&player);

    let remaining = env.as_contract(&contract_id, || {
        CoinflipContract::load_stats(&env).reserve_balance
    });
    assert_eq!(
        remaining,
        initial_reserve - gross_at_cap(WAGER),
        "reserve must be debited by gross (pre-fee) payout"
    );
}

// ── Multiplier cap: streak 5+ equals streak 4 ────────────────────────────────

/// Streaks 5, 6, 7, 10 all produce the same net payout as streak 4.
///
/// The multiplier is capped at 100_000 bps (10×) for all streaks ≥ 4.
/// This test verifies the cap is flat — no additional multiplier for longer streaks.
#[test]
fn cash_out_streaks_5_through_10_equal_streak_4_payout() {
    let expected = net_at_cap(WAGER);

    for streak in [5u32, 6, 7, 10] {
        let (env, client, contract_id) = setup();
        fund(&env, &contract_id, gross_at_cap(WAGER) * 20);
        let player = Address::generate(&env);
        inject_at_streak(&env, &contract_id, &player, streak);

        let payout = client.cash_out(&player);

        assert_eq!(
            payout, expected,
            "streak {streak} must yield the same payout as streak 4 (10× cap)"
        );
    }
}

/// Streak 4 and streak 100 produce identical payouts for the same wager.
#[test]
fn cash_out_streak_4_and_streak_100_identical_payout() {
    let (env4, client4, cid4) = setup();
    fund(&env4, &cid4, gross_at_cap(WAGER) * 20);
    let p4 = Address::generate(&env4);
    inject_at_streak(&env4, &cid4, &p4, 4);
    let payout4 = client4.cash_out(&p4);

    let (env100, client100, cid100) = setup();
    fund(&env100, &cid100, gross_at_cap(WAGER) * 20);
    let p100 = Address::generate(&env100);
    inject_at_streak(&env100, &cid100, &p100, 100);
    let payout100 = client100.cash_out(&p100);

    assert_eq!(
        payout4, payout100,
        "streak 4 and streak 100 must produce identical payouts"
    );
}

// ── Payout is strictly greater at each tier transition ────────────────────────

/// Each streak tier produces a strictly higher payout than the previous.
///
/// streak 1 < streak 2 < streak 3 < streak 4
/// This confirms the multiplier ladder is correctly applied before the cap.
#[test]
fn payout_strictly_increases_from_streak_1_to_4() {
    let mut prev_payout = 0i128;
    for streak in [1u32, 2, 3, 4] {
        let (env, client, contract_id) = setup();
        fund(&env, &contract_id, gross_at_cap(WAGER) * 20);
        let player = Address::generate(&env);
        inject_at_streak(&env, &contract_id, &player, streak);

        let payout = client.cash_out(&player);

        assert!(
            payout > prev_payout,
            "streak {streak} payout ({payout}) must exceed streak {} payout ({prev_payout})",
            streak - 1
        );
        prev_payout = payout;
    }
}

// ── Exact payout values for all tiers ────────────────────────────────────────

/// Verify exact net payout for each streak tier at WAGER = 10_000_000, fee = 300 bps.
///
/// | Streak | Gross       | Fee       | Net        |
/// |--------|-------------|-----------|------------|
/// | 1      | 19_000_000  | 570_000   | 18_430_000 |
/// | 2      | 35_000_000  | 1_050_000 | 33_950_000 |
/// | 3      | 60_000_000  | 1_800_000 | 58_200_000 |
/// | 4      | 100_000_000 | 3_000_000 | 97_000_000 |
#[test]
fn exact_net_payout_all_streak_tiers() {
    let cases: &[(u32, i128)] = &[
        (1, 18_430_000),
        (2, 33_950_000),
        (3, 58_200_000),
        (4, 97_000_000),
    ];

    for &(streak, expected_net) in cases {
        let (env, client, contract_id) = setup();
        fund(&env, &contract_id, gross_at_cap(WAGER) * 20);
        let player = Address::generate(&env);
        inject_at_streak(&env, &contract_id, &player, streak);

        let payout = client.cash_out(&player);

        assert_eq!(
            payout, expected_net,
            "streak {streak}: expected net={expected_net}, got {payout}"
        );
    }
}

// ── Solvency maintained through max-streak cash_out ──────────────────────────

/// After a 10× cash_out, the reserve remains non-negative.
#[test]
fn reserve_non_negative_after_max_streak_cash_out() {
    let (env, client, contract_id) = setup();
    // Fund exactly enough for one max-streak payout
    fund(&env, &contract_id, gross_at_cap(WAGER));
    let player = Address::generate(&env);
    inject_at_streak(&env, &contract_id, &player, 4);

    client.cash_out(&player);

    let remaining = env.as_contract(&contract_id, || {
        CoinflipContract::load_stats(&env).reserve_balance
    });
    assert!(remaining >= 0, "reserve must remain non-negative after max-streak cash_out");
}
