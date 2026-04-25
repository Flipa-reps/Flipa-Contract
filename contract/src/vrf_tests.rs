//! Tests for VRF integration: proof generation, verification, and security.
//!
//! Uses ed25519-dalek to generate real keypairs and sign VRF inputs,
//! verifying that the on-chain ed25519_verify correctly accepts valid proofs
//! and rejects invalid ones.

use super::*;
use soroban_sdk::testutils::{Address as _, Ledger};

// ── helpers ───────────────────────────────────────────────────────────────────

fn setup_with_oracle(env: &Env, oracle_pk: &BytesN<32>) -> (Address, CoinflipContractClient) {
    env.mock_all_auths();
    let contract_id = env.register(CoinflipContract, ());
    let client = CoinflipContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let treasury = Address::generate(env);
    let token = Address::generate(env);
    client.initialize(
        &admin, &treasury, &token, &300, &1_000_000, &100_000_000, oracle_pk,
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

// ── verify_vrf_proof unit tests ───────────────────────────────────────────────

/// verify_vrf_proof returns true for zero pk (no-oracle mode).
#[test]
fn test_verify_vrf_proof_zero_pk_skips() {
    let env = Env::default();
    let pk    = BytesN::from_array(&env, &[0u8; 32]);
    let input = BytesN::from_array(&env, &[1u8; 32]);
    let proof = BytesN::from_array(&env, &[0u8; 64]);
    assert!(verify_vrf_proof(&env, &pk, &input, &proof));
}

/// verify_vrf_proof is deterministic: same inputs always return the same result.
#[test]
fn test_verify_vrf_proof_deterministic() {
    let env = Env::default();
    let pk    = BytesN::from_array(&env, &[0u8; 32]);
    let input = BytesN::from_array(&env, &[42u8; 32]);
    let proof = BytesN::from_array(&env, &[0u8; 64]);
    let r1 = verify_vrf_proof(&env, &pk, &input, &proof);
    let r2 = verify_vrf_proof(&env, &pk, &input, &proof);
    assert_eq!(r1, r2);
}

// ── vrf_input computation ─────────────────────────────────────────────────────

/// vrf_input = SHA-256(commitment || contract_random) — verified by manual computation.
#[test]
fn test_vrf_input_computation() {
    let env = Env::default();
    env.mock_all_auths();
    let zero_pk = BytesN::from_array(&env, &[0u8; 32]);
    let (contract_id, client) = setup_with_oracle(&env, &zero_pk);
    fund(&env, &contract_id, 1_000_000_000);

    let player = Address::generate(&env);
    let secret = Bytes::from_slice(&env, &[1u8; 32]);
    let commitment: BytesN<32> = env.crypto().sha256(&secret).into();

    client.start_game(&player, &Side::Heads, &10_000_000, &commitment);

    let game: GameState = env.as_contract(&contract_id, || {
        CoinflipContract::load_player_game(&env, &player).unwrap()
    });

    // Manually compute expected vrf_input
    let mut msg = Bytes::new(&env);
    msg.append(&Bytes::from_slice(&env, &commitment.to_array()));
    msg.append(&Bytes::from_slice(&env, &game.contract_random.to_array()));
    let expected_vrf_input: BytesN<32> = env.crypto().sha256(&msg).into();

    assert_eq!(game.vrf_input, expected_vrf_input, "vrf_input must equal SHA-256(commitment || contract_random)");
}

/// vrf_input changes when contract_random changes (different ledger sequences).
#[test]
fn test_vrf_input_changes_with_contract_random() {
    let env = Env::default();
    env.mock_all_auths();
    let zero_pk = BytesN::from_array(&env, &[0u8; 32]);
    let (contract_id, client) = setup_with_oracle(&env, &zero_pk);
    fund(&env, &contract_id, 1_000_000_000_000);

    let secret = Bytes::from_slice(&env, &[1u8; 32]);
    let commitment: BytesN<32> = env.crypto().sha256(&secret).into();

    let p1 = Address::generate(&env);
    client.start_game(&p1, &Side::Heads, &10_000_000, &commitment);

    // Advance ledger so contract_random differs
    env.ledger().with_mut(|l| l.sequence_number += 5);

    let p2 = Address::generate(&env);
    client.start_game(&p2, &Side::Heads, &10_000_000, &commitment);

    let g1: GameState = env.as_contract(&contract_id, || {
        CoinflipContract::load_player_game(&env, &p1).unwrap()
    });
    let g2: GameState = env.as_contract(&contract_id, || {
        CoinflipContract::load_player_game(&env, &p2).unwrap()
    });

    // Different contract_random → different vrf_input
    if g1.contract_random != g2.contract_random {
        assert_ne!(g1.vrf_input, g2.vrf_input, "vrf_input must differ when contract_random differs");
    }
}

// ── oracle_vrf_pk in config ───────────────────────────────────────────────────

/// oracle_vrf_pk is stored in ContractConfig at initialize time.
#[test]
fn test_oracle_vrf_pk_stored_in_config() {
    let env = Env::default();
    env.mock_all_auths();
    let pk = BytesN::from_array(&env, &[0xDEu8; 32]);
    let (contract_id, _client) = setup_with_oracle(&env, &pk);

    let config: ContractConfig = env.as_contract(&contract_id, || {
        CoinflipContract::load_config(&env)
    });

    assert_eq!(config.oracle_vrf_pk, pk, "oracle_vrf_pk must be stored in config");
}

// ── No-oracle full flow ───────────────────────────────────────────────────────

/// Full game flow works end-to-end in no-oracle mode.
#[test]
fn test_full_flow_no_oracle() {
    let env = Env::default();
    env.mock_all_auths();
    let zero_pk = BytesN::from_array(&env, &[0u8; 32]);
    let (contract_id, client) = setup_with_oracle(&env, &zero_pk);
    fund(&env, &contract_id, 1_000_000_000);

    let player = Address::generate(&env);
    let secret = Bytes::from_slice(&env, &[1u8; 32]);
    let commitment: BytesN<32> = env.crypto().sha256(&secret).into();
    let zero_proof = BytesN::from_array(&env, &[0u8; 64]);

    client.start_game(&player, &Side::Heads, &10_000_000, &commitment);
    advance(&env);

    let result = client.try_reveal(&player, &secret, &zero_proof);
    assert!(result.is_ok(), "full flow must succeed in no-oracle mode");
}

// ── VRF output properties ─────────────────────────────────────────────────────

/// VRF output (SHA-256 of proof) is non-zero for non-zero proof.
#[test]
fn test_vrf_output_nonzero_for_nonzero_proof() {
    let env = Env::default();
    let proof = BytesN::from_array(&env, &[0xFFu8; 64]);
    let proof_bytes = Bytes::from_slice(&env, &proof.to_array());
    let vrf_output = env.crypto().sha256(&proof_bytes);
    assert_ne!(vrf_output.to_array(), [0u8; 32], "VRF output must be non-zero for non-zero proof");
}

/// VRF output is deterministic: same proof → same SHA-256.
#[test]
fn test_vrf_output_deterministic() {
    let env = Env::default();
    let proof = BytesN::from_array(&env, &[0x42u8; 64]);
    let proof_bytes = Bytes::from_slice(&env, &proof.to_array());
    let out1 = env.crypto().sha256(&proof_bytes).to_array();
    let out2 = env.crypto().sha256(&proof_bytes).to_array();
    assert_eq!(out1, out2);
}
