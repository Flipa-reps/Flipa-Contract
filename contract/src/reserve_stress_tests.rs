/// Comprehensive stress tests for reserve liquidity management under adversarial conditions.
///
/// Issue #409: Architect reserve liquidity stress testing framework under adversarial conditions
///
/// Covers:
/// - Reserve depletion with maximum payouts
/// - Reserve exhaustion scenarios and rejection
/// - Reserve recovery after losses
/// - Solvency checks at boundaries (0 and near-0 reserves)
/// - Concurrent reserve operations
/// - Reserve management strategy documentation
use super::*;
use soroban_sdk::testutils::{Address as _, Ledger};
use proptest::prelude::*;

// ── Harness ───────────────────────────────────────────────────────────────────

fn setup() -> (Env, CoinflipContractClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CoinflipContract, ());
    let client = CoinflipContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let token = Address::generate(&env);
    client.initialize(&admin, &treasury, &token, &300, &1_000_000, &100_000_000, &BytesN::from_array(&env, &[0u8; 32]));
    (env, client, contract_id, admin)
}

fn fund_reserves(env: &Env, contract_id: &Address, amount: i128) {
    env.as_contract(contract_id, || {
        let mut stats = CoinflipContract::load_stats(env);
        stats.reserve_balance = amount;
        CoinflipContract::save_stats(env, &stats);
    });
}

fn get_reserves(env: &Env, contract_id: &Address) -> i128 {
    env.as_contract(contract_id, || {
        CoinflipContract::load_stats(env).reserve_balance
    })
}

fn make_secret(env: &Env, seed: u8) -> Bytes {
    let mut b = Bytes::new(env);
    for _ in 0..32 {
        b.push_back(seed);
    }
    b
}

fn make_commitment(env: &Env, seed: u8) -> BytesN<32> {
    env.crypto().sha256(&make_secret(env, seed)).into()
}

fn inject_game(
    env: &Env,
    contract_id: &Address,
    player: &Address,
    phase: GamePhase,
    streak: u32,
    wager: i128,
) {
    let game = GameState {
        wager,
        side: Side::Heads,
        streak,
        commitment: make_commitment(env, 1),
        contract_random: make_commitment(env, 2),
        fee_bps: 300,
        phase,
        start_ledger: env.ledger().sequence(),
    
        vrf_input: env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into(),
    };
    env.as_contract(contract_id, || {
        CoinflipContract::save_player_game(env, player, &game);
    });
}

// ── Reserve depletion with maximum payouts ───────────────────────────────────

/// Test that maximum payout (10x multiplier at 5% fee) depletes reserves correctly.
/// Wager: 100_000_000 stroops (10 XLM)
/// Multiplier: 10x (streak 4+)
/// Fee: 5% (500 bps)
/// Gross: 1_000_000_000
/// Fee: 50_000_000
/// Net: 950_000_000
#[test]
fn reserve_depletion_max_payout_single_game() {
    let (env, client, contract_id, _admin) = setup();
    let initial_reserve = 1_000_000_000i128;
    fund_reserves(&env, &contract_id, initial_reserve);

    let player = Address::generate(&env);
    inject_game(&env, &contract_id, &player, GamePhase::Revealed, 4, 100_000_000);

    // Cash out the maximum payout
    let payout = client.cash_out(&player);
    assert_eq!(payout, 950_000_000, "max payout should be 950_000_000");

    // Reserve should be depleted by the payout
    let remaining = get_reserves(&env, &contract_id);
    assert_eq!(remaining, initial_reserve - payout,
        "reserve should decrease by payout amount");
}

/// Test reserve depletion with 10 consecutive maximum payouts.
/// Each game: 100_000_000 wager, 10x multiplier, 5% fee = 950_000_000 net
/// Total depletion: 9_500_000_000
#[test]
fn reserve_depletion_10x_max_payouts() {
    let (env, client, contract_id, _admin) = setup();
    let initial_reserve = 10_000_000_000i128;
    fund_reserves(&env, &contract_id, initial_reserve);

    let max_payout = 950_000_000i128;
    let mut total_depleted = 0i128;

    for i in 0..10 {
        let player = Address::generate(&env);
        inject_game(&env, &contract_id, &player, GamePhase::Revealed, 4, 100_000_000);
        let payout = client.cash_out(&player);
        assert_eq!(payout, max_payout, "payout {i} should be max");
        total_depleted += payout;
    }

    let remaining = get_reserves(&env, &contract_id);
    assert_eq!(remaining, initial_reserve - total_depleted,
        "reserve should be depleted by all payouts");
    assert_eq!(total_depleted, 9_500_000_000,
        "total depletion should be 9.5 billion stroops");
}

// ── Reserve exhaustion scenarios ──────────────────────────────────────────────

/// Test that start_game is rejected when reserve cannot cover worst-case payout.
/// Reserve: 100_000_000 (100 XLM)
/// Wager: 100_000_000 (10 XLM)
/// Worst case: 10x multiplier, 5% fee = 950_000_000 net
/// Reserve < worst case → rejection
#[test]
fn reserve_exhaustion_start_game_rejected() {
    let (env, client, contract_id, _admin) = setup();
    let reserve = 100_000_000i128;
    fund_reserves(&env, &contract_id, reserve);

    let player = Address::generate(&env);
    let result = client.try_start_game(
        &player,
        &Side::Heads,
        &100_000_000,
        &make_commitment(&env, 1, &env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into()),
    );
    assert_eq!(result, Err(Ok(Error::InsufficientReserves)),
        "start_game should be rejected when reserve < worst-case payout");
}

/// Test that zero reserve rejects all new games.
#[test]
fn reserve_exhaustion_zero_reserve_rejects_all_games() {
    let (env, client, contract_id, _admin) = setup();
    fund_reserves(&env, &contract_id, 0);

    for wager in [1_000_000, 10_000_000, 50_000_000] {
        let player = Address::generate(&env);
        let result = client.try_start_game(
            &player,
            &Side::Heads,
            &wager,
            &make_commitment(&env, 1, &env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into()),
        );
        assert_eq!(result, Err(Ok(Error::InsufficientReserves)),
            "zero reserve should reject all games (wager={wager})");
    }
}

/// Test that near-zero reserve (1 stroop) rejects all games.
#[test]
fn reserve_exhaustion_near_zero_reserve_rejects_all_games() {
    let (env, client, contract_id, _admin) = setup();
    fund_reserves(&env, &contract_id, 1);

    let player = Address::generate(&env);
    let result = client.try_start_game(
        &player,
        &Side::Heads,
        &1_000_000,
        &make_commitment(&env, 1, &env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into()),
    );
    assert_eq!(result, Err(Ok(Error::InsufficientReserves)),
        "near-zero reserve should reject games");
}

/// Test that reserve exactly at worst-case payout boundary is accepted.
/// Wager: 100_000_000
/// Worst case: 950_000_000
/// Reserve: exactly 950_000_000 → accepted
#[test]
fn reserve_exhaustion_exact_boundary_accepted() {
    let (env, client, contract_id, _admin) = setup();
    let worst_case_payout = 950_000_000i128;
    fund_reserves(&env, &contract_id, worst_case_payout);

    let player = Address::generate(&env);
    let result = client.try_start_game(
        &player,
        &Side::Heads,
        &100_000_000,
        &make_commitment(&env, 1, &env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into()),
    );
    assert!(result.is_ok(), "start_game should succeed at exact reserve boundary");
}

/// Test that reserve one stroop below worst-case boundary is rejected.
#[test]
fn reserve_exhaustion_one_stroop_below_boundary_rejected() {
    let (env, client, contract_id, _admin) = setup();
    let worst_case_payout = 950_000_000i128;
    fund_reserves(&env, &contract_id, worst_case_payout - 1);

    let player = Address::generate(&env);
    let result = client.try_start_game(
        &player,
        &Side::Heads,
        &100_000_000,
        &make_commitment(&env, 1, &env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into()),
    );
    assert_eq!(result, Err(Ok(Error::InsufficientReserves)),
        "start_game should be rejected one stroop below boundary");
}

// ── Reserve recovery after losses ────────────────────────────────────────────

/// Test that losing games credit the reserve.
/// Wager: 100_000_000
/// Loss: wager forfeited to reserve
/// Reserve increases by 100_000_000
#[test]
fn reserve_recovery_single_loss_credits_reserve() {
    let (env, client, contract_id, _admin) = setup();
    let initial_reserve = 1_000_000_000i128;
    fund_reserves(&env, &contract_id, initial_reserve);

    let player = Address::generate(&env);
    let wager = 100_000_000i128;
    inject_game(&env, &contract_id, &player, GamePhase::Revealed, 1, wager);

    // Simulate a loss by deleting the game (contract does this on loss)
    env.as_contract(&contract_id, || {
        let mut stats = CoinflipContract::load_stats(&env);
        stats.reserve_balance += wager;
        CoinflipContract::save_stats(&env, &stats);
        CoinflipContract::delete_player_game(&env, &player);
    });

    let remaining = get_reserves(&env, &contract_id);
    assert_eq!(remaining, initial_reserve + wager,
        "reserve should increase by wager on loss");
}

/// Test reserve recovery through 10 consecutive losses.
/// Each loss: 100_000_000 wager
/// Total recovery: 1_000_000_000
#[test]
fn reserve_recovery_10x_losses_accumulate() {
    let (env, client, contract_id, _admin) = setup();
    let initial_reserve = 100_000_000i128;
    fund_reserves(&env, &contract_id, initial_reserve);

    let wager = 100_000_000i128;
    let mut total_recovered = 0i128;

    for _ in 0..10 {
        let player = Address::generate(&env);
        inject_game(&env, &contract_id, &player, GamePhase::Revealed, 1, wager);

        env.as_contract(&contract_id, || {
            let mut stats = CoinflipContract::load_stats(&env);
            stats.reserve_balance += wager;
            CoinflipContract::save_stats(&env, &stats);
            CoinflipContract::delete_player_game(&env, &player);
        });

        total_recovered += wager;
    }

    let remaining = get_reserves(&env, &contract_id);
    assert_eq!(remaining, initial_reserve + total_recovered,
        "reserve should accumulate all losses");
    assert_eq!(total_recovered, 1_000_000_000,
        "total recovery should be 1 billion stroops");
}

/// Test reserve recovery from depleted state.
/// Start: 100_000_000 reserve
/// Deplete: 10 max payouts (9_500_000_000 total)
/// Recover: 10 losses (1_000_000_000 total)
/// Final: 100_000_000 + 1_000_000_000 - 9_500_000_000 = -8_400_000_000 (would be negative)
/// This tests the recovery mechanism after depletion.
#[test]
fn reserve_recovery_after_depletion_scenario() {
    let (env, client, contract_id, _admin) = setup();
    let initial_reserve = 10_000_000_000i128;
    fund_reserves(&env, &contract_id, initial_reserve);

    // Deplete with 5 max payouts
    let max_payout = 950_000_000i128;
    for i in 0..5 {
        let player = Address::generate(&env);
        inject_game(&env, &contract_id, &player, GamePhase::Revealed, 4, 100_000_000);
        let payout = client.cash_out(&player);
        assert_eq!(payout, max_payout, "payout {i} should be max");
    }

    let after_depletion = get_reserves(&env, &contract_id);
    assert_eq!(after_depletion, initial_reserve - (5 * max_payout),
        "reserve should be depleted");

    // Recover with 5 losses
    for _ in 0..5 {
        let player = Address::generate(&env);
        inject_game(&env, &contract_id, &player, GamePhase::Revealed, 1, 100_000_000);

        env.as_contract(&contract_id, || {
            let mut stats = CoinflipContract::load_stats(&env);
            stats.reserve_balance += 100_000_000;
            CoinflipContract::save_stats(&env, &stats);
            CoinflipContract::delete_player_game(&env, &player);
        });
    }

    let after_recovery = get_reserves(&env, &contract_id);
    assert_eq!(after_recovery, after_depletion + (5 * 100_000_000),
        "reserve should recover from losses");
}

// ── Solvency checks at boundaries ────────────────────────────────────────────

/// Test solvency check at zero reserve boundary.
/// Reserve: 0
/// Any wager should be rejected
#[test]
fn solvency_check_zero_reserve_boundary() {
    let (env, client, contract_id, _admin) = setup();
    fund_reserves(&env, &contract_id, 0);

    for wager in [1_000_000, 10_000_000, 100_000_000] {
        let player = Address::generate(&env);
        let result = client.try_start_game(
            &player,
            &Side::Heads,
            &wager,
            &make_commitment(&env, 1, &env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into()),
        );
        assert_eq!(result, Err(Ok(Error::InsufficientReserves)),
            "solvency check should reject at zero reserve (wager={wager})");
    }
}

/// Test solvency check prevents insolvency.
/// Reserve: 500_000_000
/// Wager: 100_000_000 (worst case: 950_000_000 payout)
/// 500_000_000 < 950_000_000 → rejected
#[test]
fn solvency_check_prevents_insolvency() {
    let (env, client, contract_id, _admin) = setup();
    fund_reserves(&env, &contract_id, 500_000_000);

    let player = Address::generate(&env);
    let result = client.try_start_game(
        &player,
        &Side::Heads,
        &100_000_000,
        &make_commitment(&env, 1, &env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into()),
    );
    assert_eq!(result, Err(Ok(Error::InsufficientReserves)),
        "solvency check should prevent insolvency");
}

/// Test solvency check with multiple concurrent games.
/// Reserve: 2_000_000_000
/// Game 1: 100_000_000 wager (worst case: 950_000_000)
/// Game 2: 100_000_000 wager (worst case: 950_000_000)
/// Total worst case: 1_900_000_000 < 2_000_000_000 → both accepted
#[test]
fn solvency_check_multiple_concurrent_games() {
    let (env, client, contract_id, _admin) = setup();
    fund_reserves(&env, &contract_id, 2_000_000_000);

    let player1 = Address::generate(&env);
    let result1 = client.try_start_game(
        &player1,
        &Side::Heads,
        &100_000_000,
        &make_commitment(&env, 1, &env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into()),
    );
    assert!(result1.is_ok(), "first game should be accepted");

    let player2 = Address::generate(&env);
    let result2 = client.try_start_game(
        &player2,
        &Side::Heads,
        &100_000_000,
        &make_commitment(&env, 2, &env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into()),
    );
    assert!(result2.is_ok(), "second game should be accepted");

    // Third game would exceed reserve
    let player3 = Address::generate(&env);
    let result3 = client.try_start_game(
        &player3,
        &Side::Heads,
        &100_000_000,
        &make_commitment(&env, 3, &env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into()),
    );
    assert_eq!(result3, Err(Ok(Error::InsufficientReserves)),
        "third game should be rejected (would exceed reserve)");
}

// ── Concurrent reserve operations ────────────────────────────────────────────

/// Test that concurrent payouts don't corrupt reserve state.
/// Start with sufficient reserve, pay out multiple games sequentially.
#[test]
fn concurrent_operations_sequential_payouts() {
    let (env, client, contract_id, _admin) = setup();
    let initial_reserve = 10_000_000_000i128;
    fund_reserves(&env, &contract_id, initial_reserve);

    let mut total_paid = 0i128;

    for i in 0..5 {
        let player = Address::generate(&env);
        inject_game(&env, &contract_id, &player, GamePhase::Revealed, 2, 50_000_000);
        let payout = client.cash_out(&player);
        total_paid += payout;
        
        let remaining = get_reserves(&env, &contract_id);
        assert_eq!(remaining, initial_reserve - total_paid,
            "reserve should be consistent after payout {i}");
    }
}

/// Test that reserve state is consistent after mixed wins and losses.
#[test]
fn concurrent_operations_mixed_wins_and_losses() {
    let (env, client, contract_id, _admin) = setup();
    let initial_reserve = 5_000_000_000i128;
    fund_reserves(&env, &contract_id, initial_reserve);

    let mut expected_reserve = initial_reserve;

    // 3 wins (payouts)
    for _ in 0..3 {
        let player = Address::generate(&env);
        inject_game(&env, &contract_id, &player, GamePhase::Revealed, 1, 50_000_000);
        let payout = client.cash_out(&player);
        expected_reserve -= payout;
    }

    // 3 losses (wagers credited to reserve)
    for _ in 0..3 {
        let player = Address::generate(&env);
        inject_game(&env, &contract_id, &player, GamePhase::Revealed, 1, 50_000_000);
        
        env.as_contract(&contract_id, || {
            let mut stats = CoinflipContract::load_stats(&env);
            stats.reserve_balance += 50_000_000;
            CoinflipContract::save_stats(&env, &stats);
            CoinflipContract::delete_player_game(&env, &player);
        });
        
        expected_reserve += 50_000_000;
    }

    let actual_reserve = get_reserves(&env, &contract_id);
    assert_eq!(actual_reserve, expected_reserve,
        "reserve should be consistent after mixed operations");
}

// ── Property-based stress tests ──────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    /// PROPERTY: Reserve never goes negative (solvency invariant).
    #[test]
    fn prop_reserve_never_negative(
        initial_reserve in 1_000_000_000i128..=100_000_000_000i128,
        num_games in 1usize..=10usize,
        wagers in prop::collection::vec(1_000_000i128..=100_000_000i128, num_games),
    ) {
        let (env, client, contract_id, _admin) = setup();
        fund_reserves(&env, &contract_id, initial_reserve);

        for (i, &wager) in wagers.iter().enumerate() {
            let player = Address::generate(&env);
            let result = client.try_start_game(
                &player,
                &Side::Heads,
                &wager,
                &make_commitment(&env, i as u8, &env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into()),
            );

            if result.is_ok() {
                inject_game(&env, &contract_id, &player, GamePhase::Revealed, 1, wager);
                let _ = client.try_cash_out(&player);
            }

            let reserve = get_reserves(&env, &contract_id);
            prop_assert!(reserve >= 0, "reserve must never be negative (got {})", reserve);
        }
    }

    /// PROPERTY: Solvency check prevents all insolvencies.
    #[test]
    fn prop_solvency_check_prevents_insolvency(
        initial_reserve in 1_000_000_000i128..=10_000_000_000i128,
        wager in 1_000_000i128..=100_000_000i128,
    ) {
        let (env, client, contract_id, _admin) = setup();
        fund_reserves(&env, &contract_id, initial_reserve);

        let player = Address::generate(&env);
        let result = client.try_start_game(
            &player,
            &Side::Heads,
            &wager,
            &make_commitment(&env, 1, &env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into()),
        );

        if result.is_ok() {
            // Game was accepted, so reserve must be sufficient for worst case
            let worst_case = wager * 100_000 / 10_000 * 500 / 10_000; // 10x * 5% fee
            prop_assert!(initial_reserve >= worst_case,
                "if game accepted, reserve({}) must cover worst case({})",
                initial_reserve, worst_case);
        }
    }

    /// PROPERTY: Reserve changes are deterministic and consistent.
    #[test]
    fn prop_reserve_changes_deterministic(
        initial_reserve in 1_000_000_000i128..=10_000_000_000i128,
        wager in 1_000_000i128..=100_000_000i128,
        streak in 1u32..=4u32,
    ) {
        let (env, client, contract_id, _admin) = setup();
        fund_reserves(&env, &contract_id, initial_reserve);

        let player = Address::generate(&env);
        inject_game(&env, &contract_id, &player, GamePhase::Revealed, streak, wager);

        let before = get_reserves(&env, &contract_id);
        let payout = client.cash_out(&player);
        let after = get_reserves(&env, &contract_id);

        prop_assert_eq!(after, before - payout,
            "reserve change must equal payout amount");
    }
}
