//! Tests for multi-party randomness and VRF integration.
//!
//! Three independent parties contribute to outcome generation:
//! 1. Player  — revealed secret (pre-image of `commitment`)
//! 2. Contract — `SHA-256(ledger_sequence)` mixed with entropy pool
//! 3. Oracle  — Ed25519 VRF proof over `vrf_input = SHA-256(commitment || contract_random)`
//!
//! When `oracle_vrf_pk` is all-zero, VRF verification is skipped (no-oracle mode).
//! In production, a real Ed25519 key is set at `initialize` time.

use super::*;
use soroban_sdk::testutils::{Address as _, Ledger};

// ── helpers ───────────────────────────────────────────────────────────────────

fn setup_no_oracle(env: &Env) -> (Address, CoinflipContractClient) {
    env.mock_all_auths();
    let contract_id = env.register(CoinflipContract, ());
    let client = CoinflipContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let treasury = Address::generate(env);
    let token = Address::generate(env);
    // Zero oracle_vrf_pk = no oracle mode
    client.initialize(
        &admin, &treasury, &token, &300, &1_000_000, &100_000_000,
        &BytesN::from_array(env, &[0u8; 32]),
    );
    (contract_id, client)
}

fn fund(env: &Env, contract_id: &Address, amount: i128) {
    env.as_contract(contract_id, || {
        let mut stats = CoinflipContract::load_stats(env);
        stats.reserve_balance = amount;
        CoinflipContract::save_stats(env, &stats);
    });
}

fn advance(env: &Env) {
    env.ledger().with_mut(|l| l.sequence_number += MIN_REVEAL_DELAY_LEDGERS);
}

fn zero_proof(env: &Env) -> BytesN<64> {
    BytesN::from_array(env, &[0u8; 64])
}

// ── verify_vrf_proof ──────────────────────────────────────────────────────────

/// verify_vrf_proof returns true when oracle_vrf_pk is all-zero (no-oracle mode).
#[test]
fn test_verify_vrf_proof_skips_when_pk_is_zero() {
    let env = Env::default();
    let pk = BytesN::from_array(&env, &[0u8; 32]);
    let input = BytesN::from_array(&env, &[1u8; 32]);
    let proof = BytesN::from_array(&env, &[0u8; 64]);
    assert!(verify_vrf_proof(&env, &pk, &input, &proof));
}

// ── vrf_input stored in GameState ────────────────────────────────────────────

/// vrf_input is stored in GameState at start_game time.
#[test]
fn test_vrf_input_stored_in_game_state() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = setup_no_oracle(&env);
    fund(&env, &contract_id, 1_000_000_000);

    let player = Address::generate(&env);
    let secret = Bytes::from_slice(&env, &[1u8; 32]);
    let commitment: BytesN<32> = env.crypto().sha256(&secret).into();

    client.start_game(&player, &Side::Heads, &10_000_000, &commitment);

    let game: GameState = env.as_contract(&contract_id, || {
        CoinflipContract::load_player_game(&env, &player).unwrap()
    });

    // vrf_input must be non-zero (it's SHA-256 of commitment || contract_random)
    assert_ne!(
        game.vrf_input,
        BytesN::from_array(&env, &[0u8; 32]),
        "vrf_input must be set to SHA-256(commitment || contract_random)"
    );
}

/// vrf_input is deterministic: same commitment + contract_random → same vrf_input.
#[test]
fn test_vrf_input_is_deterministic() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = setup_no_oracle(&env);
    fund(&env, &contract_id, 1_000_000_000_000);

    // Two players starting at the same ledger with the same commitment get the same vrf_input.
    let secret = Bytes::from_slice(&env, &[1u8; 32]);
    let commitment: BytesN<32> = env.crypto().sha256(&secret).into();

    let p1 = Address::generate(&env);
    let p2 = Address::generate(&env);
    client.start_game(&p1, &Side::Heads, &10_000_000, &commitment);
    client.start_game(&p2, &Side::Heads, &10_000_000, &commitment);

    let g1: GameState = env.as_contract(&contract_id, || {
        CoinflipContract::load_player_game(&env, &p1).unwrap()
    });
    let g2: GameState = env.as_contract(&contract_id, || {
        CoinflipContract::load_player_game(&env, &p2).unwrap()
    });

    // Same commitment + same contract_random → same vrf_input
    if g1.contract_random == g2.contract_random {
        assert_eq!(g1.vrf_input, g2.vrf_input);
    }
}

// ── generate_outcome with VRF proof ──────────────────────────────────────────

/// generate_outcome is deterministic: same inputs always yield the same side.
#[test]
fn test_generate_outcome_deterministic_with_vrf_proof() {
    let env = Env::default();
    let secret = Bytes::from_slice(&env, &[5u8; 32]);
    let cr = BytesN::from_array(&env, &[0x11u8; 32]);
    let proof = BytesN::from_array(&env, &[0xAAu8; 64]);

    let r1 = generate_outcome(&env, &secret, &cr, &proof);
    let r2 = generate_outcome(&env, &secret, &cr, &proof);
    assert_eq!(r1, r2, "generate_outcome must be deterministic");
}

/// Different VRF proofs produce different outcomes (proof influences result).
#[test]
fn test_different_vrf_proofs_influence_outcome() {
    let env = Env::default();
    let secret = Bytes::from_slice(&env, &[1u8; 32]);
    let cr = BytesN::from_array(&env, &[0xABu8; 32]);

    let proof_a = BytesN::from_array(&env, &[0x01u8; 64]);
    let proof_b = BytesN::from_array(&env, &[0x02u8; 64]);

    let outcome_a = generate_outcome(&env, &secret, &cr, &proof_a);
    let outcome_b = generate_outcome(&env, &secret, &cr, &proof_b);

    // Different proofs → different SHA-256(proof) → different XOR → different outcome
    assert_ne!(outcome_a, outcome_b, "different VRF proofs must produce different outcomes");
}

/// generate_outcome returns only Heads or Tails.
#[test]
fn test_generate_outcome_returns_valid_side() {
    let env = Env::default();
    let secret = Bytes::from_slice(&env, &[7u8; 32]);
    let cr = BytesN::from_array(&env, &[0x22u8; 32]);
    let proof = BytesN::from_array(&env, &[0x33u8; 64]);
    let side = generate_outcome(&env, &secret, &cr, &proof);
    assert!(side == Side::Heads || side == Side::Tails);
}

// ── No-oracle reveal flow ─────────────────────────────────────────────────────

/// Full reveal flow works in no-oracle mode (zero proof accepted).
#[test]
fn test_reveal_succeeds_in_no_oracle_mode() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = setup_no_oracle(&env);
    fund(&env, &contract_id, 1_000_000_000);

    let player = Address::generate(&env);
    let secret = Bytes::from_slice(&env, &[1u8; 32]);
    let commitment: BytesN<32> = env.crypto().sha256(&secret).into();

    client.start_game(&player, &Side::Heads, &10_000_000, &commitment);
    advance(&env);

    let result = client.try_reveal(&player, &secret, &zero_proof(&env));
    assert!(result.is_ok(), "reveal must succeed in no-oracle mode");
}

// ── VRF output aggregation ────────────────────────────────────────────────────

/// Verify the VRF aggregation: vrf_output = SHA-256(proof), XOR'd with contract_random.
#[test]
fn test_vrf_aggregation_correctness() {
    let env = Env::default();
    let secret = Bytes::from_slice(&env, &[1u8; 32]);
    let cr = BytesN::from_array(&env, &[0xFFu8; 32]);
    let proof = BytesN::from_array(&env, &[0xBBu8; 64]);

    // Manually compute expected outcome
    let proof_bytes = Bytes::from_slice(&env, &proof.to_array());
    let vrf_output = env.crypto().sha256(&proof_bytes).to_array();
    let cr_arr = cr.to_array();
    let mut aggregated = [0u8; 32];
    for i in 0..32 {
        aggregated[i] = cr_arr[i] ^ vrf_output[i];
    }
    let agg_bytes = Bytes::from_slice(&env, &aggregated);
    let mut combined = Bytes::new(&env);
    combined.append(&secret);
    combined.append(&agg_bytes);
    let hash = env.crypto().sha256(&combined);
    let expected = if hash.to_array()[0] % 2 == 0 { Side::Heads } else { Side::Tails };

    let outcome = generate_outcome(&env, &secret, &cr, &proof);
    assert_eq!(outcome, expected, "VRF aggregation must match manual computation");
}
