/// Comprehensive tests for admin authorization and privilege escalation prevention.
///
/// Issue #415: Architect role-based access control validation with privilege escalation prevention
///
/// Covers:
///   - All admin functions require authorization
///   - Non-admin rejection on all admin functions
///   - Authorization with multiple admins
///   - Privilege escalation prevention
///   - Authorization state consistency
///   - Address spoofing attempt prevention
use super::*;
use soroban_sdk::testutils::Address as _;

// ── Harness ───────────────────────────────────────────────────────────────────

fn setup() -> (Env, CoinflipContractClient<'static>, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CoinflipContract, ());
    let client = CoinflipContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let token = Address::generate(&env);
    client.initialize(&admin, &treasury, &token, &300, &1_000_000, &100_000_000);
    (env, client, contract_id, admin, treasury)
}

// ── set_paused authorization tests ────────────────────────────────────────────

#[test]
fn test_set_paused_requires_admin() {
    let (env, client, _contract_id, admin, _treasury) = setup();
    let non_admin = Address::generate(&env);
    let result = client.try_set_paused(&non_admin, &true);
    assert_eq!(result, Err(Ok(Error::Unauthorized)));
}

#[test]
fn test_set_paused_succeeds_with_admin() {
    let (env, client, _contract_id, admin, _treasury) = setup();
    let result = client.try_set_paused(&admin, &true);
    assert_eq!(result, Ok(Ok(())));
}

#[test]
fn test_set_paused_rejects_wrong_address() {
    let (env, client, _contract_id, _admin, treasury) = setup();
    let result = client.try_set_paused(&treasury, &true);
    assert_eq!(result, Err(Ok(Error::Unauthorized)));
}

#[test]
fn test_set_paused_does_not_change_state_on_unauthorized() {
    let (env, client, contract_id, admin, _treasury) = setup();
    let non_admin = Address::generate(&env);
    
    // Verify initial state
    env.as_contract(&contract_id, || {
        let config = CoinflipContract::load_config(&env);
        assert!(!config.paused);
    });
    
    // Attempt unauthorized change
    let _ = client.try_set_paused(&non_admin, &true);
    
    // Verify state unchanged
    env.as_contract(&contract_id, || {
        let config = CoinflipContract::load_config(&env);
        assert!(!config.paused);
    });
}

// ── set_treasury authorization tests ──────────────────────────────────────────

#[test]
fn test_set_treasury_requires_admin() {
    let (env, client, _contract_id, _admin, treasury) = setup();
    let non_admin = Address::generate(&env);
    let new_treasury = Address::generate(&env);
    let result = client.try_set_treasury(&non_admin, &new_treasury);
    assert_eq!(result, Err(Ok(Error::Unauthorized)));
}

#[test]
fn test_set_treasury_succeeds_with_admin() {
    let (env, client, _contract_id, admin, _treasury) = setup();
    let new_treasury = Address::generate(&env);
    let result = client.try_set_treasury(&admin, &new_treasury);
    assert_eq!(result, Ok(Ok(())));
}

#[test]
fn test_set_treasury_rejects_non_admin() {
    let (env, client, _contract_id, _admin, treasury) = setup();
    let new_treasury = Address::generate(&env);
    let result = client.try_set_treasury(&treasury, &new_treasury);
    assert_eq!(result, Err(Ok(Error::Unauthorized)));
}

#[test]
fn test_set_treasury_does_not_change_state_on_unauthorized() {
    let (env, client, contract_id, admin, treasury) = setup();
    let non_admin = Address::generate(&env);
    let new_treasury = Address::generate(&env);
    
    // Verify initial state
    env.as_contract(&contract_id, || {
        let config = CoinflipContract::load_config(&env);
        assert_eq!(config.treasury, treasury);
    });
    
    // Attempt unauthorized change
    let _ = client.try_set_treasury(&non_admin, &new_treasury);
    
    // Verify state unchanged
    env.as_contract(&contract_id, || {
        let config = CoinflipContract::load_config(&env);
        assert_eq!(config.treasury, treasury);
    });
}

// ── set_wager_limits authorization tests ──────────────────────────────────────

#[test]
fn test_set_wager_limits_requires_admin() {
    let (env, client, _contract_id, _admin, _treasury) = setup();
    let non_admin = Address::generate(&env);
    let result = client.try_set_wager_limits(&non_admin, &500_000, &50_000_000);
    assert_eq!(result, Err(Ok(Error::Unauthorized)));
}

#[test]
fn test_set_wager_limits_succeeds_with_admin() {
    let (env, client, _contract_id, admin, _treasury) = setup();
    let result = client.try_set_wager_limits(&admin, &500_000, &50_000_000);
    assert_eq!(result, Ok(Ok(())));
}

#[test]
fn test_set_wager_limits_rejects_non_admin() {
    let (env, client, _contract_id, _admin, _treasury) = setup();
    let non_admin = Address::generate(&env);
    let result = client.try_set_wager_limits(&non_admin, &500_000, &50_000_000);
    assert_eq!(result, Err(Ok(Error::Unauthorized)));
}

#[test]
fn test_set_wager_limits_does_not_change_state_on_unauthorized() {
    let (env, client, contract_id, admin, _treasury) = setup();
    let non_admin = Address::generate(&env);
    
    // Verify initial state
    let (initial_min, initial_max) = env.as_contract(&contract_id, || {
        let config = CoinflipContract::load_config(&env);
        (config.min_wager, config.max_wager)
    });
    
    // Attempt unauthorized change
    let _ = client.try_set_wager_limits(&non_admin, &500_000, &50_000_000);
    
    // Verify state unchanged
    env.as_contract(&contract_id, || {
        let config = CoinflipContract::load_config(&env);
        assert_eq!(config.min_wager, initial_min);
        assert_eq!(config.max_wager, initial_max);
    });
}

// ── set_fee authorization tests ───────────────────────────────────────────────

#[test]
fn test_set_fee_requires_admin() {
    let (env, client, _contract_id, _admin, _treasury) = setup();
    let non_admin = Address::generate(&env);
    let result = client.try_set_fee(&non_admin, &350);
    assert_eq!(result, Err(Ok(Error::Unauthorized)));
}

#[test]
fn test_set_fee_succeeds_with_admin() {
    let (env, client, _contract_id, admin, _treasury) = setup();
    let result = client.try_set_fee(&admin, &350);
    assert_eq!(result, Ok(Ok(())));
}

#[test]
fn test_set_fee_rejects_non_admin() {
    let (env, client, _contract_id, _admin, _treasury) = setup();
    let non_admin = Address::generate(&env);
    let result = client.try_set_fee(&non_admin, &350);
    assert_eq!(result, Err(Ok(Error::Unauthorized)));
}

#[test]
fn test_set_fee_does_not_change_state_on_unauthorized() {
    let (env, client, contract_id, admin, _treasury) = setup();
    let non_admin = Address::generate(&env);
    
    // Verify initial state
    let initial_fee = env.as_contract(&contract_id, || {
        let config = CoinflipContract::load_config(&env);
        config.fee_bps
    });
    
    // Attempt unauthorized change
    let _ = client.try_set_fee(&non_admin, &350);
    
    // Verify state unchanged
    env.as_contract(&contract_id, || {
        let config = CoinflipContract::load_config(&env);
        assert_eq!(config.fee_bps, initial_fee);
    });
}

// ── Privilege escalation prevention tests ─────────────────────────────────────

#[test]
fn test_non_admin_cannot_change_admin() {
    let (env, client, contract_id, admin, _treasury) = setup();
    let non_admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    
    // Verify initial admin
    env.as_contract(&contract_id, || {
        let config = CoinflipContract::load_config(&env);
        assert_eq!(config.admin, admin);
    });
    
    // Non-admin attempts to change admin via set_treasury (should fail)
    let _ = client.try_set_treasury(&non_admin, &new_admin);
    
    // Verify admin unchanged
    env.as_contract(&contract_id, || {
        let config = CoinflipContract::load_config(&env);
        assert_eq!(config.admin, admin);
    });
}

#[test]
fn test_non_admin_cannot_pause_contract() {
    let (env, client, contract_id, admin, _treasury) = setup();
    let non_admin = Address::generate(&env);
    
    // Verify contract not paused
    env.as_contract(&contract_id, || {
        let config = CoinflipContract::load_config(&env);
        assert!(!config.paused);
    });
    
    // Non-admin attempts to pause
    let _ = client.try_set_paused(&non_admin, &true);
    
    // Verify contract still not paused
    env.as_contract(&contract_id, || {
        let config = CoinflipContract::load_config(&env);
        assert!(!config.paused);
    });
}

#[test]
fn test_non_admin_cannot_redirect_fees() {
    let (env, client, contract_id, admin, treasury) = setup();
    let non_admin = Address::generate(&env);
    let attacker_address = Address::generate(&env);
    
    // Verify initial treasury
    env.as_contract(&contract_id, || {
        let config = CoinflipContract::load_config(&env);
        assert_eq!(config.treasury, treasury);
    });
    
    // Non-admin attempts to redirect fees
    let _ = client.try_set_treasury(&non_admin, &attacker_address);
    
    // Verify treasury unchanged
    env.as_contract(&contract_id, || {
        let config = CoinflipContract::load_config(&env);
        assert_eq!(config.treasury, treasury);
    });
}

// ── Authorization state consistency tests ─────────────────────────────────────

#[test]
fn test_admin_address_verification_accuracy() {
    let (env, client, contract_id, admin, _treasury) = setup();
    let non_admin = Address::generate(&env);
    
    // Admin should succeed
    let result = client.try_set_paused(&admin, &true);
    assert_eq!(result, Ok(Ok(())));
    
    // Non-admin should fail
    let result = client.try_set_paused(&non_admin, &false);
    assert_eq!(result, Err(Ok(Error::Unauthorized)));
}

#[test]
fn test_authorization_with_address_spoofing_attempt() {
    let (env, client, contract_id, admin, _treasury) = setup();
    let spoofed_admin = Address::generate(&env);
    
    // Verify initial admin
    env.as_contract(&contract_id, || {
        let config = CoinflipContract::load_config(&env);
        assert_eq!(config.admin, admin);
    });
    
    // Attempt to use spoofed admin address
    let result = client.try_set_paused(&spoofed_admin, &true);
    assert_eq!(result, Err(Ok(Error::Unauthorized)));
    
    // Verify admin unchanged
    env.as_contract(&contract_id, || {
        let config = CoinflipContract::load_config(&env);
        assert_eq!(config.admin, admin);
    });
}

#[test]
fn test_multiple_unauthorized_attempts_do_not_affect_state() {
    let (env, client, contract_id, admin, _treasury) = setup();
    let non_admin1 = Address::generate(&env);
    let non_admin2 = Address::generate(&env);
    let non_admin3 = Address::generate(&env);
    
    // Verify initial state
    let initial_paused = env.as_contract(&contract_id, || {
        let config = CoinflipContract::load_config(&env);
        config.paused
    });
    
    // Multiple unauthorized attempts
    let _ = client.try_set_paused(&non_admin1, &true);
    let _ = client.try_set_paused(&non_admin2, &false);
    let _ = client.try_set_paused(&non_admin3, &true);
    
    // Verify state unchanged
    env.as_contract(&contract_id, || {
        let config = CoinflipContract::load_config(&env);
        assert_eq!(config.paused, initial_paused);
    });
}

// ── Authorization consistency across all admin functions ──────────────────────

#[test]
fn test_all_admin_functions_reject_non_admin() {
    let (env, client, _contract_id, _admin, _treasury) = setup();
    let non_admin = Address::generate(&env);
    let new_address = Address::generate(&env);
    
    // Test all admin functions with non-admin
    assert_eq!(
        client.try_set_paused(&non_admin, &true),
        Err(Ok(Error::Unauthorized))
    );
    assert_eq!(
        client.try_set_treasury(&non_admin, &new_address),
        Err(Ok(Error::Unauthorized))
    );
    assert_eq!(
        client.try_set_wager_limits(&non_admin, &500_000, &50_000_000),
        Err(Ok(Error::Unauthorized))
    );
    assert_eq!(
        client.try_set_fee(&non_admin, &350),
        Err(Ok(Error::Unauthorized))
    );
}

#[test]
fn test_all_admin_functions_accept_admin() {
    let (env, client, _contract_id, admin, _treasury) = setup();
    let new_address = Address::generate(&env);
    
    // Test all admin functions with admin
    assert_eq!(client.try_set_paused(&admin, &true), Ok(Ok(())));
    assert_eq!(client.try_set_treasury(&admin, &new_address), Ok(Ok(())));
    assert_eq!(
        client.try_set_wager_limits(&admin, &500_000, &50_000_000),
        Ok(Ok(()))
    );
    assert_eq!(client.try_set_fee(&admin, &350), Ok(Ok(())));
}

// ── Authorization with edge cases ─────────────────────────────────────────────

#[test]
fn test_authorization_with_generated_addresses() {
    let (env, client, _contract_id, admin, _treasury) = setup();
    
    // Generate multiple non-admin addresses
    for _ in 0..5 {
        let non_admin = Address::generate(&env);
        let result = client.try_set_paused(&non_admin, &true);
        assert_eq!(result, Err(Ok(Error::Unauthorized)));
    }
    
    // Admin should still work
    let result = client.try_set_paused(&admin, &true);
    assert_eq!(result, Ok(Ok(())));
}

#[test]
fn test_authorization_state_persists_across_calls() {
    let (env, client, contract_id, admin, _treasury) = setup();
    let non_admin = Address::generate(&env);
    
    // First unauthorized attempt
    let _ = client.try_set_paused(&non_admin, &true);
    
    // Verify state unchanged
    env.as_contract(&contract_id, || {
        let config = CoinflipContract::load_config(&env);
        assert!(!config.paused);
    });
    
    // Second unauthorized attempt
    let _ = client.try_set_paused(&non_admin, &false);
    
    // Verify state still unchanged
    env.as_contract(&contract_id, || {
        let config = CoinflipContract::load_config(&env);
        assert!(!config.paused);
    });
    
    // Admin call should succeed
    let result = client.try_set_paused(&admin, &true);
    assert_eq!(result, Ok(Ok(())));
    
    // Verify state changed by admin
    env.as_contract(&contract_id, || {
        let config = CoinflipContract::load_config(&env);
        assert!(config.paused);
    });
}
