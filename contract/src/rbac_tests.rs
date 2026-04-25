//! # RBAC Tests
//!
//! Verifies role-based access control for all admin operations.

use super::*;
use soroban_sdk::testutils::Address as _;

fn setup(env: &Env) -> (Address, CoinflipContractClient) {
    env.mock_all_auths();
    let contract_id = env.register(CoinflipContract, ());
    let client = CoinflipContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let treasury = Address::generate(env);
    let token = Address::generate(env);
    client.initialize(&admin, &treasury, &token, &300, &1_000_000, &100_000_000);
    (admin, client)
}

// ── grant_role / revoke_role ──────────────────────────────────────────────────

#[test]
fn test_grant_role_by_super_admin() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let grantee = Address::generate(&env);
    assert!(client.try_grant_role(&admin, &grantee, &Role::ConfigAdmin).is_ok());
    assert_eq!(client.get_role_of(&grantee), Some(Role::ConfigAdmin));
}

#[test]
fn test_grant_role_rejected_for_non_super_admin() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let config_admin = Address::generate(&env);
    client.grant_role(&admin, &config_admin, &Role::ConfigAdmin);

    let target = Address::generate(&env);
    // ConfigAdmin cannot grant roles
    assert_eq!(
        client.try_grant_role(&config_admin, &target, &Role::PauseAdmin),
        Err(Ok(Error::InsufficientRole))
    );
}

#[test]
fn test_revoke_role_by_super_admin() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let grantee = Address::generate(&env);
    client.grant_role(&admin, &grantee, &Role::PauseAdmin);
    client.revoke_role(&admin, &grantee);
    assert_eq!(client.get_role_of(&grantee), None);
}

#[test]
fn test_revoke_role_rejected_for_non_super_admin() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let pause_admin = Address::generate(&env);
    client.grant_role(&admin, &pause_admin, &Role::PauseAdmin);
    assert_eq!(
        client.try_revoke_role(&pause_admin, &pause_admin),
        Err(Ok(Error::InsufficientRole))
    );
}

#[test]
fn test_config_admin_implicit_super_admin() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    // config.admin always has SuperAdmin without an explicit grant
    assert_eq!(client.get_role_of(&admin), Some(Role::SuperAdmin));
}

// ── set_paused (PauseAdmin+) ──────────────────────────────────────────────────

#[test]
fn test_set_paused_allowed_for_pause_admin() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let pause_admin = Address::generate(&env);
    client.grant_role(&admin, &pause_admin, &Role::PauseAdmin);
    assert!(client.try_set_paused(&pause_admin, &true).is_ok());
    assert!(client.get_config().paused);
}

#[test]
fn test_set_paused_allowed_for_config_admin() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let config_admin = Address::generate(&env);
    client.grant_role(&admin, &config_admin, &Role::ConfigAdmin);
    assert!(client.try_set_paused(&config_admin, &true).is_ok());
}

#[test]
fn test_set_paused_allowed_for_super_admin() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    assert!(client.try_set_paused(&admin, &true).is_ok());
}

#[test]
fn test_set_paused_rejected_for_no_role() {
    let env = Env::default();
    let (_, client) = setup(&env);
    let stranger = Address::generate(&env);
    assert_eq!(
        client.try_set_paused(&stranger, &true),
        Err(Ok(Error::InsufficientRole))
    );
}

// ── set_fee (ConfigAdmin+) ────────────────────────────────────────────────────

#[test]
fn test_set_fee_allowed_for_config_admin() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let config_admin = Address::generate(&env);
    client.grant_role(&admin, &config_admin, &Role::ConfigAdmin);
    assert!(client.try_set_fee(&config_admin, &400).is_ok());
    assert_eq!(client.get_config().fee_bps, 400);
}

#[test]
fn test_set_fee_rejected_for_pause_admin() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let pause_admin = Address::generate(&env);
    client.grant_role(&admin, &pause_admin, &Role::PauseAdmin);
    assert_eq!(
        client.try_set_fee(&pause_admin, &400),
        Err(Ok(Error::InsufficientRole))
    );
}

#[test]
fn test_set_fee_rejected_for_no_role() {
    let env = Env::default();
    let (_, client) = setup(&env);
    let stranger = Address::generate(&env);
    assert_eq!(
        client.try_set_fee(&stranger, &400),
        Err(Ok(Error::InsufficientRole))
    );
}

// ── set_wager_limits (ConfigAdmin+) ──────────────────────────────────────────

#[test]
fn test_set_wager_limits_allowed_for_config_admin() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let config_admin = Address::generate(&env);
    client.grant_role(&admin, &config_admin, &Role::ConfigAdmin);
    assert!(client.try_set_wager_limits(&config_admin, &2_000_000, &200_000_000).is_ok());
}

#[test]
fn test_set_wager_limits_rejected_for_pause_admin() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let pause_admin = Address::generate(&env);
    client.grant_role(&admin, &pause_admin, &Role::PauseAdmin);
    assert_eq!(
        client.try_set_wager_limits(&pause_admin, &2_000_000, &200_000_000),
        Err(Ok(Error::InsufficientRole))
    );
}

// ── set_multipliers (ConfigAdmin+) ───────────────────────────────────────────

#[test]
fn test_set_multipliers_allowed_for_config_admin() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let config_admin = Address::generate(&env);
    client.grant_role(&admin, &config_admin, &Role::ConfigAdmin);
    assert!(client.try_set_multipliers(&config_admin, &20_000, &40_000, &70_000, &110_000).is_ok());
}

#[test]
fn test_set_multipliers_rejected_for_pause_admin() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let pause_admin = Address::generate(&env);
    client.grant_role(&admin, &pause_admin, &Role::PauseAdmin);
    assert_eq!(
        client.try_set_multipliers(&pause_admin, &20_000, &40_000, &70_000, &110_000),
        Err(Ok(Error::InsufficientRole))
    );
}

// ── set_treasury (SuperAdmin only) ────────────────────────────────────────────

#[test]
fn test_set_treasury_allowed_for_super_admin_only() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let new_treasury = Address::generate(&env);
    assert!(client.try_set_treasury(&admin, &new_treasury).is_ok());
}

#[test]
fn test_set_treasury_rejected_for_config_admin() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let config_admin = Address::generate(&env);
    client.grant_role(&admin, &config_admin, &Role::ConfigAdmin);
    let new_treasury = Address::generate(&env);
    assert_eq!(
        client.try_set_treasury(&config_admin, &new_treasury),
        Err(Ok(Error::InsufficientRole))
    );
}

#[test]
fn test_set_treasury_rejected_for_pause_admin() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let pause_admin = Address::generate(&env);
    client.grant_role(&admin, &pause_admin, &Role::PauseAdmin);
    let new_treasury = Address::generate(&env);
    assert_eq!(
        client.try_set_treasury(&pause_admin, &new_treasury),
        Err(Ok(Error::InsufficientRole))
    );
}

// ── role upgrade / downgrade ──────────────────────────────────────────────────

#[test]
fn test_role_upgrade_grants_additional_permissions() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let operator = Address::generate(&env);

    // Start as PauseAdmin — cannot set fee
    client.grant_role(&admin, &operator, &Role::PauseAdmin);
    assert_eq!(client.try_set_fee(&operator, &400), Err(Ok(Error::InsufficientRole)));

    // Upgrade to ConfigAdmin — can now set fee
    client.grant_role(&admin, &operator, &Role::ConfigAdmin);
    assert!(client.try_set_fee(&operator, &400).is_ok());
}

#[test]
fn test_revoked_role_loses_permissions() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let pause_admin = Address::generate(&env);
    client.grant_role(&admin, &pause_admin, &Role::PauseAdmin);

    // Can pause before revocation
    assert!(client.try_set_paused(&pause_admin, &true).is_ok());
    client.set_paused(&admin, &false); // reset

    // Revoke and verify permission is gone
    client.revoke_role(&admin, &pause_admin);
    assert_eq!(
        client.try_set_paused(&pause_admin, &true),
        Err(Ok(Error::InsufficientRole))
    );
}
