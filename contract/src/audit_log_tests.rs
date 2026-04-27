//! # Security Audit Log Tests (#473)
//!
//! Tests for the immutable, tamper-evident security audit log.

use super::*;
use soroban_sdk::{testutils::Address as _, Env};

fn setup() -> (Env, CoinflipContractClient<'static>, Address) {
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
    (env, client, admin)
}

// ── initial state ─────────────────────────────────────────────────────────────

#[test]
fn test_audit_log_initial_count_is_zero_or_one() {
    let (_, client, _) = setup();
    // initialize() writes one audit entry.
    let count = client.get_audit_log_count();
    assert!(count >= 1, "initialize should write at least one audit entry");
}

#[test]
fn test_audit_chain_tip_is_nonzero_after_init() {
    let (env, client, _) = setup();
    let tip = client.get_audit_chain_tip();
    assert_ne!(tip, BytesN::from_array(&env, &[0u8; 32]));
}

// ── get_audit_log_entry ───────────────────────────────────────────────────────

#[test]
fn test_get_audit_log_entry_first_entry_exists() {
    let (_, client, _) = setup();
    let entry = client.get_audit_log_entry(&0u64);
    assert!(entry.is_some());
    let e = entry.unwrap();
    assert_eq!(e.index, 0);
}

#[test]
fn test_get_audit_log_entry_out_of_range_returns_none() {
    let (_, client, _) = setup();
    let entry = client.get_audit_log_entry(&9999u64);
    assert!(entry.is_none());
}

// ── verify_audit_chain ────────────────────────────────────────────────────────

#[test]
fn test_verify_audit_chain_single_entry() {
    let (_, client, _) = setup();
    let count = client.get_audit_log_count();
    if count > 0 {
        assert!(client.verify_audit_chain(&0u64, &(count - 1)));
    }
}

#[test]
fn test_verify_audit_chain_invalid_range_returns_false() {
    let (_, client, _) = setup();
    // from > to should return false.
    assert!(!client.verify_audit_chain(&5u64, &2u64));
}

// ── admin action triggers audit entry ────────────────────────────────────────

#[test]
fn test_set_paused_creates_audit_entry() {
    let (_, client, admin) = setup();
    let before = client.get_audit_log_count();
    client.set_paused(&admin, &true, &None);
    let after = client.get_audit_log_count();
    assert!(after > before, "set_paused should append an audit log entry");
}

#[test]
fn test_set_fee_creates_audit_entry() {
    let (_, client, admin) = setup();
    let before = client.get_audit_log_count();
    client.set_fee(&admin, &400, &None);
    let after = client.get_audit_log_count();
    assert!(after > before, "set_fee should append an audit log entry");
}

// ── chain integrity after multiple entries ────────────────────────────────────

#[test]
fn test_audit_chain_integrity_after_multiple_admin_actions() {
    let (_, client, admin) = setup();
    client.set_paused(&admin, &true, &None);
    client.set_paused(&admin, &false, &None);
    client.set_fee(&admin, &400, &None);
    let count = client.get_audit_log_count();
    assert!(client.verify_audit_chain(&0u64, &(count - 1)));
}

// ── audit log constants ───────────────────────────────────────────────────────

#[test]
fn test_audit_log_max_entries_constant() {
    assert!(AUDIT_LOG_MAX_ENTRIES >= 100, "should retain at least 100 entries");
}
