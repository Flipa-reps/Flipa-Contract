/// Property-based tests for contract statistics accuracy.
///
/// # Stat Semantics
///
/// | Field             | Updated by          | Direction  | Invariant                                      |
/// |-------------------|---------------------|------------|------------------------------------------------|
/// | `total_games`     | `start_game`        | +1         | Strictly monotone; never decremented           |
/// | `total_volume`    | `start_game`        | +wager     | Strictly monotone; never decremented           |
/// | `total_fees`      | `cash_out` / `claim_winnings` | +fee | Monotone; only increases on settled wins  |
/// | `reserve_balance` | `start_game` (check only), `reveal` (loss: +wager), `cash_out` / `claim_winnings` (win: -gross) | ± | Must stay ≥ 0 |
///
/// # Coverage
///
/// - Property 29a: `total_games` increments by exactly 1 per `start_game`
/// - Property 29b: `total_volume` increases by exactly the wager per `start_game`
/// - Property 29c: `total_fees` increases by exactly the fee on `cash_out`
/// - Property 29d: `reserve_balance` decreases by gross payout on `cash_out`
/// - Property 29e: `reserve_balance` increases by wager on loss forfeiture
/// - Property 29f: `total_games` and `total_volume` are monotonically non-decreasing
/// - Multi-game sequences: concurrent starts, mixed win/loss/continue flows
/// - Edge cases: failed starts, 100-game accumulation, fee-free loss path
use super::*;
use soroban_sdk::testutils::Address as _;
use proptest::prelude::*;

// ── Harness ───────────────────────────────────────────────────────────────────

/// Set up a fresh contract environment with default config.
///
/// Returns `(env, client, contract_id)`.
/// Config: fee=300bps, min_wager=1_000_000, max_wager=100_000_000.
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

/// Directly set `reserve_balance` in stats storage, bypassing `start_game` guards.
fn fund(env: &Env, contract_id: &Address, amount: i128) {
    env.as_contract(contract_id, || {
        let mut stats = CoinflipContract::load_stats(env);
        stats.reserve_balance = amount;
        CoinflipContract::save_stats(env, &stats);
    });
}

/// Build a 32-byte `Bytes` value filled with `seed`.
fn make_secret(env: &Env, seed: u8) -> Bytes {
    let mut b = Bytes::new(env);
    for _ in 0..32 {
        b.push_back(seed);
    }
    b
}

/// SHA-256 of a 32-byte secret filled with `seed`.
fn make_commitment(env: &Env, seed: u8) -> BytesN<32> {
    env.crypto().sha256(&make_secret(env, seed)).into()
}

/// Read stats from inside the contract's storage context.
fn load_stats(env: &Env, contract_id: &Address) -> ContractStats {
    env.as_contract(contract_id, || CoinflipContract::load_stats(env))
}

/// Inject a `GameState` directly into storage, bypassing `start_game` guards.
///
/// Useful for testing settlement paths (`cash_out`, `claim_winnings`) in
/// isolation without needing a valid commit-reveal sequence.
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
    // No reserves — start_game will fail with InsufficientReserves
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
    // reveal + cash_out must not change total_games
    client.reveal(&player, &make_secret(&env, 1));
    client.cash_out(&player);
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
    client.reveal(&player, &make_secret(&env, 1));
    client.cash_out(&player);
    // total_volume must not change after reveal/cash_out
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
    // Inject a Revealed game at streak=1 so cash_out settles it
    inject_game(&env, &contract_id, &player, GamePhase::Revealed, 1, wager);
    client.cash_out(&player);
    // gross = 10_000_000 * 1.9 = 19_000_000; fee = 19_000_000 * 300/10_000 = 570_000
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
        client.cash_out(&player);
    }
    assert_eq!(load_stats(&env, &contract_id).total_fees, expected_fees);
}

#[test]
fn test_total_fees_does_not_accumulate_on_loss() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000);
    let player = Address::generate(&env);
    // Seed 3 → loss for Heads player (verified by generate_outcome logic)
    let secret = make_secret(&env, 3);
    let commitment = make_commitment(&env, 3);
    client.start_game(&player, &Side::Heads, &5_000_000, &commitment);
    client.reveal(&player, &secret);
    // No fee should be collected on a loss
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
    client.cash_out(&player);
    // gross = 10_000_000 * 19_000 / 10_000 = 19_000_000
    let (gross, _, _) = calculate_payout_breakdown(wager, 1, 300).unwrap();
    assert_eq!(
        load_stats(&env, &contract_id).reserve_balance,
        initial_reserve - gross
    );
}

#[test]
fn test_reserve_balance_increases_on_loss() {
    let (env, client, contract_id) = setup();
    let initial_reserve = 1_000_000_000i128;
    fund(&env, &contract_id, initial_reserve);
    let wager = 5_000_000i128;
    let player = Address::generate(&env);
    // Seed 3 → loss for Heads player
    let secret = make_secret(&env, 3);
    let commitment = make_commitment(&env, 3);
    client.start_game(&player, &Side::Heads, &wager, &commitment);
    client.reveal(&player, &secret);
    // On loss, wager is forfeited to reserves
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
    // continue_streak transitions phase but does not touch reserve_balance
    client.continue_streak(&player, &make_commitment(&env, 42));
    assert_eq!(load_stats(&env, &contract_id).reserve_balance, before);
}

#[test]
fn test_reserve_balance_unchanged_by_start_game() {
    // start_game only checks reserve_balance; it does not decrement it.
    let (env, client, contract_id) = setup();
    let initial_reserve = 1_000_000_000i128;
    fund(&env, &contract_id, initial_reserve);
    let player = Address::generate(&env);
    client.start_game(&player, &Side::Heads, &5_000_000, &make_commitment(&env, 1));
    assert_eq!(
        load_stats(&env, &contract_id).reserve_balance,
        initial_reserve
    );
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
        client.cash_out(&player);
        let current = load_stats(&env, &contract_id).total_fees;
        assert!(current >= prev_fees, "total_fees must never decrease");
        prev_fees = current;
    }
}

// ── Property 29: Statistics accuracy ─────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// PROPERTY 29a: `total_games` increments by exactly 1 per `start_game` call.
    #[test]
    fn prop_29a_total_games_increments_by_one(
        wager in 1_000_000i128..=100_000_000i128,
    ) {
        let (env, client, contract_id) = setup();
        fund(&env, &contract_id, 1_000_000_000_000i128);
        let before = load_stats(&env, &contract_id).total_games;
        let player = Address::generate(&env);
        let commitment = BytesN::from_array(&env, &[42u8; 32]);
        client.start_game(&player, &Side::Heads, &wager, &commitment);
        let after = load_stats(&env, &contract_id).total_games;
        prop_assert_eq!(after, before + 1);
    }

    /// PROPERTY 29b: `total_volume` increases by exactly the wager on each `start_game`.
    #[test]
    fn prop_29b_total_volume_increases_by_wager(
        wager in 1_000_000i128..=100_000_000i128,
    ) {
        let (env, client, contract_id) = setup();
        fund(&env, &contract_id, 1_000_000_000_000i128);
        let before = load_stats(&env, &contract_id).total_volume;
        let player = Address::generate(&env);
        let commitment = BytesN::from_array(&env, &[42u8; 32]);
        client.start_game(&player, &Side::Heads, &wager, &commitment);
        let after = load_stats(&env, &contract_id).total_volume;
        prop_assert_eq!(after, before + wager);
    }

    /// PROPERTY 29c: `total_fees` increases by exactly the fee amount on `cash_out`.
    ///
    /// Uses `inject_game` to place a `Revealed` game with a known `fee_bps` snapshot,
    /// then verifies the fee delta matches `calculate_payout_breakdown`.
    #[test]
    fn prop_29c_total_fees_increases_by_fee_on_cash_out(
        wager in 1_000_000i128..=100_000_000i128,
        streak in 1u32..=4u32,
        fee_bps in 200u32..=500u32,
    ) {
        let (env, client, contract_id) = setup();
        fund(&env, &contract_id, 1_000_000_000_000i128);
        let player = Address::generate(&env);
        // Inject a Revealed game with the property-generated fee_bps snapshot
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
        client.cash_out(&player);
        let after = load_stats(&env, &contract_id).total_fees;
        let (_, expected_fee, _) = calculate_payout_breakdown(wager, streak, fee_bps).unwrap();
        prop_assert_eq!(after, before + expected_fee);
    }

    /// PROPERTY 29d: `reserve_balance` decreases by gross payout on `cash_out`.
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
        client.cash_out(&player);
        let after = load_stats(&env, &contract_id).reserve_balance;
        let (gross, _, _) = calculate_payout_breakdown(wager, streak, 300).unwrap();
        prop_assert_eq!(after, before - gross);
    }

    /// PROPERTY 29e: `reserve_balance` increases by wager on loss forfeiture.
    ///
    /// Uses seed=3 which is known to produce a loss for a Heads player.
    /// `prop_assume!(!won)` guards against the rare case where the seed
    /// produces a win (should not happen for seed=3, but is defensive).
    #[test]
    fn prop_29e_reserve_increases_by_wager_on_loss(
        wager in 1_000_000i128..=100_000_000i128,
    ) {
        let (env, client, contract_id) = setup();
        fund(&env, &contract_id, 1_000_000_000_000i128);
        let player = Address::generate(&env);
        let secret = make_secret(&env, 3);
        let commitment: BytesN<32> = env.crypto().sha256(&secret).into();
        client.start_game(&player, &Side::Heads, &wager, &commitment);
        let before = load_stats(&env, &contract_id).reserve_balance;
        let won = client.reveal(&player, &secret);
        prop_assume!(!won);
        let after = load_stats(&env, &contract_id).reserve_balance;
        prop_assert_eq!(after, before + wager);
    }

    /// PROPERTY 29f: `total_games` and `total_volume` are monotonically non-decreasing
    /// across a sequence of `start_game` calls with varying wagers.
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
            client.start_game(&player, &Side::Heads, &wager, &commitment);
            let curr = load_stats(&env, &contract_id);
            prop_assert!(curr.total_games >= prev.total_games,
                "total_games decreased: {} -> {}", prev.total_games, curr.total_games);
            prop_assert!(curr.total_volume >= prev.total_volume,
                "total_volume decreased: {} -> {}", prev.total_volume, curr.total_volume);
            prev = curr;
        }
    }
}

// ── Multi-game sequence tests ─────────────────────────────────────────────────

/// Verify that starting N games from distinct players accumulates stats correctly.
/// Uses counters instead of std::Vec to stay no_std compatible.
#[test]
fn test_concurrent_games_accumulate_statistics() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000_000);

    let mut expected_volume = 0i128;
    for i in 0u8..10 {
        let player = Address::generate(&env);
        let wager = 1_000_000i128 * (i as i128 + 1);
        expected_volume += wager;
        client.start_game(&player, &Side::Heads, &wager, &make_commitment(&env, i + 1));
    }

    let stats = load_stats(&env, &contract_id);
    assert_eq!(stats.total_games, 10);
    assert_eq!(stats.total_volume, expected_volume);
}

/// Verify that settling N games accumulates `total_fees` correctly.
///
/// Bug fix: the original test injected one set of players to compute
/// `expected_fees`, then injected a *different* set of players to cash out,
/// so the expected and actual values were unrelated.  This version injects
/// and cashes out the same player in each iteration.
#[test]
fn test_concurrent_settlements_accumulate_fees() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000_000);

    let mut expected_fees = 0i128;
    for i in 0u32..5 {
        let player = Address::generate(&env);
        let wager = 10_000_000i128;
        let streak = (i % 4) + 1;
        inject_game(&env, &contract_id, &player, GamePhase::Revealed, streak, wager);
        let (_gross, fee, _net) = calculate_payout_breakdown(wager, streak, 300).unwrap();
        expected_fees += fee;
        client.cash_out(&player);
    }

    assert_eq!(load_stats(&env, &contract_id).total_fees, expected_fees);
}

/// Verify that a mix of wins and losses produces the correct `reserve_balance`.
///
/// - 3 winning cash-outs each deduct `gross` from reserves.
/// - 2 losses each add `wager` to reserves.
#[test]
fn test_concurrent_wins_and_losses_update_reserve() {
    let (env, client, contract_id) = setup();
    let initial_reserve = 1_000_000_000i128;
    fund(&env, &contract_id, initial_reserve);

    let mut net_change = 0i128;

    // 3 winning cash-outs
    for _ in 0u8..3 {
        let player = Address::generate(&env);
        let wager = 5_000_000i128;
        inject_game(&env, &contract_id, &player, GamePhase::Revealed, 1, wager);
        let (gross, _, _) = calculate_payout_breakdown(wager, 1, 300).unwrap();
        net_change -= gross;
        client.cash_out(&player);
    }

    // 2 losses (seed 3 → Tails outcome, Heads player loses)
    for _ in 0u8..2 {
        let player = Address::generate(&env);
        let wager = 5_000_000i128;
        let secret = make_secret(&env, 3);
        let commitment = make_commitment(&env, 3);
        client.start_game(&player, &Side::Heads, &wager, &commitment);
        client.reveal(&player, &secret);
        net_change += wager;
    }

    assert_eq!(
        load_stats(&env, &contract_id).reserve_balance,
        initial_reserve + net_change
    );
}

/// Verify stat consistency across a mixed sequence: win+cash_out, loss, win+continue.
///
/// Uses `inject_game` for the win paths to avoid dependence on seed-based
/// outcome determinism for non-loss seeds.
#[test]
fn test_statistics_consistency_with_mixed_operations() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000_000);

    let player1 = Address::generate(&env);
    let player2 = Address::generate(&env);
    let player3 = Address::generate(&env);

    // Player 1: injected win → cash_out
    inject_game(&env, &contract_id, &player1, GamePhase::Revealed, 1, 10_000_000);
    client.cash_out(&player1);

    // Player 2: real start → loss (seed 3 → Tails, Heads player loses)
    let secret2 = make_secret(&env, 3);
    let commitment2 = make_commitment(&env, 3);
    client.start_game(&player2, &Side::Heads, &5_000_000, &commitment2);
    client.reveal(&player2, &secret2);

    // Player 3: injected win → continue_streak
    inject_game(&env, &contract_id, &player3, GamePhase::Revealed, 1, 7_000_000);
    client.continue_streak(&player3, &make_commitment(&env, 42));

    let stats = load_stats(&env, &contract_id);
    // Only player2's start_game increments total_games (players 1 and 3 were injected)
    assert_eq!(stats.total_games, 1);
    // Only player2's wager was recorded via start_game
    assert_eq!(stats.total_volume, 5_000_000);
    // Player 1's cash_out collected a fee
    assert!(stats.total_fees > 0);
    // Reserve must remain positive
    assert!(stats.reserve_balance > 0);
}

// ── Edge cases ────────────────────────────────────────────────────────────────

/// All stat fields must be non-negative after 10 game starts.
///
/// Note: `total_games` is `u64` so `>= 0` is always true; the meaningful
/// check is that `total_volume`, `total_fees`, and `reserve_balance` (all
/// `i128`) remain non-negative.  We also assert the exact `total_games` count.
#[test]
fn test_statistics_never_become_negative() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000_000);

    for i in 0u8..10 {
        let player = Address::generate(&env);
        client.start_game(&player, &Side::Heads, &1_000_000, &make_commitment(&env, i + 1));
    }

    let stats = load_stats(&env, &contract_id);
    assert_eq!(stats.total_games, 10);
    assert!(stats.total_volume >= 0, "total_volume must be non-negative");
    assert!(stats.total_fees >= 0, "total_fees must be non-negative");
    assert!(stats.reserve_balance >= 0, "reserve_balance must be non-negative");
}

/// Verify that 100 sequential game starts accumulate stats correctly.
#[test]
fn test_statistics_with_large_number_of_games() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 10_000_000_000_000);

    let wager = 1_000_000i128;
    let n = 100u8;
    for i in 0..n {
        let player = Address::generate(&env);
        client.start_game(&player, &Side::Heads, &wager, &make_commitment(&env, i));
    }

    let stats = load_stats(&env, &contract_id);
    assert_eq!(stats.total_games, n as u64);
    assert_eq!(stats.total_volume, wager * n as i128);
}

/// Verify that `total_fees` accumulates correctly across all four streak tiers.
#[test]
fn test_fees_accumulate_across_all_streak_tiers() {
    let (env, client, contract_id) = setup();
    fund(&env, &contract_id, 1_000_000_000_000);

    let wager = 10_000_000i128;
    let fee_bps = 300u32;
    let mut expected = 0i128;

    for streak in 1u32..=4 {
        let player = Address::generate(&env);
        inject_game(&env, &contract_id, &player, GamePhase::Revealed, streak, wager);
        let (_, fee, _) = calculate_payout_breakdown(wager, streak, fee_bps).unwrap();
        expected += fee;
        client.cash_out(&player);
    }

    assert_eq!(load_stats(&env, &contract_id).total_fees, expected);
}

/// Verify that `reserve_balance` is correctly decremented across multiple cash-outs.
#[test]
fn test_reserve_decrements_correctly_across_multiple_cash_outs() {
    let (env, client, contract_id) = setup();
    let initial = 1_000_000_000_000i128;
    fund(&env, &contract_id, initial);

    let wager = 5_000_000i128;
    let mut total_gross = 0i128;

    for streak in 1u32..=4 {
        let player = Address::generate(&env);
        inject_game(&env, &contract_id, &player, GamePhase::Revealed, streak, wager);
        let (gross, _, _) = calculate_payout_breakdown(wager, streak, 300).unwrap();
        total_gross += gross;
        client.cash_out(&player);
    }

    assert_eq!(
        load_stats(&env, &contract_id).reserve_balance,
        initial - total_gross
    );
}
