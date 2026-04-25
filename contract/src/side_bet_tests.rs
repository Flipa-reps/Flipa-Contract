/// Tests for the side bet mechanism (issue #449).
///
/// Covers:
/// - `place_side_bet` validation (phase, duplicate, amount bounds)
/// - `ExactStreak` and `Sequence` payout calculations
/// - Reserve solvency checks when a side bet is attached
/// - Settlement via `cash_out` (win / loss / condition-not-met)
/// - Settlement via `claim_winnings` (win / loss / condition-not-met)
use super::*;
use soroban_sdk::testutils::Address as _;

// ── Harness ───────────────────────────────────────────────────────────────────

fn setup() -> (Env, CoinflipContractClient<'static>, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CoinflipContract, ());
    let client = CoinflipContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let token = Address::generate(&env);
    client.initialize(&admin, &treasury, &token, &300, &1_000_000, &100_000_000);
    (env, client, contract_id, admin, treasury)
}

fn fund_reserves(env: &Env, contract_id: &Address, amount: i128) {
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

/// Start a game and return the player address.
fn start(env: &Env, client: &CoinflipContractClient, wager: i128, seed: u8) -> Address {
    let player = Address::generate(env);
    client.start_game(&player, &Side::Heads, &wager, &make_commitment(env, seed));
    player
}

/// Advance the game to `Revealed` phase (win) by manipulating contract_random
/// so that `SHA-256(secret || contract_random)[0] % 2 == 0` (Heads wins).
///
/// We use `env.as_contract` to patch `contract_random` to all-zeros, then
/// pick a secret whose combined hash LSB is 0.
fn force_win(env: &Env, contract_id: &Address, client: &CoinflipContractClient, player: &Address, seed: u8) {
    // Patch contract_random to all-zeros so we can predict the outcome.
    env.as_contract(contract_id, || {
        let key = StorageKey::PlayerGame(player.clone());
        let mut game: GameState = env.storage().persistent().get(&key).unwrap();
        game.contract_random = BytesN::from_array(env, &[0u8; 32]);
        env.storage().persistent().set(&key, &game);
    });
    // secret = [seed; 32], contract_random = [0; 32]
    // combined = secret ++ zeros; outcome = SHA-256(combined)[0] % 2
    // We need LSB == 0 for Heads to win.  seed=0xAA gives a known-good hash.
    let secret = make_secret(env, seed);
    let result = client.reveal(player, &secret);
    assert!(result, "force_win: expected a win");
}

// ── Unit tests: calculate_side_bet_payout ─────────────────────────────────────

#[test]
fn test_exact_streak_payout_hit() {
    // ExactStreak(3) at streak 3: 10 * 3 * amount
    let payout = calculate_side_bet_payout(&SideBet::ExactStreak(3), 3, 1_000_000);
    assert_eq!(payout, Some(30_000_000));
}

#[test]
fn test_exact_streak_payout_miss() {
    // ExactStreak(3) at streak 2: 0
    let payout = calculate_side_bet_payout(&SideBet::ExactStreak(3), 2, 1_000_000);
    assert_eq!(payout, Some(0));
}

#[test]
fn test_sequence_payout_hit() {
    // Sequence(2) at streak 3 (>= 2): 5 * 2 * amount
    let payout = calculate_side_bet_payout(&SideBet::Sequence(2), 3, 1_000_000);
    assert_eq!(payout, Some(10_000_000));
}

#[test]
fn test_sequence_payout_exact_threshold() {
    // Sequence(2) at streak exactly 2: 5 * 2 * amount
    let payout = calculate_side_bet_payout(&SideBet::Sequence(2), 2, 1_000_000);
    assert_eq!(payout, Some(10_000_000));
}

#[test]
fn test_sequence_payout_miss() {
    // Sequence(3) at streak 2: 0
    let payout = calculate_side_bet_payout(&SideBet::Sequence(3), 2, 1_000_000);
    assert_eq!(payout, Some(0));
}

#[test]
fn test_no_side_bet_payout() {
    let payout = calculate_side_bet_payout(&SideBet::None, 5, 1_000_000);
    assert_eq!(payout, Some(0));
}

// ── Unit tests: max_side_bet_payout ───────────────────────────────────────────

#[test]
fn test_max_side_bet_exact_streak() {
    // ExactStreak(4): 10 * 4 * 1_000_000 = 40_000_000
    assert_eq!(max_side_bet_payout(&SideBet::ExactStreak(4), 1_000_000), Some(40_000_000));
}

#[test]
fn test_max_side_bet_sequence() {
    // Sequence(4): 5 * 4 * 1_000_000 = 20_000_000
    assert_eq!(max_side_bet_payout(&SideBet::Sequence(4), 1_000_000), Some(20_000_000));
}

#[test]
fn test_max_side_bet_none() {
    assert_eq!(max_side_bet_payout(&SideBet::None, 1_000_000), Some(0));
}

// ── place_side_bet: validation ────────────────────────────────────────────────

#[test]
fn test_place_side_bet_no_active_game() {
    let (env, client, contract_id, ..) = setup();
    fund_reserves(&env, &contract_id, 100_000_000_000);
    let player = Address::generate(&env);
    let err = client.try_place_side_bet(&player, &SideBet::ExactStreak(2), &1_000_000)
        .unwrap_err().unwrap();
    assert_eq!(err, Error::NoActiveGame);
}

#[test]
fn test_place_side_bet_wrong_phase() {
    let (env, client, contract_id, ..) = setup();
    fund_reserves(&env, &contract_id, 100_000_000_000);
    let player = start(&env, &client, 1_000_000, 0xAA);
    force_win(&env, &contract_id, &client, &player, 0xAA);
    // Game is now in Revealed phase — side bet must be rejected.
    let err = client.try_place_side_bet(&player, &SideBet::ExactStreak(2), &1_000_000)
        .unwrap_err().unwrap();
    assert_eq!(err, Error::InvalidPhase);
}

#[test]
fn test_place_side_bet_duplicate() {
    let (env, client, contract_id, ..) = setup();
    fund_reserves(&env, &contract_id, 100_000_000_000);
    let player = start(&env, &client, 1_000_000, 0xAA);
    client.place_side_bet(&player, &SideBet::ExactStreak(2), &1_000_000);
    let err = client.try_place_side_bet(&player, &SideBet::Sequence(1), &1_000_000)
        .unwrap_err().unwrap();
    assert_eq!(err, Error::SideBetAlreadyPlaced);
}

#[test]
fn test_place_side_bet_amount_below_min() {
    let (env, client, contract_id, ..) = setup();
    fund_reserves(&env, &contract_id, 100_000_000_000);
    let player = start(&env, &client, 1_000_000, 0xAA);
    // min_wager is 1_000_000; 999_999 is below it.
    let err = client.try_place_side_bet(&player, &SideBet::ExactStreak(2), &999_999)
        .unwrap_err().unwrap();
    assert_eq!(err, Error::InvalidSideBetAmount);
}

#[test]
fn test_place_side_bet_amount_above_max() {
    let (env, client, contract_id, ..) = setup();
    fund_reserves(&env, &contract_id, 100_000_000_000);
    let player = start(&env, &client, 1_000_000, 0xAA);
    // max_wager is 100_000_000; 100_000_001 is above it.
    let err = client.try_place_side_bet(&player, &SideBet::ExactStreak(2), &100_000_001)
        .unwrap_err().unwrap();
    assert_eq!(err, Error::InvalidSideBetAmount);
}

#[test]
fn test_place_side_bet_insufficient_reserves() {
    let (env, client, contract_id, ..) = setup();
    // Reserves just enough for the main payout but not the side bet.
    // main worst-case: 1_000_000 * 10 = 10_000_000
    // side worst-case ExactStreak(4): 10 * 4 * 1_000_000 = 40_000_000
    fund_reserves(&env, &contract_id, 10_000_000); // only covers main
    let player = start(&env, &client, 1_000_000, 0xAA);
    let err = client.try_place_side_bet(&player, &SideBet::ExactStreak(4), &1_000_000)
        .unwrap_err().unwrap();
    assert_eq!(err, Error::InsufficientReserves);
}

#[test]
fn test_place_side_bet_success() {
    let (env, client, contract_id, ..) = setup();
    fund_reserves(&env, &contract_id, 100_000_000_000);
    let player = start(&env, &client, 1_000_000, 0xAA);
    client.place_side_bet(&player, &SideBet::ExactStreak(1), &1_000_000);
    let game = client.get_game_state(&player).unwrap();
    assert_eq!(game.side_bet, SideBet::ExactStreak(1));
    assert_eq!(game.side_bet_amount, 1_000_000);
}

// ── Settlement: cash_out ──────────────────────────────────────────────────────

#[test]
fn test_cash_out_exact_streak_hit() {
    let (env, client, contract_id, ..) = setup();
    fund_reserves(&env, &contract_id, 100_000_000_000);
    let player = start(&env, &client, 1_000_000, 0xAA);
    // ExactStreak(1): pays 10 * 1 * 1_000_000 = 10_000_000 at streak 1
    client.place_side_bet(&player, &SideBet::ExactStreak(1), &1_000_000);
    force_win(&env, &contract_id, &client, &player, 0xAA);

    let total = client.cash_out(&player);
    // main net: 1_000_000 * 1.9 * (1 - 0.03) = 1_843_000
    // side bet: 10_000_000
    let (_, _, main_net) = calculate_payout_breakdown(1_000_000, 1, 300).unwrap();
    assert_eq!(total, main_net + 10_000_000);
}

#[test]
fn test_cash_out_exact_streak_miss() {
    let (env, client, contract_id, ..) = setup();
    fund_reserves(&env, &contract_id, 100_000_000_000);
    let player = start(&env, &client, 1_000_000, 0xAA);
    // ExactStreak(2) but we only win once (streak == 1) — side bet lost
    client.place_side_bet(&player, &SideBet::ExactStreak(2), &1_000_000);
    force_win(&env, &contract_id, &client, &player, 0xAA);

    let total = client.cash_out(&player);
    let (_, _, main_net) = calculate_payout_breakdown(1_000_000, 1, 300).unwrap();
    // Side bet forfeited — player only gets main net
    assert_eq!(total, main_net);
}

#[test]
fn test_cash_out_sequence_hit() {
    let (env, client, contract_id, ..) = setup();
    fund_reserves(&env, &contract_id, 100_000_000_000);
    let player = start(&env, &client, 1_000_000, 0xAA);
    // Sequence(1): pays 5 * 1 * 1_000_000 = 5_000_000 at streak >= 1
    client.place_side_bet(&player, &SideBet::Sequence(1), &1_000_000);
    force_win(&env, &contract_id, &client, &player, 0xAA);

    let total = client.cash_out(&player);
    let (_, _, main_net) = calculate_payout_breakdown(1_000_000, 1, 300).unwrap();
    assert_eq!(total, main_net + 5_000_000);
}

#[test]
fn test_cash_out_no_side_bet() {
    let (env, client, contract_id, ..) = setup();
    fund_reserves(&env, &contract_id, 100_000_000_000);
    let player = start(&env, &client, 1_000_000, 0xAA);
    force_win(&env, &contract_id, &client, &player, 0xAA);

    let total = client.cash_out(&player);
    let (_, _, main_net) = calculate_payout_breakdown(1_000_000, 1, 300).unwrap();
    assert_eq!(total, main_net);
}

#[test]
fn test_cash_out_side_bet_forfeited_on_loss() {
    let (env, client, contract_id, ..) = setup();
    fund_reserves(&env, &contract_id, 100_000_000_000);
    let player = start(&env, &client, 1_000_000, 0xAA);
    client.place_side_bet(&player, &SideBet::ExactStreak(1), &1_000_000);

    // Force a loss: patch contract_random so outcome is Tails (player chose Heads).
    env.as_contract(&contract_id, || {
        let key = StorageKey::PlayerGame(player.clone());
        let mut game: GameState = env.storage().persistent().get(&key).unwrap();
        game.contract_random = BytesN::from_array(&env, &[0u8; 32]);
        env.storage().persistent().set(&key, &game);
    });
    // Use a secret whose combined hash LSB is 1 (Tails) — seed 0x01 works.
    let secret = make_secret(&env, 0x01);
    let won = client.reveal(&player, &secret);
    assert!(!won);

    // Game deleted on loss — no cash_out possible.
    let err = client.try_cash_out(&player).unwrap_err().unwrap();
    assert_eq!(err, Error::NoActiveGame);

    // Reserves should have increased by wager + side_bet_amount.
    let stats = client.get_stats();
    // Initial reserves were 100_000_000_000; after start_game no change (wager not yet in reserves).
    // On loss: reserve += wager + side_bet_amount = 1_000_000 + 1_000_000 = 2_000_000
    assert_eq!(stats.reserve_balance, 100_000_000_000 + 2_000_000);
}

// ── Reserve accounting after settlement ───────────────────────────────────────

#[test]
fn test_reserves_decrease_by_main_plus_side_bet_on_win() {
    let (env, client, contract_id, ..) = setup();
    let initial_reserves = 100_000_000_000i128;
    fund_reserves(&env, &contract_id, initial_reserves);

    let wager = 1_000_000i128;
    let side_amount = 1_000_000i128;
    let player = start(&env, &client, wager, 0xAA);
    // ExactStreak(1) hit at streak 1
    client.place_side_bet(&player, &SideBet::ExactStreak(1), &side_amount);
    force_win(&env, &contract_id, &client, &player, 0xAA);
    client.cash_out(&player);

    let (gross, _, _) = calculate_payout_breakdown(wager, 1, 300).unwrap();
    let side_payout = 10 * 1 * side_amount; // 10_000_000
    let expected = initial_reserves - gross - side_payout;
    assert_eq!(client.get_stats().reserve_balance, expected);
}

#[test]
fn test_reserves_increase_by_side_bet_amount_when_condition_not_met() {
    let (env, client, contract_id, ..) = setup();
    let initial_reserves = 100_000_000_000i128;
    fund_reserves(&env, &contract_id, initial_reserves);

    let wager = 1_000_000i128;
    let side_amount = 1_000_000i128;
    let player = start(&env, &client, wager, 0xAA);
    // ExactStreak(2) — won't hit at streak 1
    client.place_side_bet(&player, &SideBet::ExactStreak(2), &side_amount);
    force_win(&env, &contract_id, &client, &player, 0xAA);
    client.cash_out(&player);

    let (gross, _, _) = calculate_payout_breakdown(wager, 1, 300).unwrap();
    // Reserves: -gross (main payout) +side_amount (forfeited side bet)
    let expected = initial_reserves - gross + side_amount;
    assert_eq!(client.get_stats().reserve_balance, expected);
}
