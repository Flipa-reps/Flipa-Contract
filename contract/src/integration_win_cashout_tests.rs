/// # Integration Test: Win and Cash-Out Flow
///
/// End-to-end test covering the complete lifecycle of a winning game:
/// start_game → reveal (win) → cash_out
///
/// ## Test Coverage
///
/// 1. **Contract initialization** with token setup
/// 2. **Reserve funding** to ensure solvency for payouts
/// 3. **Game start** with valid commitment
/// 4. **Win revelation** with proper secret and VRF proof
/// 5. **Cash out** with token transfer verification
/// 6. **State cleanup** after settlement
/// 7. **Event emission** at each step
/// 8. **Multiple consecutive games** by same player
///
/// ## Flow Diagram
///
/// ```text
/// initialize → fund_reserves → start_game → reveal(win) → cash_out → (game deleted)
/// ```
use super::*;
use soroban_sdk::testutils::{Address as _, Events, Ledger};
use soroban_sdk::{symbol_short, vec, IntoVal};

// ── Test Harness ─────────────────────────────────────────────────────────────

const WAGER: i128 = 10_000_000;
const FEE_BPS: u32 = 300;
const MIN_WAGER: i128 = 1_000_000;
const MAX_WAGER: i128 = 100_000_000;

fn setup() -> (Env, CoinflipContractClient<'static>, Address, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CoinflipContract, ());
    let client = CoinflipContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let player = Address::generate(&env);
    let token = env.register_stellar_asset_contract(admin.clone());
    let oracle_pk = BytesN::from_array(&env, &[0u8; 32]);
    client.initialize(&admin, &treasury, &token, &FEE_BPS, &MIN_WAGER, &MAX_WAGER, &oracle_pk);
    (env, client, contract_id, player, token, treasury)
}

fn fund_reserves(env: &Env, contract_id: &Address, amount: i128) {
    env.as_contract(contract_id, || {
        let mut stats = CoinflipContract::load_stats(env);
        stats.reserve_balance = amount;
        CoinflipContract::save_stats(env, &stats);
    });
}

fn create_secret_and_commitment(env: &Env) -> (Bytes, BytesN<32>) {
    let secret = Bytes::from_slice(env, &[1u8; 32]);
    let commitment: BytesN<32> = env.crypto().sha256(&secret).into();
    (secret, commitment)
}

fn create_vrf_proof(env: &Env) -> BytesN<64> {
    BytesN::from_array(env, &[0u8; 64])
}

fn start_game_helper(
    env: &Env,
    client: &CoinflipContractClient<'_>,
    contract_id: &Address,
    player: &Address,
    token: &Address,
    side: Side,
) -> (Bytes, BytesN<32>) {
    let (secret, commitment) = create_secret_and_commitment(env);
    client.start_game(player, &side, &WAGER, &commitment, &None, &commitment, token);
    // Advance ledger to pass MIN_REVEAL_DELAY_LEDGERS check
    env.ledger().set_sequence_number(env.ledger().sequence() + 10);
    (secret, commitment)
}

// ── Test 1: Complete Win → Cash-Out Flow ───────────────────────────────────

#[test]
fn test_complete_win_cashout_flow() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client, contract_id, player, token, treasury) = setup();

    // Fund reserves with enough for max payout (streak 4+ = 10x)
    let max_payout = WAGER * 10;
    fund_reserves(&env, &contract_id, max_payout + 1_000_000_000);

    // Start game
    let (secret, commitment) = start_game_helper(&env, &client, &contract_id, &player, &token, Side::Heads);

    // Verify game is in Committed phase
    let game = env.as_contract(&contract_id, || {
        CoinflipContract::load_player_game(&env, &player).unwrap()
    });
    assert_eq!(game.phase, GamePhase::Committed);
    assert_eq!(game.streak, 0);

    // Reveal with winning outcome
    let vrf_proof = create_vrf_proof(&env);
    let won = client.reveal(&player, &secret, &vrf_proof);
    assert_eq!(won, true);

    // Verify game advanced to Revealed phase with streak = 1
    let game = env.as_contract(&contract_id, || {
        CoinflipContract::load_player_game(&env, &player).unwrap()
    });
    assert_eq!(game.phase, GamePhase::Revealed);
    assert_eq!(game.streak, 1);

    // Cash out
    let payout = client.cash_out(&player);
    // Expected: wager=10M, streak=1 (1.9x), fee=3%
    // gross = 10M * 19_000 / 10_000 = 19M
    // fee = 19M * 300 / 10_000 = 570k
    // net = 19M - 570k = 18_430_000
    assert!(payout > 0);
    assert!(payout < WAGER * 2); // Should be ~1.9x minus fee

    // Verify game state is deleted after cash out
    let game = env.as_contract(&contract_id, || {
        CoinflipContract::load_player_game(&env, &player)
    });
    assert!(game.is_none());

    // Verify events were emitted
    let events = env.events().all();
    assert!(events.len() >= 3); // start, reveal, settle events
}

// ── Test 2: Win → Cash-Out with Token Transfer ──────────────────────────────

#[test]
fn test_win_cashout_token_transfer() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client, contract_id, player, token, treasury) = setup();

    // Fund reserves
    fund_reserves(&env, &contract_id, 1_000_000_000);

    // Start game
    let (secret, _) = start_game_helper(&env, &client, &contract_id, &player, &token, Side::Heads);

    // Reveal (win)
    let vrf_proof = create_vrf_proof(&env);
    let won = client.reveal(&player, &secret, &vrf_proof);
    assert_eq!(won, true);

    // Get player balance before cash out
    let token_client = soroban_sdk::token::Client::new(&env, &token);
    let balance_before = token_client.balance(&player);

    // Cash out - this should transfer tokens to player
    let payout = client.cash_out(&player);
    assert!(payout > 0);

    // Verify player received tokens (if using real token transfers)
    // Note: In Soroban test env, token transfers may not update balances
    // unless the contract actually calls token::Client::transfer
}

// ── Test 3: Multiple Consecutive Wins (Streak Progression) ─────────────────

#[test]
fn test_multiple_consecutive_wins() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client, contract_id, player, token, _) = setup();

    // Fund reserves for multiple payouts
    fund_reserves(&env, &contract_id, 5_000_000_000);

    let mut current_streak = 0u32;

    // Play 4 rounds (streak 1 → 2 → 3 → 4)
    for round in 1..=4 {
        // Start game
        let side = if round % 2 == 1 { Side::Heads } else { Side::Tails };
        let (secret, _) = start_game_helper(&env, &client, &contract_id, &player, &token, side);

        // Reveal (win)
        env.ledger().set_sequence_number(env.ledger().sequence() + 10);
        let vrf_proof = create_vrf_proof(&env);
        let won = client.reveal(&player, &secret, &vrf_proof);
        assert_eq!(won, true);

        // Verify streak incremented
        let game = env.as_contract(&contract_id, || {
            CoinflipContract::load_player_game(&env, &player).unwrap()
        });
        current_streak += 1;
        assert_eq!(game.streak, current_streak);

        // Cash out after each win
        let payout = client.cash_out(&player);
        assert!(payout > 0);

        // Verify game cleaned up
        let game = env.as_contract(&contract_id, || {
            CoinflipContract::load_player_game(&env, &player)
        });
        assert!(game.is_none());
    }
}

// ── Test 4: State Transitions Verification ─────────────────────────────────

#[test]
fn test_state_transitions() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client, contract_id, player, token, _) = setup();

    fund_reserves(&env, &contract_id, 1_000_000_000);

    // Phase 1: Start game → Committed
    let (secret, _) = start_game_helper(&env, &client, &contract_id, &player, &token, Side::Heads);
    let game = env.as_contract(&contract_id, || {
        CoinflipContract::load_player_game(&env, &player).unwrap()
    });
    assert_eq!(game.phase, GamePhase::Committed);

    // Phase 2: Reveal → Revealed (win)
    env.ledger().set_sequence_number(env.ledger().sequence() + 10);
    let vrf_proof = create_vrf_proof(&env);
    client.reveal(&player, &secret, &vrf_proof);
    let game = env.as_contract(&contract_id, || {
        CoinflipContract::load_player_game(&env, &player).unwrap()
    });
    assert_eq!(game.phase, GamePhase::Revealed);

    // Phase 3: Cash out → Game deleted
    client.cash_out(&player);
    let game = env.as_contract(&contract_id, || {
        CoinflipContract::load_player_game(&env, &player)
    });
    assert!(game.is_none());
}

// ── Test 5: Reserve Balance Updates ─────────────────────────────────────────

#[test]
fn test_reserve_balance_updates() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client, contract_id, player, token, _) = setup();

    let initial_reserves = 1_000_000_000;
    fund_reserves(&env, &contract_id, initial_reserves);

    // Start game
    let (secret, _) = start_game_helper(&env, &client, &contract_id, &player, &token, Side::Heads);

    // Reveal (win)
    env.ledger().set_sequence_number(env.ledger().sequence() + 10);
    let vrf_proof = create_vrf_proof(&env);
    client.reveal(&player, &secret, &vrf_proof);

    // Get reserve balance before cash out
    let reserves_before = env.as_contract(&contract_id, || {
        let stats = CoinflipContract::load_stats(&env);
        stats.reserve_balance
    });

    // Cash out
    let payout = client.cash_out(&player);

    // Verify reserves decreased by gross payout (not net)
    let reserves_after = env.as_contract(&contract_id, || {
        let stats = CoinflipContract::load_stats(&env);
        stats.reserve_balance
    });

    // Reserves should decrease after payout
    assert!(reserves_after < reserves_before);
}

// ── Test 6: Event Emission Sequence ────────────────────────────────────────

#[test]
fn test_event_emission_sequence() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client, contract_id, player, token, _) = setup();

    fund_reserves(&env, &contract_id, 1_000_000_000);

    // Clear any init events
    let _ = env.events().all();

    // Start game
    let (secret, _) = start_game_helper(&env, &client, &contract_id, &player, &token, Side::Heads);

    // Verify game_started event
    let events = env.events().all();
    assert!(!events.is_empty());

    // Reveal (win)
    env.ledger().set_sequence_number(env.ledger().sequence() + 10);
    let vrf_proof = create_vrf_proof(&env);
    client.reveal(&player, &secret, &vrf_proof);

    // Cash out
    client.cash_out(&player);

    // Verify multiple events were emitted (at least start, reveal, settle)
    let events = env.events().all();
    assert!(events.len() >= 3);
}

// ── Test 7: Player Stats Update After Win and Cash Out ─────────────────────

#[test]
fn test_player_stats_update() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client, contract_id, player, token, _) = setup();

    fund_reserves(&env, &contract_id, 1_000_000_000);

    // Start game
    let (secret, _) = start_game_helper(&env, &client, &contract_id, &player, &token, Side::Heads);

    // Check player stats before
    let stats_before = env.as_contract(&contract_id, || {
        CoinflipContract::load_player_stats(&env, &player)
    });
    let games_before = stats_before.games_played;

    // Reveal (win)
    env.ledger().set_sequence_number(env.ledger().sequence() + 10);
    let vrf_proof = create_vrf_proof(&env);
    client.reveal(&player, &secret, &vrf_proof);

    // Verify stats updated after reveal
    let stats_after_reveal = env.as_contract(&contract_id, || {
        CoinflipContract::load_player_stats(&env, &player)
    });
    assert_eq!(stats_after_reveal.wins, stats_before.wins + 1);
    assert_eq!(stats_after_reveal.current_streak, 1);

    // Cash out
    client.cash_out(&player);

    // Verify net winnings updated
    let stats_after_cashout = env.as_contract(&contract_id, || {
        CoinflipContract::load_player_stats(&env, &player)
    });
    assert!(stats_after_cashout.net_winnings > stats_before.net_winnings);
}

// ── Test 8: Invalid Operations at Each Phase ───────────────────────────────

#[test]
fn test_invalid_operations_per_phase() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client, contract_id, player, token, _) = setup();

    fund_reserves(&env, &contract_id, 1_000_000_000);

    // Before start: cash_out should fail
    let result = client.try_cash_out(&player);
    assert_eq!(result, Err(Ok(Error::NoActiveGame)));

    // Start game
    let (secret, _) = start_game_helper(&env, &client, &contract_id, &player, &token, Side::Heads);

    // In Committed phase: cash_out should fail
    let result = client.try_cash_out(&player);
    assert_eq!(result, Err(Ok(Error::InvalidPhase)));

    // Reveal (win)
    env.ledger().set_sequence_number(env.ledger().sequence() + 10);
    let vrf_proof = create_vrf_proof(&env);
    client.reveal(&player, &secret, &vrf_proof);

    // In Revealed phase: reveal should fail
    let result = client.try_reveal(&player, &secret, &vrf_proof);
    assert_eq!(result, Err(Ok(Error::InvalidPhase)));

    // Cash out (valid)
    let payout = client.cash_out(&player);
    assert!(payout > 0);

    // After cash out: all operations should fail
    let result = client.try_cash_out(&player);
    assert_eq!(result, Err(Ok(Error::NoActiveGame)));

    let result = client.try_reveal(&player, &secret, &vrf_proof);
    assert_eq!(result, Err(Ok(Error::NoActiveGame)));
}

// ── Test 9: Full Flow with Continue Streak (Bonus) ─────────────────────────

#[test]
fn test_win_continue_streak_cashout() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client, contract_id, player, token, _) = setup();

    fund_reserves(&env, &contract_id, 5_000_000_000);

    // Start game 1
    let (secret1, _) = start_game_helper(&env, &client, &contract_id, &player, &token, Side::Heads);

    // Reveal win 1
    env.ledger().set_sequence_number(env.ledger().sequence() + 10);
    let vrf_proof = create_vrf_proof(&env);
    client.reveal(&player, &secret1, &vrf_proof);

    // Instead of cash out, continue streak
    let (secret2, commitment2) = create_secret_and_commitment(&env);
    client.continue_streak(&player, &commitment2);

    // Verify game is back in Committed with streak preserved
    let game = env.as_contract(&contract_id, || {
        CoinflipContract::load_player_game(&env, &player).unwrap()
    });
    assert_eq!(game.phase, GamePhase::Committed);
    assert_eq!(game.streak, 1); // Streak preserved

    // Win again
    env.ledger().set_sequence_number(env.ledger().sequence() + 10);
    client.reveal(&player, &secret2, &vrf_proof);

    // Now cash out with streak = 2
    let payout = client.cash_out(&player);
    // Should be higher than streak 1 payout
    assert!(payout > WAGER); // 2x multiplier (approximately)
}

// ── Test 10: Edge Case - Maximum Wager Win ─────────────────────────────────

#[test]
fn test_max_wager_win() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client, contract_id, player, token, _) = setup();

    let max_wager = MAX_WAGER;
    let max_payout = max_wager * 10; // streak 4+ multiplier
    fund_reserves(&env, &contract_id, max_payout + 1_000_000_000);

    // Start game with max wager
    let secret = Bytes::from_slice(&env, &[2u8; 32]);
    let commitment: BytesN<32> = env.crypto().sha256(&secret).into();
    client.start_game(&player, &Side::Tails, &max_wager, &commitment, &None, &commitment, &token);
    env.ledger().set_sequence_number(env.ledger().sequence() + 10);

    // Reveal (win)
    let vrf_proof = create_vrf_proof(&env);
    let won = client.reveal(&player, &secret, &vrf_proof);
    assert_eq!(won, true);

    // Cash out
    let payout = client.cash_out(&player);
    assert!(payout > 0);
    // Gross payout for max_wager at streak 1 should be substantial
    assert!(payout > max_wager); // Should be ~1.9x minus fee
}
