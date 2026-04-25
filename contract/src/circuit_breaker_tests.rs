//! Tests for three new features:
//!
//! - #474: Reserve-threshold circuit breaker (`min_reserve_threshold`)
//! - #475: Secret stored in `claim_winnings` HistoryEntry for `verify_past_game`
//! - #476: Duplicate commitment rejection (`DuplicateCommitment`)

use super::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::testutils::Ledger;

// ── shared helpers ────────────────────────────────────────────────────────────

fn setup(env: &Env) -> (soroban_sdk::Address, CoinflipContractClient) {
    env.mock_all_auths();
    let contract_id = env.register(CoinflipContract, ());
    let client = CoinflipContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let treasury = Address::generate(env);
    let token = Address::generate(env);
    client.initialize(&admin, &treasury, &token, &300, &1_000_000, &100_000_000);
    (contract_id, client)
}

fn fund(env: &Env, contract_id: &soroban_sdk::Address, amount: i128) {
    env.as_contract(contract_id, || {
        let mut stats = CoinflipContract::load_stats(env);
        stats.reserve_balance = amount;
        CoinflipContract::save_stats(env, &stats);
    });
}

fn get_admin(env: &Env, contract_id: &soroban_sdk::Address) -> Address {
    env.as_contract(contract_id, || CoinflipContract::load_config(env).admin)
}

fn commitment_for(env: &Env, byte: u8) -> BytesN<32> {
    env.crypto()
        .sha256(&soroban_sdk::Bytes::from_slice(env, &[byte; 32]))
        .into()
}

// ═══════════════════════════════════════════════════════════════════════════
// #474 — Reserve-threshold circuit breaker
// ═══════════════════════════════════════════════════════════════════════════

/// Circuit breaker is disabled by default (threshold == 0).
#[test]
fn test_circuit_breaker_disabled_by_default() {
    let env = Env::default();
    let (contract_id, _) = setup(&env);
    let cfg: ContractConfig = env.as_contract(&contract_id, || {
        CoinflipContract::load_config(&env)
    });
    assert_eq!(cfg.min_reserve_threshold, 0);
}

/// set_min_reserve_threshold persists the value.
#[test]
fn test_set_min_reserve_threshold_persists() {
    let env = Env::default();
    let (contract_id, client) = setup(&env);
    let admin = get_admin(&env, &contract_id);
    client.set_min_reserve_threshold(&admin, &50_000_000);
    let cfg: ContractConfig = env.as_contract(&contract_id, || {
        CoinflipContract::load_config(&env)
    });
    assert_eq!(cfg.min_reserve_threshold, 50_000_000);
}

/// Non-admin cannot set the threshold.
#[test]
fn test_set_min_reserve_threshold_rejects_non_admin() {
    let env = Env::default();
    let (_, client) = setup(&env);
    let stranger = Address::generate(&env);
    let result = client.try_set_min_reserve_threshold(&stranger, &50_000_000);
    assert_eq!(result, Err(Ok(Error::Unauthorized)));
}

/// start_game is rejected when reserves are exactly at the threshold.
#[test]
fn test_circuit_breaker_triggers_at_threshold() {
    let env = Env::default();
    let (contract_id, client) = setup(&env);
    let admin = get_admin(&env, &contract_id);

    let threshold = 100_000_000i128;
    client.set_min_reserve_threshold(&admin, &threshold);
    // Fund reserves to exactly the threshold value.
    fund(&env, &contract_id, threshold);

    let player = Address::generate(&env);
    let result = client.try_start_game(&player, &Side::Heads, &1_000_000, &commitment_for(&env, 1));
    assert_eq!(result, Err(Ok(Error::ContractPaused)));
}

/// start_game is rejected when reserves are below the threshold.
#[test]
fn test_circuit_breaker_triggers_below_threshold() {
    let env = Env::default();
    let (contract_id, client) = setup(&env);
    let admin = get_admin(&env, &contract_id);

    let threshold = 100_000_000i128;
    client.set_min_reserve_threshold(&admin, &threshold);
    fund(&env, &contract_id, threshold - 1);

    let player = Address::generate(&env);
    let result = client.try_start_game(&player, &Side::Heads, &1_000_000, &commitment_for(&env, 1));
    assert_eq!(result, Err(Ok(Error::ContractPaused)));
}

/// start_game succeeds when reserves are strictly above the threshold.
#[test]
fn test_circuit_breaker_allows_game_above_threshold() {
    let env = Env::default();
    let (contract_id, client) = setup(&env);
    let admin = get_admin(&env, &contract_id);

    let threshold = 50_000_000i128;
    client.set_min_reserve_threshold(&admin, &threshold);
    // Fund well above threshold and above worst-case payout for 1_000_000 wager.
    fund(&env, &contract_id, threshold + 10_000_000);

    let player = Address::generate(&env);
    let result = client.try_start_game(&player, &Side::Heads, &1_000_000, &commitment_for(&env, 1));
    assert!(result.is_ok());
}

/// Setting threshold to 0 disables the circuit breaker.
#[test]
fn test_circuit_breaker_disabled_when_threshold_zero() {
    let env = Env::default();
    let (contract_id, client) = setup(&env);
    let admin = get_admin(&env, &contract_id);

    // Enable then disable.
    client.set_min_reserve_threshold(&admin, &100_000_000);
    client.set_min_reserve_threshold(&admin, &0);
    fund(&env, &contract_id, 1_000_000_000);

    let player = Address::generate(&env);
    let result = client.try_start_game(&player, &Side::Heads, &1_000_000, &commitment_for(&env, 1));
    assert!(result.is_ok());
}

// ═══════════════════════════════════════════════════════════════════════════
// #475 — Secret stored in claim_winnings HistoryEntry
// ═══════════════════════════════════════════════════════════════════════════

/// verify_past_game returns true for a game settled via claim_winnings
/// when the correct secret is supplied.
#[test]
fn test_claim_winnings_stores_secret_for_verification() {
    let env = Env::default();
    let (contract_id, client) = setup(&env);
    fund(&env, &contract_id, 1_000_000_000);

    let player = Address::generate(&env);
    let secret = soroban_sdk::Bytes::from_slice(&env, &[1u8; 32]);
    let commitment: BytesN<32> = env.crypto().sha256(&secret).into();

    client.start_game(&player, &Side::Heads, &10_000_000, &commitment);
    // Force a win by injecting Revealed state with streak=1.
    env.as_contract(&contract_id, || {
        let mut game = CoinflipContract::load_player_game(&env, &player).unwrap();
        game.streak = 1;
        game.phase = GamePhase::Revealed;
        CoinflipContract::save_player_game(&env, &player, &game);
    });

    client.claim_winnings(&player, &secret);

    // verify_past_game must succeed (returns true) because the secret is stored.
    let verified = client.verify_past_game(&player, &0);
    assert_eq!(verified, Ok(true));
}

/// verify_past_game returns false when an empty secret is stored (old behaviour
/// for cash_out path — regression guard).
#[test]
fn test_cash_out_empty_secret_verify_returns_false() {
    let env = Env::default();
    let (contract_id, client) = setup(&env);
    fund(&env, &contract_id, 1_000_000_000);

    let player = Address::generate(&env);
    let secret = soroban_sdk::Bytes::from_slice(&env, &[1u8; 32]);
    let commitment: BytesN<32> = env.crypto().sha256(&secret).into();

    client.start_game(&player, &Side::Heads, &10_000_000, &commitment);
    env.as_contract(&contract_id, || {
        let mut game = CoinflipContract::load_player_game(&env, &player).unwrap();
        game.streak = 1;
        game.phase = GamePhase::Revealed;
        CoinflipContract::save_player_game(&env, &player, &game);
    });

    client.cash_out(&player);

    // cash_out stores empty secret → verify_past_game returns false.
    let verified = client.verify_past_game(&player, &0);
    assert_eq!(verified, Ok(false));
}

// ═══════════════════════════════════════════════════════════════════════════
// #476 — Duplicate commitment rejection
// ═══════════════════════════════════════════════════════════════════════════

/// A commitment used in a previous game is rejected in start_game.
#[test]
fn test_duplicate_commitment_rejected_in_start_game() {
    let env = Env::default();
    let (contract_id, client) = setup(&env);
    fund(&env, &contract_id, 1_000_000_000);

    let player1 = Address::generate(&env);
    let player2 = Address::generate(&env);
    let commitment = commitment_for(&env, 42);

    // First use succeeds.
    client.start_game(&player1, &Side::Heads, &1_000_000, &commitment);

    // Same commitment for a different player must be rejected.
    let result = client.try_start_game(&player2, &Side::Heads, &1_000_000, &commitment);
    assert_eq!(result, Err(Ok(Error::DuplicateCommitment)));
}

/// A fresh commitment is always accepted.
#[test]
fn test_fresh_commitment_accepted() {
    let env = Env::default();
    let (contract_id, client) = setup(&env);
    fund(&env, &contract_id, 1_000_000_000);

    let player1 = Address::generate(&env);
    let player2 = Address::generate(&env);

    client.start_game(&player1, &Side::Heads, &1_000_000, &commitment_for(&env, 1));
    let result = client.try_start_game(&player2, &Side::Heads, &1_000_000, &commitment_for(&env, 2));
    assert!(result.is_ok());
}

/// A commitment used in start_game is rejected in continue_streak.
#[test]
fn test_duplicate_commitment_rejected_in_continue_streak() {
    let env = Env::default();
    let (contract_id, client) = setup(&env);
    fund(&env, &contract_id, 1_000_000_000);

    let player = Address::generate(&env);
    let first_commitment = commitment_for(&env, 10);

    client.start_game(&player, &Side::Heads, &1_000_000, &first_commitment);

    // Inject Revealed/win state so continue_streak is eligible.
    env.as_contract(&contract_id, || {
        let mut game = CoinflipContract::load_player_game(&env, &player).unwrap();
        game.streak = 1;
        game.phase = GamePhase::Revealed;
        CoinflipContract::save_player_game(&env, &player, &game);
    });

    // Reusing the same commitment must be rejected.
    let result = client.try_continue_streak(&player, &first_commitment);
    assert_eq!(result, Err(Ok(Error::DuplicateCommitment)));
}

/// A fresh commitment in continue_streak is accepted.
#[test]
fn test_fresh_commitment_accepted_in_continue_streak() {
    let env = Env::default();
    let (contract_id, client) = setup(&env);
    fund(&env, &contract_id, 1_000_000_000);

    let player = Address::generate(&env);
    client.start_game(&player, &Side::Heads, &1_000_000, &commitment_for(&env, 10));

    env.as_contract(&contract_id, || {
        let mut game = CoinflipContract::load_player_game(&env, &player).unwrap();
        game.streak = 1;
        game.phase = GamePhase::Revealed;
        CoinflipContract::save_player_game(&env, &player, &game);
    });

    env.ledger().with_mut(|l| l.sequence_number += 1);
    let result = client.try_continue_streak(&player, &commitment_for(&env, 99));
    assert!(result.is_ok());
}

/// No state mutation when DuplicateCommitment fires.
#[test]
fn test_duplicate_commitment_no_state_mutation() {
    let env = Env::default();
    let (contract_id, client) = setup(&env);
    fund(&env, &contract_id, 1_000_000_000);

    let player1 = Address::generate(&env);
    let player2 = Address::generate(&env);
    let commitment = commitment_for(&env, 77);

    client.start_game(&player1, &Side::Heads, &1_000_000, &commitment);

    let stats_before: ContractStats = env.as_contract(&contract_id, || {
        CoinflipContract::load_stats(&env)
    });

    let _ = client.try_start_game(&player2, &Side::Heads, &1_000_000, &commitment);

    let stats_after: ContractStats = env.as_contract(&contract_id, || {
        CoinflipContract::load_stats(&env)
    });

    assert_eq!(stats_before.total_games, stats_after.total_games);
    assert_eq!(stats_before.total_volume, stats_after.total_volume);

    // player2 must have no game stored.
    let game: Option<GameState> = env.as_contract(&contract_id, || {
        CoinflipContract::load_player_game(&env, &player2).unwrap()
    });
    assert!(game.is_none());
}
