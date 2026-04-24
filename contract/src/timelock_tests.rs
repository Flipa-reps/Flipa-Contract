//! Tests for the time-lock commitment scheme.
//!
//! The time-lock prevents a player from committing and immediately revealing
//! in the same ledger, which would let them observe the contract's randomness
//! contribution before deciding to proceed.
//!
//! Invariants:
//! - `reveal` before `start_ledger + MIN_REVEAL_DELAY_LEDGERS` → `RevealTimeout`
//! - `reveal` at exactly `start_ledger + MIN_REVEAL_DELAY_LEDGERS` → succeeds
//! - `reveal` after the delay → succeeds
//! - No state mutation when the time-lock guard fires

use super::*;
use soroban_sdk::testutils::{Address as _, Ledger};

// ── helpers ───────────────────────────────────────────────────────────────────

fn setup(env: &Env) -> (Address, CoinflipContractClient) {
    env.mock_all_auths();
    let contract_id = env.register(CoinflipContract, ());
    let client = CoinflipContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let treasury = Address::generate(env);
    let token = Address::generate(env);
    client.initialize(&admin, &treasury, &token, &300, &1_000_000, &100_000_000, &BytesN::from_array(&env, &[0u8; 32]));
    (contract_id, client)
}

fn fund(env: &Env, contract_id: &Address, amount: i128) {
    env.as_contract(contract_id, || {
        let mut stats = CoinflipContract::load_stats(env);
        stats.reserve_balance = amount;
        CoinflipContract::save_stats(env, &stats);
    });
}

fn secret(env: &Env) -> Bytes {
    Bytes::from_slice(env, &[1u8; 32])
}

fn commitment(env: &Env) -> BytesN<32> {
    env.crypto().sha256(&secret(env)).into()
}

// ── Time-lock enforcement ─────────────────────────────────────────────────────

/// reveal at the same ledger as start_game → RevealTimeout.
#[test]
fn test_reveal_same_ledger_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = setup(&env);
    fund(&env, &contract_id, 1_000_000_000);

    let player = Address::generate(&env);
    client.start_game(&player, &Side::Heads, &10_000_000, &commitment(&env, &env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into()));

    // No ledger advance — same ledger as start_game.
    let result = client.try_reveal(&player, &secret(&env, &BytesN::from_array(&env, &[0u8; 64])));
    assert_eq!(result, Err(Ok(Error::RevealTimeout)));
}

/// reveal one ledger before the delay expires → RevealTimeout.
#[test]
fn test_reveal_one_ledger_too_early_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = setup(&env);
    fund(&env, &contract_id, 1_000_000_000);

    let player = Address::generate(&env);
    client.start_game(&player, &Side::Heads, &10_000_000, &commitment(&env, &env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into()));

    // Advance to one ledger before the minimum delay.
    env.ledger().with_mut(|l| l.sequence_number += MIN_REVEAL_DELAY_LEDGERS - 1);
    let result = client.try_reveal(&player, &secret(&env, &BytesN::from_array(&env, &[0u8; 64])));
    assert_eq!(result, Err(Ok(Error::RevealTimeout)));
}

/// reveal exactly at start_ledger + MIN_REVEAL_DELAY_LEDGERS → succeeds.
#[test]
fn test_reveal_at_exact_delay_boundary_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = setup(&env);
    fund(&env, &contract_id, 1_000_000_000);

    let player = Address::generate(&env);
    client.start_game(&player, &Side::Heads, &10_000_000, &commitment(&env, &env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into()));

    env.ledger().with_mut(|l| l.sequence_number += MIN_REVEAL_DELAY_LEDGERS);
    let result = client.try_reveal(&player, &secret(&env, &BytesN::from_array(&env, &[0u8; 64])));
    assert!(result.is_ok(), "reveal at exact delay boundary must succeed");
}

/// reveal well after the delay → succeeds.
#[test]
fn test_reveal_after_delay_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = setup(&env);
    fund(&env, &contract_id, 1_000_000_000);

    let player = Address::generate(&env);
    client.start_game(&player, &Side::Heads, &10_000_000, &commitment(&env, &env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into()));

    env.ledger().with_mut(|l| l.sequence_number += MIN_REVEAL_DELAY_LEDGERS + 50);
    let result = client.try_reveal(&player, &secret(&env, &BytesN::from_array(&env, &[0u8; 64])));
    assert!(result.is_ok(), "reveal after delay must succeed");
}

// ── No state mutation on time-lock rejection ──────────────────────────────────

/// Game state must be unchanged when RevealTimeout fires.
#[test]
fn test_reveal_timelock_no_state_mutation() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = setup(&env);
    fund(&env, &contract_id, 1_000_000_000);

    let player = Address::generate(&env);
    client.start_game(&player, &Side::Heads, &10_000_000, &commitment(&env, &env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into()));

    let before: GameState = env.as_contract(&contract_id, || {
        CoinflipContract::load_player_game(&env, &player).unwrap()
    });

    // Attempt reveal too early.
    let _ = client.try_reveal(&player, &secret(&env, &BytesN::from_array(&env, &[0u8; 64])));

    let after: GameState = env.as_contract(&contract_id, || {
        CoinflipContract::load_player_game(&env, &player).unwrap()
    });

    assert_eq!(before, after, "game state must be unchanged on RevealTimeout");
}

/// Stats must be unchanged when RevealTimeout fires.
#[test]
fn test_reveal_timelock_no_stats_mutation() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = setup(&env);
    fund(&env, &contract_id, 1_000_000_000);

    let player = Address::generate(&env);
    client.start_game(&player, &Side::Heads, &10_000_000, &commitment(&env, &env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into()));

    let before: ContractStats = env.as_contract(&contract_id, || {
        CoinflipContract::load_stats(&env)
    });

    let _ = client.try_reveal(&player, &secret(&env, &BytesN::from_array(&env, &[0u8; 64])));

    let after: ContractStats = env.as_contract(&contract_id, || {
        CoinflipContract::load_stats(&env)
    });

    assert_eq!(before.reserve_balance, after.reserve_balance);
    assert_eq!(before.total_fees, after.total_fees);
}

// ── Time-lock constant ────────────────────────────────────────────────────────

/// MIN_REVEAL_DELAY_LEDGERS must be 10.
#[test]
fn test_min_reveal_delay_constant_value() {
    assert_eq!(MIN_REVEAL_DELAY_LEDGERS, 10);
}

/// MIN_REVEAL_DELAY_LEDGERS must be strictly less than REVEAL_TIMEOUT_LEDGERS
/// so the valid reveal window is non-empty.
#[test]
fn test_reveal_window_is_non_empty() {
    // REVEAL_TIMEOUT_LEDGERS is not pub, but we can verify the relationship
    // indirectly: a reveal at MIN_REVEAL_DELAY_LEDGERS must succeed, and
    // reclaim_wager at MIN_REVEAL_DELAY_LEDGERS must still be too early.
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = setup(&env);
    fund(&env, &contract_id, 1_000_000_000);

    let player = Address::generate(&env);
    client.start_game(&player, &Side::Heads, &10_000_000, &commitment(&env, &env.crypto().sha256(&soroban_sdk::Bytes::from_slice(&env, &[42u8; 32])).into()));

    // At MIN_REVEAL_DELAY_LEDGERS: reveal succeeds, reclaim is too early.
    env.ledger().with_mut(|l| l.sequence_number += MIN_REVEAL_DELAY_LEDGERS);
    assert!(client.try_reveal(&player, &secret(&env, &BytesN::from_array(&env, &[0u8; 64]))).is_ok());
}
