//! Tests for multi-party randomness contribution.
//!
//! Three independent parties contribute to outcome generation:
//! 1. Player  — revealed secret (pre-image of `commitment`)
//! 2. Contract — `SHA-256(ledger_sequence)` mixed with entropy pool
//! 3. Oracle  — revealed random bytes (pre-image of `oracle_commitment`)
//!
//! Aggregation: `SHA-256(player_secret || SHA-256(contract_random XOR SHA-256(oracle_random)))`
//!
//! Security invariants:
//! - Wrong oracle_random → CommitmentMismatch
//! - Different oracle values → different outcomes (oracle influences result)
//! - No single party can predict the outcome without the others' committed values

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
    client.initialize(&admin, &treasury, &token, &300, &1_000_000, &100_000_000);
    (contract_id, client)
}

fn fund(env: &Env, contract_id: &Address, amount: i128) {
    env.as_contract(contract_id, || {
        let mut stats = CoinflipContract::load_stats(env);
        stats.reserve_balance = amount;
        CoinflipContract::save_stats(env, &stats);
    });
}

fn player_secret(env: &Env, seed: u8) -> Bytes {
    Bytes::from_slice(env, &[seed; 32])
}

fn player_commitment(env: &Env, seed: u8) -> BytesN<32> {
    env.crypto().sha256(&player_secret(env, seed)).into()
}

fn oracle_random(env: &Env, seed: u8) -> Bytes {
    Bytes::from_slice(env, &[seed; 32])
}

fn oracle_commitment(env: &Env, seed: u8) -> BytesN<32> {
    env.crypto().sha256(&oracle_random(env, seed)).into()
}

fn advance(env: &Env) {
    env.ledger().with_mut(|l| l.sequence_number += MIN_REVEAL_DELAY_LEDGERS);
}

// ── Contribution verification ─────────────────────────────────────────────────

/// Wrong oracle_random (doesn't match oracle_commitment) → CommitmentMismatch.
#[test]
fn test_wrong_oracle_random_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = setup(&env);
    fund(&env, &contract_id, 1_000_000_000);

    let player = Address::generate(&env);
    client.start_game(
        &player, &Side::Heads, &10_000_000,
        &player_commitment(&env, 1), &oracle_commitment(&env, 10),
    );
    advance(&env);

    // Reveal with wrong oracle_random (seed 99 ≠ seed 10).
    let result = client.try_reveal(
        &player, &player_secret(&env, 1), &oracle_random(&env, 99),
    );
    assert_eq!(result, Err(Ok(Error::CommitmentMismatch)));
}

/// Correct oracle_random → reveal succeeds.
#[test]
fn test_correct_oracle_random_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = setup(&env);
    fund(&env, &contract_id, 1_000_000_000);

    let player = Address::generate(&env);
    client.start_game(
        &player, &Side::Heads, &10_000_000,
        &player_commitment(&env, 1), &oracle_commitment(&env, 10),
    );
    advance(&env);

    let result = client.try_reveal(
        &player, &player_secret(&env, 1), &oracle_random(&env, 10),
    );
    assert!(result.is_ok(), "correct oracle_random must be accepted");
}

/// No state mutation when oracle verification fails.
#[test]
fn test_wrong_oracle_no_state_mutation() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = setup(&env);
    fund(&env, &contract_id, 1_000_000_000);

    let player = Address::generate(&env);
    client.start_game(
        &player, &Side::Heads, &10_000_000,
        &player_commitment(&env, 1), &oracle_commitment(&env, 10),
    );
    advance(&env);

    let before: GameState = env.as_contract(&contract_id, || {
        CoinflipContract::load_player_game(&env, &player).unwrap()
    });

    let _ = client.try_reveal(&player, &player_secret(&env, 1), &oracle_random(&env, 99));

    let after: GameState = env.as_contract(&contract_id, || {
        CoinflipContract::load_player_game(&env, &player).unwrap()
    });

    assert_eq!(before, after, "game state must be unchanged on oracle mismatch");
}

// ── Oracle influences outcome ─────────────────────────────────────────────────

/// Different oracle values produce different outcomes for the same player secret.
/// (Tests that oracle contribution is actually used in outcome derivation.)
#[test]
fn test_oracle_contribution_influences_outcome() {
    let env = Env::default();

    // Use generate_outcome directly to verify oracle changes the result.
    let secret = player_secret(&env, 1);
    let cr = BytesN::from_array(&env, &[0xABu8; 32]);

    let oracle_a = oracle_random(&env, 1);
    let oracle_b = oracle_random(&env, 2);

    let outcome_a = generate_outcome(&env, &secret, &cr, &oracle_a);
    let outcome_b = generate_outcome(&env, &secret, &cr, &oracle_b);

    // With different oracle values the outcomes must differ (for these specific inputs).
    // We verify the aggregation is non-trivial: oracle_a and oracle_b produce different
    // SHA-256 hashes, so the XOR with cr differs, so the final hash differs.
    assert_ne!(
        outcome_a, outcome_b,
        "different oracle values must produce different outcomes"
    );
}

/// generate_outcome is deterministic: same three inputs always yield the same side.
#[test]
fn test_generate_outcome_deterministic_with_oracle() {
    let env = Env::default();
    let secret = player_secret(&env, 5);
    let cr = BytesN::from_array(&env, &[0x11u8; 32]);
    let oracle = oracle_random(&env, 7);

    let r1 = generate_outcome(&env, &secret, &cr, &oracle);
    let r2 = generate_outcome(&env, &secret, &cr, &oracle);
    assert_eq!(r1, r2, "generate_outcome must be deterministic");
}

// ── oracle_commitment stored in GameState ─────────────────────────────────────

/// oracle_commitment is persisted in GameState at start_game time.
#[test]
fn test_oracle_commitment_stored_in_game_state() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = setup(&env);
    fund(&env, &contract_id, 1_000_000_000);

    let player = Address::generate(&env);
    let oc = oracle_commitment(&env, 42);
    client.start_game(
        &player, &Side::Heads, &10_000_000,
        &player_commitment(&env, 1), &oc,
    );

    let game: GameState = env.as_contract(&contract_id, || {
        CoinflipContract::load_player_game(&env, &player).unwrap()
    });

    assert_eq!(game.oracle_commitment, oc, "oracle_commitment must be stored in GameState");
}

// ── Aggregation correctness ───────────────────────────────────────────────────

/// Verify the XOR aggregation: outcome with oracle=[0;32] equals outcome with
/// contract_random XOR SHA-256([0;32]) as the effective randomness.
#[test]
fn test_aggregation_xor_correctness() {
    let env = Env::default();
    let secret = player_secret(&env, 1);
    let cr = BytesN::from_array(&env, &[0xFFu8; 32]);

    // oracle_random = [0;32] → SHA-256([0;32]) XOR cr
    let zero_oracle = Bytes::from_slice(&env, &[0u8; 32]);
    let oracle_hash = env.crypto().sha256(&zero_oracle).to_array();
    let cr_arr = cr.to_array();
    let mut expected_agg = [0u8; 32];
    for i in 0..32 {
        expected_agg[i] = cr_arr[i] ^ oracle_hash[i];
    }

    // generate_outcome with zero oracle should use expected_agg as the effective randomness
    let outcome = generate_outcome(&env, &secret, &cr, &zero_oracle);

    // Manually compute expected outcome
    let agg_bytes = Bytes::from_slice(&env, &expected_agg);
    let mut combined = Bytes::new(&env);
    combined.append(&secret);
    combined.append(&agg_bytes);
    let hash = env.crypto().sha256(&combined);
    let expected = if hash.to_array()[0] % 2 == 0 { Side::Heads } else { Side::Tails };

    assert_eq!(outcome, expected, "XOR aggregation must match manual computation");
}
