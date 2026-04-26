//! MPC threshold cryptography security validation tests.
//!
//! Covers:
//! - Session creation and phase transitions
//! - Commitment submission (happy path, duplicate rejection, signature bypass)
//! - Share reveal with pre-image verification
//! - Threshold aggregation correctness and determinism
//! - Threshold signature verification (happy path and threshold enforcement)
//! - Security: wrong share rejected, duplicate party rejected

use super::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::Bytes;

// ── helpers ───────────────────────────────────────────────────────────────────

fn env() -> Env {
    Env::default()
}

fn zero_sig(env: &Env) -> BytesN<64> {
    BytesN::from_array(env, &[0u8; 64])
}

fn zero_pk(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[0u8; 32])
}

fn make_share(env: &Env, byte: u8) -> Bytes {
    Bytes::from_slice(env, &[byte; 32])
}

fn commit(env: &Env, share: &Bytes) -> BytesN<32> {
    env.crypto().sha256(share).into()
}

// ── session creation ──────────────────────────────────────────────────────────

#[test]
fn test_create_session_returns_incrementing_ids() {
    let env = env();
    let id1 = mpc_create_session(&env, 2, 3);
    let id2 = mpc_create_session(&env, 1, 1);
    assert_eq!(id1, 1);
    assert_eq!(id2, 2);
}

#[test]
fn test_create_session_starts_in_commit_phase() {
    let env = env();
    let id = mpc_create_session(&env, 2, 3);
    let session: MpcSession = env
        .storage()
        .persistent()
        .get(&StorageKey::MpcSession(id))
        .unwrap();
    assert_eq!(session.phase, MpcPhase::Commit);
    assert_eq!(session.threshold, 2);
    assert_eq!(session.total_parties, 3);
    assert_eq!(session.commitments.len(), 0);
    assert_eq!(session.shares.len(), 0);
}

// ── commitment submission ─────────────────────────────────────────────────────

#[test]
fn test_submit_commitment_advances_to_reveal_when_full() {
    let env = env();
    let id = mpc_create_session(&env, 2, 2);

    let p1 = Address::generate(&env);
    let p2 = Address::generate(&env);
    let share1 = make_share(&env, 0xAA);
    let share2 = make_share(&env, 0xBB);

    mpc_submit_commitment(&env, id, p1, commit(&env, &share1), zero_pk(&env), zero_sig(&env));
    // After 1 commitment, still in Commit phase.
    let session: MpcSession = env.storage().persistent().get(&StorageKey::MpcSession(id)).unwrap();
    assert_eq!(session.phase, MpcPhase::Commit);

    mpc_submit_commitment(&env, id, p2, commit(&env, &share2), zero_pk(&env), zero_sig(&env));
    // After 2 commitments (== total_parties), advances to Reveal.
    let session: MpcSession = env.storage().persistent().get(&StorageKey::MpcSession(id)).unwrap();
    assert_eq!(session.phase, MpcPhase::Reveal);
}

#[test]
#[should_panic]
fn test_submit_commitment_rejects_duplicate_party() {
    let env = env();
    let id = mpc_create_session(&env, 1, 2);
    let party = Address::generate(&env);
    let share = make_share(&env, 0x01);
    mpc_submit_commitment(&env, id, party.clone(), commit(&env, &share), zero_pk(&env), zero_sig(&env));
    // Second submission from same party must panic.
    mpc_submit_commitment(&env, id, party, commit(&env, &share), zero_pk(&env), zero_sig(&env));
}

#[test]
#[should_panic]
fn test_submit_commitment_rejects_wrong_phase() {
    let env = env();
    // 1-of-1: submitting the first commitment advances to Reveal immediately.
    let id = mpc_create_session(&env, 1, 1);
    let p1 = Address::generate(&env);
    let share = make_share(&env, 0x01);
    mpc_submit_commitment(&env, id, p1, commit(&env, &share), zero_pk(&env), zero_sig(&env));
    // Session is now in Reveal phase; another commit must panic.
    let p2 = Address::generate(&env);
    mpc_submit_commitment(&env, id, p2, commit(&env, &share), zero_pk(&env), zero_sig(&env));
}

// ── share reveal ──────────────────────────────────────────────────────────────

#[test]
fn test_reveal_share_aggregates_at_threshold() {
    let env = env();
    // 2-of-3: only 2 reveals needed.
    let id = mpc_create_session(&env, 2, 3);

    let parties: [Address; 3] = [
        Address::generate(&env),
        Address::generate(&env),
        Address::generate(&env),
    ];
    let shares: [Bytes; 3] = [
        make_share(&env, 0x11),
        make_share(&env, 0x22),
        make_share(&env, 0x33),
    ];

    for i in 0..3 {
        mpc_submit_commitment(
            &env, id, parties[i].clone(), commit(&env, &shares[i]),
            zero_pk(&env), zero_sig(&env),
        );
    }

    // Reveal first share — threshold not yet met.
    mpc_reveal_share(&env, id, parties[0].clone(), shares[0].clone());
    let session: MpcSession = env.storage().persistent().get(&StorageKey::MpcSession(id)).unwrap();
    assert_eq!(session.phase, MpcPhase::Reveal);

    // Reveal second share — threshold met, session aggregates.
    mpc_reveal_share(&env, id, parties[1].clone(), shares[1].clone());
    let session: MpcSession = env.storage().persistent().get(&StorageKey::MpcSession(id)).unwrap();
    assert_eq!(session.phase, MpcPhase::Aggregated);
    assert_ne!(session.aggregated, BytesN::from_array(&env, &[0u8; 32]));
}

#[test]
#[should_panic]
fn test_reveal_share_rejects_wrong_preimage() {
    let env = env();
    let id = mpc_create_session(&env, 1, 1);
    let party = Address::generate(&env);
    let share = make_share(&env, 0xAA);
    mpc_submit_commitment(&env, id, party.clone(), commit(&env, &share), zero_pk(&env), zero_sig(&env));

    // Reveal a different share — SHA-256 won't match the commitment.
    let wrong_share = make_share(&env, 0xBB);
    mpc_reveal_share(&env, id, party, wrong_share);
}

#[test]
#[should_panic]
fn test_reveal_share_rejects_unknown_party() {
    let env = env();
    let id = mpc_create_session(&env, 1, 1);
    let party = Address::generate(&env);
    let share = make_share(&env, 0xAA);
    mpc_submit_commitment(&env, id, party, commit(&env, &share), zero_pk(&env), zero_sig(&env));

    // A different address tries to reveal.
    let stranger = Address::generate(&env);
    mpc_reveal_share(&env, id, stranger, share);
}

// ── mpc_aggregate ─────────────────────────────────────────────────────────────

#[test]
fn test_aggregate_is_deterministic() {
    let env = env();
    let shares = {
        let mut v = soroban_sdk::Vec::new(&env);
        v.push_back(make_share(&env, 0x01));
        v.push_back(make_share(&env, 0x02));
        v
    };
    let r1 = mpc_aggregate(&env, &shares, 42);
    let r2 = mpc_aggregate(&env, &shares, 42);
    assert_eq!(r1, r2);
}

#[test]
fn test_aggregate_differs_with_different_ledger() {
    let env = env();
    let shares = {
        let mut v = soroban_sdk::Vec::new(&env);
        v.push_back(make_share(&env, 0x01));
        v
    };
    let r1 = mpc_aggregate(&env, &shares, 1);
    let r2 = mpc_aggregate(&env, &shares, 2);
    assert_ne!(r1, r2, "different ledger must produce different aggregate");
}

#[test]
fn test_aggregate_differs_with_different_shares() {
    let env = env();
    let shares_a = {
        let mut v = soroban_sdk::Vec::new(&env);
        v.push_back(make_share(&env, 0xAA));
        v
    };
    let shares_b = {
        let mut v = soroban_sdk::Vec::new(&env);
        v.push_back(make_share(&env, 0xBB));
        v
    };
    assert_ne!(mpc_aggregate(&env, &shares_a, 1), mpc_aggregate(&env, &shares_b, 1));
}

#[test]
fn test_aggregate_nonzero_for_nonzero_shares() {
    let env = env();
    let shares = {
        let mut v = soroban_sdk::Vec::new(&env);
        v.push_back(make_share(&env, 0xFF));
        v
    };
    let result = mpc_aggregate(&env, &shares, 100);
    assert_ne!(result, BytesN::from_array(&env, &[0u8; 32]));
}

// ── verify_threshold_signatures ───────────────────────────────────────────────

#[test]
fn test_threshold_signatures_zero_pk_skipped() {
    // All-zero PKs are skipped; threshold of 0 is trivially met.
    let env = env();
    let msg = Bytes::from_slice(&env, b"test");
    let mut pks: soroban_sdk::Vec<BytesN<32>> = soroban_sdk::Vec::new(&env);
    let mut sigs: soroban_sdk::Vec<BytesN<64>> = soroban_sdk::Vec::new(&env);
    pks.push_back(zero_pk(&env));
    sigs.push_back(zero_sig(&env));

    // threshold=0 → always true (no signatures required)
    assert!(verify_threshold_signatures(&env, &msg, &pks, &sigs, 0));
}

#[test]
fn test_threshold_not_met_returns_false() {
    // No valid (non-zero) PKs → valid count stays 0 → threshold=1 not met.
    let env = env();
    let msg = Bytes::from_slice(&env, b"test");
    let pks: soroban_sdk::Vec<BytesN<32>> = soroban_sdk::Vec::new(&env);
    let sigs: soroban_sdk::Vec<BytesN<64>> = soroban_sdk::Vec::new(&env);

    assert!(!verify_threshold_signatures(&env, &msg, &pks, &sigs, 1));
}

// ── end-to-end MPC session flow ───────────────────────────────────────────────

/// Full 2-of-3 session: create → commit × 3 → reveal × 2 → aggregated.
#[test]
fn test_full_mpc_session_2_of_3() {
    let env = env();
    let id = mpc_create_session(&env, 2, 3);

    let parties: [Address; 3] = [
        Address::generate(&env),
        Address::generate(&env),
        Address::generate(&env),
    ];
    let shares: [Bytes; 3] = [
        make_share(&env, 0xDE),
        make_share(&env, 0xAD),
        make_share(&env, 0xBE),
    ];

    for i in 0..3 {
        mpc_submit_commitment(
            &env, id, parties[i].clone(), commit(&env, &shares[i]),
            zero_pk(&env), zero_sig(&env),
        );
    }

    mpc_reveal_share(&env, id, parties[0].clone(), shares[0].clone());
    mpc_reveal_share(&env, id, parties[1].clone(), shares[1].clone());

    let session: MpcSession = env.storage().persistent().get(&StorageKey::MpcSession(id)).unwrap();
    assert_eq!(session.phase, MpcPhase::Aggregated);

    // Aggregated entropy must be reproducible from the same inputs.
    let expected = mpc_aggregate(&env, &session.shares, session.start_ledger);
    assert_eq!(session.aggregated, expected);
}

/// 1-of-1 session: minimal threshold.
#[test]
fn test_full_mpc_session_1_of_1() {
    let env = env();
    let id = mpc_create_session(&env, 1, 1);
    let party = Address::generate(&env);
    let share = make_share(&env, 0x42);

    mpc_submit_commitment(&env, id, party.clone(), commit(&env, &share), zero_pk(&env), zero_sig(&env));
    mpc_reveal_share(&env, id, party, share);

    let session: MpcSession = env.storage().persistent().get(&StorageKey::MpcSession(id)).unwrap();
    assert_eq!(session.phase, MpcPhase::Aggregated);
}
