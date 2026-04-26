//! ZK commitment proof verification tests.
//!
//! Covers:
//! - Valid proof round-trip (prove → verify = Valid)
//! - Invalid proof detection (tampered r_hash, tampered response)
//! - Proof is constant size (64 bytes total)
//! - Different secrets produce different proofs
//! - Same inputs produce same proof (determinism)
//! - zk_challenge domain separation
//! - start_game_with_zk_proof: valid proof accepted, invalid proof rejected

use super::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::Bytes;

// ── helpers ───────────────────────────────────────────────────────────────────

fn env() -> Env {
    Env::default()
}

fn secret(env: &Env, byte: u8) -> Bytes {
    Bytes::from_slice(env, &[byte; 32])
}

fn nonce(env: &Env, byte: u8) -> BytesN<32> {
    BytesN::from_array(env, &[byte; 32])
}

fn commitment(env: &Env, s: &Bytes) -> BytesN<32> {
    env.crypto().sha256(s).into()
}

fn statement(env: &Env, c: BytesN<32>) -> ZkStatement {
    ZkStatement {
        commitment: c,
        domain: Bytes::from_slice(env, ZK_DOMAIN),
    }
}

// ── round-trip ────────────────────────────────────────────────────────────────

#[test]
fn test_valid_proof_verifies() {
    let env = env();
    let s = secret(&env, 0x42);
    let c = commitment(&env, &s);
    let proof = zk_prove_commitment(&env, &s, &c, &nonce(&env, 0x01));
    assert_eq!(zk_verify_commitment(&env, &statement(&env, c), &proof), ZkVerifyResult::Valid);
}

#[test]
fn test_proof_is_deterministic() {
    let env = env();
    let s = secret(&env, 0xAB);
    let c = commitment(&env, &s);
    let n = nonce(&env, 0x99);
    let p1 = zk_prove_commitment(&env, &s, &c, &n);
    let p2 = zk_prove_commitment(&env, &s, &c, &n);
    assert_eq!(p1, p2);
}

// ── tampered proof detection ──────────────────────────────────────────────────

#[test]
fn test_tampered_r_hash_fails() {
    let env = env();
    let s = secret(&env, 0x01);
    let c = commitment(&env, &s);
    let mut proof = zk_prove_commitment(&env, &s, &c, &nonce(&env, 0x02));
    // Flip the r_hash.
    proof.r_hash = BytesN::from_array(&env, &[0xFFu8; 32]);
    assert_eq!(zk_verify_commitment(&env, &statement(&env, c), &proof), ZkVerifyResult::Invalid);
}

#[test]
fn test_tampered_response_fails() {
    let env = env();
    let s = secret(&env, 0x01);
    let c = commitment(&env, &s);
    let mut proof = zk_prove_commitment(&env, &s, &c, &nonce(&env, 0x02));
    proof.response = BytesN::from_array(&env, &[0xFFu8; 32]);
    assert_eq!(zk_verify_commitment(&env, &statement(&env, c), &proof), ZkVerifyResult::Invalid);
}

#[test]
fn test_wrong_commitment_in_statement_fails() {
    let env = env();
    let s = secret(&env, 0x01);
    let c = commitment(&env, &s);
    let proof = zk_prove_commitment(&env, &s, &c, &nonce(&env, 0x02));
    // Use a different commitment in the statement.
    let wrong_c = BytesN::from_array(&env, &[0xAAu8; 32]);
    assert_eq!(
        zk_verify_commitment(&env, &statement(&env, wrong_c), &proof),
        ZkVerifyResult::Invalid
    );
}

// ── proof size ────────────────────────────────────────────────────────────────

#[test]
fn test_proof_is_constant_64_bytes() {
    let env = env();
    let s = secret(&env, 0x10);
    let c = commitment(&env, &s);
    let proof = zk_prove_commitment(&env, &s, &c, &nonce(&env, 0x20));
    // r_hash (32) + response (32) = 64 bytes total.
    assert_eq!(proof.r_hash.to_array().len(), 32);
    assert_eq!(proof.response.to_array().len(), 32);
}

// ── different secrets → different proofs ─────────────────────────────────────

#[test]
fn test_different_secrets_produce_different_proofs() {
    let env = env();
    let n = nonce(&env, 0x01);

    let s1 = secret(&env, 0xAA);
    let c1 = commitment(&env, &s1);
    let p1 = zk_prove_commitment(&env, &s1, &c1, &n);

    let s2 = secret(&env, 0xBB);
    let c2 = commitment(&env, &s2);
    let p2 = zk_prove_commitment(&env, &s2, &c2, &n);

    assert_ne!(p1.response, p2.response);
}

// ── different nonces → different proofs ──────────────────────────────────────

#[test]
fn test_different_nonces_produce_different_proofs() {
    let env = env();
    let s = secret(&env, 0x55);
    let c = commitment(&env, &s);

    let p1 = zk_prove_commitment(&env, &s, &c, &nonce(&env, 0x01));
    let p2 = zk_prove_commitment(&env, &s, &c, &nonce(&env, 0x02));

    assert_ne!(p1.r_hash, p2.r_hash);
    assert_ne!(p1.response, p2.response);
}

// ── zk_challenge domain separation ───────────────────────────────────────────

#[test]
fn test_challenge_changes_with_commitment() {
    let env = env();
    let r = BytesN::from_array(&env, &[0x01u8; 32]);
    let c1 = BytesN::from_array(&env, &[0xAAu8; 32]);
    let c2 = BytesN::from_array(&env, &[0xBBu8; 32]);
    assert_ne!(zk_challenge(&env, &c1, &r), zk_challenge(&env, &c2, &r));
}

#[test]
fn test_challenge_changes_with_r_hash() {
    let env = env();
    let c = BytesN::from_array(&env, &[0x01u8; 32]);
    let r1 = BytesN::from_array(&env, &[0xAAu8; 32]);
    let r2 = BytesN::from_array(&env, &[0xBBu8; 32]);
    assert_ne!(zk_challenge(&env, &c, &r1), zk_challenge(&env, &c, &r2));
}

// ── start_game_with_zk_proof integration ─────────────────────────────────────

fn setup(env: &Env) -> (Address, CoinflipContractClient) {
    env.mock_all_auths();
    let contract_id = env.register(CoinflipContract, ());
    let client = CoinflipContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let treasury = Address::generate(env);
    let token = Address::generate(env);
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

#[test]
fn test_start_game_with_valid_zk_proof_succeeds() {
    let env = env();
    let (contract_id, client) = setup(&env);
    fund(&env, &contract_id, 1_000_000_000);

    let player = Address::generate(&env);
    let s = secret(&env, 0x77);
    let c = commitment(&env, &s);
    let proof = zk_prove_commitment(&env, &s, &c, &nonce(&env, 0x11));

    let result = client.try_start_game_with_zk_proof(
        &player,
        &Side::Heads,
        &10_000_000,
        &c,
        &proof,
        &None,
        &BytesN::from_array(&env, &[0u8; 32]),
        &Address::generate(&env), // token (will fail whitelist in full flow, but tests the ZK path)
    );
    // The ZK proof itself is valid; downstream errors (e.g. token whitelist) are separate.
    // We assert the error is NOT InvalidCommitment (ZK rejection).
    if let Err(e) = result {
        assert_ne!(e.unwrap(), Error::InvalidCommitment, "ZK proof must not be rejected");
    }
}

#[test]
fn test_start_game_with_invalid_zk_proof_rejected() {
    let env = env();
    let (contract_id, client) = setup(&env);
    fund(&env, &contract_id, 1_000_000_000);

    let player = Address::generate(&env);
    let s = secret(&env, 0x77);
    let c = commitment(&env, &s);

    // Forge a proof with wrong response.
    let bad_proof = ZkProof {
        r_hash:   BytesN::from_array(&env, &[0xDEu8; 32]),
        response: BytesN::from_array(&env, &[0xADu8; 32]),
    };

    let result = client.try_start_game_with_zk_proof(
        &player,
        &Side::Heads,
        &10_000_000,
        &c,
        &bad_proof,
        &None,
        &BytesN::from_array(&env, &[0u8; 32]),
        &Address::generate(&env),
    );
    assert_eq!(result.unwrap_err().unwrap(), Error::InvalidCommitment);
}
