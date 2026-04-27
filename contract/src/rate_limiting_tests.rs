//! # Rate Limiting Tests (#472)
//!
//! Tests for per-player and global rate limiting / DOS protection.

use super::*;
use soroban_sdk::{testutils::{Address as _, Ledger}, Env};

fn setup() -> (Env, CoinflipContractClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CoinflipContract, ());
    let client = CoinflipContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let token = Address::generate(&env);
    client.initialize(
        &admin, &treasury, &token,
        &300, &1_000_000, &100_000_000,
        &BytesN::from_array(&env, &[0u8; 32]),
    );
    (env, client, admin, treasury)
}

// ── get_player_rate_limit ─────────────────────────────────────────────────────

#[test]
fn test_get_player_rate_limit_initial_state() {
    let (env, client, _, _) = setup();
    let player = Address::generate(&env);
    let state = client.get_player_rate_limit(&player);
    assert_eq!(state.count, 0);
    assert_eq!(state.window_start, 0);
}

// ── get_global_rate_limit ─────────────────────────────────────────────────────

#[test]
fn test_get_global_rate_limit_initial_state() {
    let (_, client, _, _) = setup();
    let state = client.get_global_rate_limit();
    assert_eq!(state.count, 0);
    assert_eq!(state.window_start, 0);
}

// ── reset_player_rate_limit ───────────────────────────────────────────────────

#[test]
fn test_reset_player_rate_limit_by_admin() {
    let (env, client, admin, _) = setup();
    let player = Address::generate(&env);
    // Reset should succeed for admin.
    assert!(client.try_reset_player_rate_limit(&admin, &player).is_ok());
    let state = client.get_player_rate_limit(&player);
    assert_eq!(state.count, 0);
}

#[test]
fn test_reset_player_rate_limit_rejected_for_non_admin() {
    let (env, client, _, _) = setup();
    let player = Address::generate(&env);
    let non_admin = Address::generate(&env);
    let result = client.try_reset_player_rate_limit(&non_admin, &player);
    assert!(result.is_err());
}

// ── rate limit constants ──────────────────────────────────────────────────────

#[test]
fn test_rate_limit_constants_are_reasonable() {
    assert!(RATE_LIMIT_PER_PLAYER > 0);
    assert!(RATE_LIMIT_GLOBAL > RATE_LIMIT_PER_PLAYER);
    assert!(RATE_LIMIT_WINDOW_LEDGERS > 0);
}
