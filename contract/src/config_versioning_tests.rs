/// Unit tests for config versioning and rollback functionality.
///
/// Issue: #508 — Config Versioning and Rollback
///
/// Covers:
///   - Initialize creates version 1
///   - Label validation (too long rejected)
///   - List empty history
///   - Get version not found
///   - Rollback unauthorized
///   - Rollback emits event
///   - Rollback audit label
///   - Compare identical versions
///   - Read-only queries no auth
///   - History cap evicts oldest
use super::*;
use soroban_sdk::testutils::{Address as _, Ledger};

// ── Harness ───────────────────────────────────────────────────────────────────

fn setup() -> (Env, CoinflipContractClient<'static>, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CoinflipContract, ());
    let client = CoinflipContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let token = Address::generate(&env);
    client.initialize(
        &admin,
        &treasury,
        &token,
        &300,
        &1_000_000,
        &100_000_000,
        &BytesN::from_array(&env, &[0u8; 32]),
    );
    (env, client, contract_id, admin, treasury)
}

fn make_label(env: &Env, s: &str) -> Bytes {
    Bytes::from_slice(env, s.as_bytes())
}

// ── Unit Tests ────────────────────────────────────────────────────────────────

#[test]
fn test_initialize_creates_version_1() {
    let (env, client, _contract_id, _admin, _treasury) = setup();
    let history = client.list_config_versions();
    assert_eq!(history.len(), 1);
    let v1 = history.get(0).unwrap();
    assert_eq!(v1.version_number, 1);
    assert_eq!(v1.label.len(), 0); // empty label
    assert_eq!(v1.config.fee_bps, 300);
}

#[test]
fn test_label_too_long_rejected() {
    let (env, client, _contract_id, admin, _treasury) = setup();
    // 65 bytes → exceeds MAX_LABEL_BYTES (64)
    let long_label = Bytes::from_slice(&env, &[b'x'; 65]);
    let result = client.try_set_fee(&admin, &350, &Some(long_label));
    assert_eq!(result, Err(Ok(Error::InvalidVersionLabel)));
    // Config unchanged
    let history = client.list_config_versions();
    assert_eq!(history.len(), 1); // still only version 1 from initialize
}

#[test]
fn test_list_empty_history() {
    // Fresh contract with no initialize → empty history
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CoinflipContract, ());
    let client = CoinflipContractClient::new(&env, &contract_id);
    let history = client.list_config_versions();
    assert_eq!(history.len(), 0);
}

#[test]
fn test_get_version_not_found() {
    let (_env, client, _contract_id, _admin, _treasury) = setup();
    let result = client.try_get_config_version(&999);
    assert_eq!(result, Err(Ok(Error::VersionNotFound)));
}

#[test]
fn test_rollback_unauthorized() {
    let (env, client, _contract_id, _admin, _treasury) = setup();
    let attacker = Address::generate(&env);
    let result = client.try_rollback_config(&attacker, &1);
    assert_eq!(result, Err(Ok(Error::Unauthorized)));
    // State unchanged
    let history = client.list_config_versions();
    assert_eq!(history.len(), 1); // still only version 1
}

#[test]
fn test_rollback_emits_event() {
    let (env, client, _contract_id, admin, _treasury) = setup();
    // Create version 2 by changing fee
    client.set_fee(&admin, &350, &None);
    let history = client.list_config_versions();
    assert_eq!(history.len(), 2);
    // Rollback to version 1
    client.rollback_config(&admin, &1);
    // Check event
    let events = env.events().all();
    let last_event = events.last().unwrap();
    assert_eq!(last_event.0, (symbol_short!("tossd"), Symbol::new(&env, "config_rollback")));
    let (target_v, new_v): (u32, u32) = last_event.1.try_into().unwrap();
    assert_eq!(target_v, 1);
    assert_eq!(new_v, 3); // version 3 is the rollback audit snapshot
}

#[test]
fn test_rollback_audit_label() {
    let (env, client, _contract_id, admin, _treasury) = setup();
    // Create version 2
    client.set_fee(&admin, &350, &None);
    // Rollback to version 1
    client.rollback_config(&admin, &1);
    let history = client.list_config_versions();
    assert_eq!(history.len(), 3);
    let v3 = history.get(2).unwrap();
    let label_str = core::str::from_utf8(&v3.label.to_vec()).unwrap();
    assert_eq!(label_str, "rollback to v1");
}

#[test]
fn test_compare_identical_versions() {
    let (_env, client, _contract_id, _admin, _treasury) = setup();
    let diff = client.compare_config_versions(&1, &1).unwrap();
    assert_eq!(diff.len(), 0);
}

#[test]
fn test_read_only_queries_no_auth() {
    let (env, client, _contract_id, admin, _treasury) = setup();
    // Create version 2
    client.set_fee(&admin, &350, &None);
    // Any address can call queries
    let anyone = Address::generate(&env);
    env.mock_all_auths();
    let history = client.list_config_versions();
    assert_eq!(history.len(), 2);
    let v1 = client.get_config_version(&1).unwrap();
    assert_eq!(v1.version_number, 1);
    let diff = client.compare_config_versions(&1, &2).unwrap();
    assert!(diff.len() > 0); // fee_bps differs
}

#[test]
fn test_history_cap_evicts_oldest() {
    let (env, client, _contract_id, admin, _treasury) = setup();
    // Create 50 more versions (total 51 including initialize)
    for i in 0..50 {
        let label = make_label(&env, &format!("v{}", i + 2));
        client.set_fee(&admin, &(300 + (i % 200)), &Some(label));
    }
    let history = client.list_config_versions();
    assert_eq!(history.len(), 50); // capped at MAX_CONFIG_HISTORY
    // Version 1 should be evicted
    let first = history.get(0).unwrap();
    assert_eq!(first.version_number, 2); // version 1 is gone
    let last = history.last().unwrap();
    assert_eq!(last.version_number, 51);
}

#[test]
fn test_rollback_round_trip() {
    let (_env, client, _contract_id, admin, _treasury) = setup();
    // Version 1: fee_bps = 300
    // Create version 2: fee_bps = 400
    client.set_fee(&admin, &400, &None);
    let v2 = client.get_config_version(&2).unwrap();
    assert_eq!(v2.config.fee_bps, 400);
    // Rollback to version 1
    client.rollback_config(&admin, &1);
    // Live config should match version 1
    let history = client.list_config_versions();
    let v3 = history.last().unwrap(); // rollback audit snapshot
    assert_eq!(v3.config.fee_bps, 300);
}

#[test]
fn test_compare_versions_diff() {
    let (env, client, _contract_id, admin, _treasury) = setup();
    // Version 1: fee_bps = 300
    // Version 2: fee_bps = 400
    client.set_fee(&admin, &400, &None);
    let diff = client.compare_config_versions(&1, &2).unwrap();
    assert_eq!(diff.len(), 1);
    let entry = diff.get(0).unwrap();
    assert_eq!(entry.field, Symbol::new(&env, "fee_bps"));
}

#[test]
fn test_snapshot_on_set_wager_limits() {
    let (env, client, _contract_id, admin, _treasury) = setup();
    let label = make_label(&env, "new limits");
    client.set_wager_limits(&admin, &500_000, &200_000_000, &Some(label.clone()));
    let history = client.list_config_versions();
    assert_eq!(history.len(), 2);
    let v2 = history.get(1).unwrap();
    assert_eq!(v2.label, label);
    assert_eq!(v2.config.min_wager, 500_000);
}

#[test]
fn test_snapshot_on_set_treasury() {
    let (env, client, _contract_id, admin, treasury) = setup();
    let new_treasury = Address::generate(&env);
    let label = make_label(&env, "treasury change");
    client.set_treasury(&admin, &new_treasury, &Some(label.clone()));
    let history = client.list_config_versions();
    assert_eq!(history.len(), 2);
    let v2 = history.get(1).unwrap();
    assert_eq!(v2.label, label);
    assert_eq!(v2.config.treasury, new_treasury);
}

#[test]
fn test_snapshot_on_set_paused() {
    let (env, client, _contract_id, admin, _treasury) = setup();
    let label = make_label(&env, "pause");
    client.set_paused(&admin, &true, &Some(label.clone()));
    let history = client.list_config_versions();
    assert_eq!(history.len(), 2);
    let v2 = history.get(1).unwrap();
    assert_eq!(v2.label, label);
    assert_eq!(v2.config.paused, true);
}

#[test]
fn test_snapshot_on_set_multipliers() {
    let (env, client, _contract_id, admin, _treasury) = setup();
    let label = make_label(&env, "new multipliers");
    client.set_multipliers(&admin, &19_000, &38_000, &76_000, &152_000, &Some(label.clone()));
    let history = client.list_config_versions();
    assert_eq!(history.len(), 2);
    let v2 = history.get(1).unwrap();
    assert_eq!(v2.label, label);
}

#[test]
fn test_rollback_version_not_found() {
    let (_env, client, _contract_id, admin, _treasury) = setup();
    let result = client.try_rollback_config(&admin, &999);
    assert_eq!(result, Err(Ok(Error::VersionNotFound)));
}

#[test]
fn test_compare_versions_missing_a() {
    let (_env, client, _contract_id, _admin, _treasury) = setup();
    let result = client.try_compare_config_versions(&999, &1);
    assert_eq!(result, Err(Ok(Error::VersionNotFound)));
}

#[test]
fn test_compare_versions_missing_b() {
    let (_env, client, _contract_id, _admin, _treasury) = setup();
    let result = client.try_compare_config_versions(&1, &999);
    assert_eq!(result, Err(Ok(Error::VersionNotFound)));
}
