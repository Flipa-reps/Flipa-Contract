/// Property-based tests for contract statistics accuracy.
///
/// Issue: add-property-tests-statistics-accuracy
///
/// Covers:
///   - Property 29: Statistics accuracy test
///   - total_games increments correctly
///   - total_volume accumulates all wagers
///   - total_fees accumulates all collected fees
///   - reserve_balance updates correctly
///   - 100+ iterations with various game sequences
///   - Stats never decrease incorrectly
use super::*;
use soroban_sdk::testutils::Address as _;
use proptest::prelude::*;

// ── Harness ───────────────────────────────────────────────────────────────────

fn setup() -> (Env, CoinflipContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CoinflipContract, ());
    let client = CoinflipContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let token = Address::generate(&env);
    client.initialize(&admin, &treasury, &token, &300, &1_000_000, &100_000_000);
    (env, client, contract_id)
}

fn fund(env: &Env, contract_id: &Address, amount: i128) {
    env.as_contract(contract_id, || {
        let mut stats = CoinflipContract::load_stats(env);
        stats.reserve_balance = amount;
        CoinflipContract::save_stats(env, &stats);
    });
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

fn load_stats(env: &Env, contract_id: &Address) -> ContractStats {
    env.as_contract(contract_id, || CoinflipContract::load_stats(env))
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
        start_ledger: 0,
    };
    env.as_contract(contract_id, || {
        CoinflipContract::save_player_game(env, player, &game);
    });
}

// ── total_games increments correctly ─────────────────────────────────────────

#[test]
fn test_total_games_starts_at_zero() {
    let (env, _client, contract_id) = setup();
    assert_eq!(load_stats(&env, &contract_id).total_games, 0);
}

#[test]
fn test_total_games_increments_on_start_game() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    let player = Address::generate(&env);
    client.start_game(&player, &Side::Heads, &5_000_000, &make_commitment(&env, 1));
    assert_eq!(load_stats(&env, &contract_id).total_games, 1);
}

#[test]
fn test_total_games_increments_for_each_new_game() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000_000);
    for i in 0u8..10 {
        let player = Address::generate(&env);
        client.start_game(&player, &Side::Heads, &1_000_000, &make_commitment(&env, i + 1));
    }
    assert_eq!(load_stats(&env, &contract_id).total_games, 10);
}

#[test]
fn test_total_games_does_not_increment_on_failed_start() {
    let (env, client, contract_id) = setup();
    // No reserves — start_game will fail
    let player = Address::generate(&env);
    let _ = client.try_start_game(&player, &Side::Heads, &5_000_000, &make_commitment(&env, 1));
    assert_eq!(load_stats(&env, &contract_id).total_games, 0);
}

#[test]
fn test_total_games_does_not_increment_on_reveal_or_cash_out() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    let player = Address::generate(&env);
    client.start_game(&player, &Side::Heads, &5_000_000, &make_commitment(&env, 1));
    let before = load_stats(&env, &contract_id).total_games;
    client.reveal(&player, &make_secret(&env, 1)).unwrap();
    client.cash_out(&player).unwrap();
    assert_eq!(load_stats(&env, &contract_id).total_games, before);
}

// ── total_volume accumulates all wagers ──────────────────────────────────────

#[test]
fn test_total_volume_starts_at_zero() {
    let (env, _client, contract_id) = setup();
    assert_eq!(load_stats(&env, &contract_id).total_volume, 0);
}

#[test]
fn test_total_volume_accumulates_wager_on_start_game() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    let wager = 7_000_000i128;
    let player = Address::generate(&env);
    client.start_game(&player, &Side::Heads, &wager, &make_commitment(&env, 1));
    assert_eq!(load_stats(&env, &contract_id).total_volume, wager);
}

#[test]
fn test_total_volume_accumulates_across_multiple_games() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000_000);
    let wagers = [1_000_000i128, 5_000_000, 10_000_000, 3_000_000, 7_500_000];
    let expected: i128 = wagers.iter().sum();
    for (i, &wager) in wagers.iter().enumerate() {
        let player = Address::generate(&env);
        client.start_game(&player, &Side::Heads, &wager, &make_commitment(&env, i as u8 + 1));
    }
    assert_eq!(load_stats(&env, &contract_id).total_volume, expected);
}

#[test]
fn test_total_volume_does_not_change_on_cash_out() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    let player = Address::generate(&env);
    client.start_game(&player, &Side::Heads, &5_000_000, &make_commitment(&env, 1));
    let before = load_stats(&env, &contract_id).total_volume;
    client.reveal(&player, &make_secret(&env, 1)).unwrap();
    client.cash_out(&player).unwrap();
    assert_eq!(load_stats(&env, &contract_id).total_volume, before);
}

// ── total_fees accumulates correctly ─────────────────────────────────────────

#[test]
fn test_total_fees_starts_at_zero() {
    let (env, _client, contract_id) = setup();
    assert_eq!(load_stats(&env, &contract_id).total_fees, 0);
}

#[test]
fn test_total_fees_accumulates_on_cash_out() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    let wager = 10_000_000i128;
    let player = Address::generate(&env);
    inject_game(&env, &contract_id, &player, GamePhase::Revealed, 1, wager);
    client.cash_out(&player).unwrap();
    // gross=19_000_000, fee=570_000 (300bps)
    let expected_fee = 570_000i128;
    assert_eq!(load_stats(&env, &contract_id).total_fees, expected_fee);
}

#[test]
fn test_total_fees_accumulates_across_multiple_settlements() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000_000);
    let wager = 10_000_000i128;
    let mut expected_fees = 0i128;
    for streak in 1u32..=4 {
        let player = Address::generate(&env);
        inject_game(&env, &contract_id, &player, GamePhase::Revealed, streak, wager);
        let (_gross, fee, _net) = calculate_payout_breakdown(wager, streak, 300).unwrap();
        expected_fees += fee;
        client.cash_out(&player).unwrap();
    }
    assert_eq!(load_stats(&env, &contract_id).total_fees, expected_fees);
}

#[test]
fn test_total_fees_does_not_accumulate_on_loss() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    let player = Address::generate(&env);
    // Seed 3 → loss for Heads player
    let secret = make_secret(&env, 3);
    let commitment = make_commitment(&env, 3);
    client.start_game(&player, &Side::Heads, &5_000_000, &commitment);
    client.reveal(&player, &secret).unwrap();
    assert_eq!(load_stats(&env, &contract_id).total_fees, 0);
}

// ── reserve_balance updates correctly ────────────────────────────────────────

#[test]
fn test_reserve_balance_decreases_by_gross_on_cash_out() {
    let (env, client, contract_id) = setup();
    let initial_reserve = 1_000_000_000i128;
    fund(&env, &contract_id, initial_reserve);
    let wager = 10_000_000i128;
    let player = Address::generate(&env);
    inject_game(&env, &contract_id, &player, GamePhase::Revealed, 1, wager);
    client.cash_out(&player).unwrap();
    // gross = 10_000_000 * 1.9 = 19_000_000
    let expected_reserve = initial_reserve - 19_000_000;
    assert_eq!(load_stats(&env, &contract_id).reserve_balance, expected_reserve);
}

#[test]
fn test_reserve_balance_increases_on_loss() {
    let (env, client, contract_id) = setup();
    let initial_reserve = 1_000_000_000i128;
    fund(&env, &contract_id, initial_reserve);
    let wager = 5_000_000i128;
    let player = Address::generate(&env);
    // Seed 3 → loss
    let secret = make_secret(&env, 3);
    let commitment = make_commitment(&env, 3);
    client.start_game(&player, &Side::Heads, &wager, &commitment);
    client.reveal(&player, &secret).unwrap();
    assert_eq!(
        load_stats(&env, &contract_id).reserve_balance,
        initial_reserve + wager
    );
}

#[test]
fn test_reserve_balance_unchanged_on_continue_streak() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    let player = Address::generate(&env);
    inject_game(&env, &contract_id, &player, GamePhase::Revealed, 1, 5_000_000);
    let before = load_stats(&env, &contract_id).reserve_balance;
    client.continue_streak(&player, &make_commitment(&env, 42)).unwrap();
    assert_eq!(load_stats(&env, &contract_id).reserve_balance, before);
}

// ── Stats never decrease incorrectly ─────────────────────────────────────────

#[test]
fn test_total_games_never_decreases() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000_000);
    let mut prev_games = 0u64;
    for i in 0u8..5 {
        let player = Address::generate(&env);
        client.start_game(&player, &Side::Heads, &1_000_000, &make_commitment(&env, i + 1));
        let current = load_stats(&env, &contract_id).total_games;
        assert!(current >= prev_games, "total_games must never decrease");
        prev_games = current;
    }
}

#[test]
fn test_total_volume_never_decreases() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000_000);
    let mut prev_volume = 0i128;
    for i in 0u8..5 {
        let player = Address::generate(&env);
        client.start_game(&player, &Side::Heads, &1_000_000, &make_commitment(&env, i + 1));
        let current = load_stats(&env, &contract_id).total_volume;
        assert!(current >= prev_volume, "total_volume must never decrease");
        prev_volume = current;
    }
}

#[test]
fn test_total_fees_never_decreases() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000_000);
    let mut prev_fees = 0i128;
    for streak in 1u32..=4 {
        let player = Address::generate(&env);
        inject_game(&env, &contract_id, &player, GamePhase::Revealed, streak, 5_000_000);
        client.cash_out(&player).unwrap();
        let current = load_stats(&env, &contract_id).total_fees;
        assert!(current >= prev_fees, "total_fees must never decrease");
        prev_fees = current;
    }
}

// ── Property 29: Statistics accuracy ─────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// PROPERTY 29a: total_games increments by exactly 1 per start_game call.
    #[test]
    fn prop_29a_total_games_increments_by_one(
        wager in 1_000_000i128..=100_000_000i128,
    ) {
        let (env, client, contract_id) = setup();
        fund(&env, &contract_id, 1_000_000_000_000i128);
        let before = load_stats(&env, &contract_id).total_games;
        let player = Address::generate(&env);
        let commitment = BytesN::from_array(&env, &[42u8; 32]);
        client.start_game(&player, &Side::Heads, &wager, &commitment).unwrap();
        let after = load_stats(&env, &contract_id).total_games;
        prop_assert_eq!(after, before + 1);
    }

    /// PROPERTY 29b: total_volume increases by exactly the wager on each start_game.
    #[test]
    fn prop_29b_total_volume_increases_by_wager(
        wager in 1_000_000i128..=100_000_000i128,
    ) {
        let (env, client, contract_id) = setup();
        fund(&env, &contract_id, 1_000_000_000_000i128);
        let before = load_stats(&env, &contract_id).total_volume;
        let player = Address::generate(&env);
        let commitment = BytesN::from_array(&env, &[42u8; 32]);
        client.start_game(&player, &Side::Heads, &wager, &commitment).unwrap();
        let after = load_stats(&env, &contract_id).total_volume;
        prop_assert_eq!(after, before + wager);
    }

    /// PROPERTY 29c: total_fees increases by exactly the fee amount on cash_out.
    #[test]
    fn prop_29c_total_fees_increases_by_fee_on_cash_out(
        wager in 1_000_000i128..=100_000_000i128,
        streak in 1u32..=4u32,
        fee_bps in 200u32..=500u32,
    ) {
        let (env, client, contract_id) = setup();
        fund(&env, &contract_id, 1_000_000_000_000i128);
        env.as_contract(&contract_id, || {
            let mut config = CoinflipContract::load_config(&env);
            config.fee_bps = fee_bps;
            CoinflipContract::save_config(&env, &config);
        });
        let player = Address::generate(&env);
        let game = GameState {
            wager,
            side: Side::Heads,
            streak,
            commitment: BytesN::from_array(&env, &[1u8; 32]),
            contract_random: BytesN::from_array(&env, &[2u8; 32]),
            fee_bps,
            phase: GamePhase::Revealed,
            start_ledger: 0,
        };
        env.as_contract(&contract_id, || {
            CoinflipContract::save_player_game(&env, &player, &game);
        });
        let before = load_stats(&env, &contract_id).total_fees;
        client.cash_out(&player).unwrap();
        let after = load_stats(&env, &contract_id).total_fees;
        let (_, expected_fee, _) = calculate_payout_breakdown(wager, streak, fee_bps).unwrap();
        prop_assert_eq!(after, before + expected_fee);
    }

    /// PROPERTY 29d: reserve_balance decreases by gross payout on cash_out.
    #[test]
    fn prop_29d_reserve_decreases_by_gross_on_cash_out(
        wager in 1_000_000i128..=100_000_000i128,
        streak in 1u32..=4u32,
    ) {
        let (env, client, contract_id) = setup();
        fund(&env, &contract_id, 1_000_000_000_000i128);
        let player = Address::generate(&env);
        let game = GameState {
            wager,
            side: Side::Heads,
            streak,
            commitment: BytesN::from_array(&env, &[1u8; 32]),
            contract_random: BytesN::from_array(&env, &[2u8; 32]),
            fee_bps: 300,
            phase: GamePhase::Revealed,
            start_ledger: 0,
        };
        env.as_contract(&contract_id, || {
            CoinflipContract::save_player_game(&env, &player, &game);
        });
        let before = load_stats(&env, &contract_id).reserve_balance;
        client.cash_out(&player).unwrap();
        let after = load_stats(&env, &contract_id).reserve_balance;
        let (gross, _, _) = calculate_payout_breakdown(wager, streak, 300).unwrap();
        prop_assert_eq!(after, before - gross);
    }

    /// PROPERTY 29e: reserve_balance increases by wager on loss (forfeiture).
    #[test]
    fn prop_29e_reserve_increases_by_wager_on_loss(
        wager in 1_000_000i128..=100_000_000i128,
    ) {
        let (env, client, contract_id) = setup();
        fund(&env, &contract_id, 1_000_000_000_000i128);
        let player = Address::generate(&env);
        // Seed 3 → loss for Heads player
        let secret = {
            let mut b = Bytes::new(&env);
            for _ in 0..32 { b.push_back(3u8); }
            b
        };
        let commitment: BytesN<32> = env.crypto().sha256(&secret).into();
        client.start_game(&player, &Side::Heads, &wager, &commitment).unwrap();
        let before = load_stats(&env, &contract_id).reserve_balance;
        let won = client.reveal(&player, &secret).unwrap();
        prop_assume!(!won);
        let after = load_stats(&env, &contract_id).reserve_balance;
        prop_assert_eq!(after, before + wager);
    }

    /// PROPERTY 29f: total_games, total_volume are monotonically non-decreasing
    /// across a sequence of game starts.
    #[test]
    fn prop_29f_stats_monotonically_non_decreasing(
        num_games in 1usize..=10usize,
        wager in 1_000_000i128..=10_000_000i128,
    ) {
        let (env, client, contract_id) = setup();
        fund(&env, &contract_id, 1_000_000_000_000i128);
        let mut prev = load_stats(&env, &contract_id);
        for i in 0..num_games {
            let player = Address::generate(&env);
            let commitment = BytesN::from_array(&env, &[i as u8 + 1; 32]);
            client.start_game(&player, &Side::Heads, &wager, &commitment).unwrap();
            let curr = load_stats(&env, &contract_id);
            prop_assert!(curr.total_games >= prev.total_games);
            prop_assert!(curr.total_volume >= prev.total_volume);
            prev = curr;
        }
    }
}
