//! Tests for contract upgrade, versioning, and rollback (issue #468).

#[cfg(test)]
mod upgrade_migration_tests {
    use crate::*;
    use soroban_sdk::{testutils::Address as _, Env};

    fn setup() -> (Env, CoinflipContractClient<'static>, Address, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(CoinflipContract, ());
        let client = CoinflipContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        let token = Address::generate(&env);
        client.initialize(
            &admin, &treasury, &token, &300, &1_000_000, &100_000_000,
            &BytesN::from_array(&env, &[0u8; 32]),
        );
        (env, client, contract_id, admin)
    }

    /// initialize creates version 1 with the genesis config.
    #[test]
    fn test_initialize_creates_version_1() {
        let (_env, client, _cid, _admin) = setup();
        let history = client.list_config_versions();
        assert_eq!(history.len(), 1);
        let v1 = history.get(0).unwrap();
        assert_eq!(v1.version_number, 1);
        assert_eq!(v1.config.fee_bps, 300);
    }

    /// Each admin write appends a new version.
    #[test]
    fn test_each_write_increments_version() {
        let (_env, client, _cid, admin) = setup();
        client.set_fee(&admin, &350, &None);
        client.set_fee(&admin, &400, &None);
        let history = client.list_config_versions();
        assert_eq!(history.len(), 3);
        assert_eq!(history.get(2).unwrap().version_number, 3);
    }

    /// get_config_version returns the correct snapshot.
    #[test]
    fn test_get_config_version_returns_snapshot() {
        let (_env, client, _cid, admin) = setup();
        client.set_fee(&admin, &400, &None);
        let v1 = client.get_config_version(&1).unwrap();
        assert_eq!(v1.config.fee_bps, 300);
        let v2 = client.get_config_version(&2).unwrap();
        assert_eq!(v2.config.fee_bps, 400);
    }

    /// get_config_version returns VersionNotFound for unknown version.
    #[test]
    fn test_get_version_not_found() {
        let (_env, client, _cid, _admin) = setup();
        assert_eq!(
            client.try_get_config_version(&999),
            Err(Ok(Error::VersionNotFound))
        );
    }

    /// rollback restores the target config and appends an audit snapshot.
    #[test]
    fn test_rollback_restores_config() {
        let (_env, client, _cid, admin) = setup();
        client.set_fee(&admin, &400, &None);
        client.rollback_config(&admin, &1);
        // Version 3 is the rollback audit snapshot — it should have fee_bps = 300.
        let v3 = client.get_config_version(&3).unwrap();
        assert_eq!(v3.config.fee_bps, 300);
    }

    /// rollback is rejected for non-admin callers.
    #[test]
    fn test_rollback_unauthorized() {
        let (env, client, _cid, _admin) = setup();
        let attacker = Address::generate(&env);
        assert_eq!(
            client.try_rollback_config(&attacker, &1),
            Err(Ok(Error::Unauthorized))
        );
    }

    /// rollback to missing version returns VersionNotFound.
    #[test]
    fn test_rollback_version_not_found() {
        let (_env, client, _cid, admin) = setup();
        assert_eq!(
            client.try_rollback_config(&admin, &999),
            Err(Ok(Error::VersionNotFound))
        );
    }

    /// compare_config_versions returns empty diff for identical versions.
    #[test]
    fn test_compare_identical_versions_empty_diff() {
        let (_env, client, _cid, _admin) = setup();
        let diff = client.compare_config_versions(&1, &1).unwrap();
        assert_eq!(diff.len(), 0);
    }

    /// compare_config_versions detects changed fee_bps.
    #[test]
    fn test_compare_versions_detects_diff() {
        let (env, client, _cid, admin) = setup();
        client.set_fee(&admin, &400, &None);
        let diff = client.compare_config_versions(&1, &2).unwrap();
        assert_eq!(diff.len(), 1);
        assert_eq!(diff.get(0).unwrap().field, Symbol::new(&env, "fee_bps"));
    }

    /// Label too long is rejected; config unchanged.
    #[test]
    fn test_label_too_long_rejected() {
        let (env, client, _cid, admin) = setup();
        let long_label = Bytes::from_slice(&env, &[b'x'; 65]);
        assert_eq!(
            client.try_set_fee(&admin, &350, &Some(long_label)),
            Err(Ok(Error::InvalidVersionLabel))
        );
        // Still only version 1.
        assert_eq!(client.list_config_versions().len(), 1);
    }

    /// History is capped at MAX_CONFIG_HISTORY; oldest entry is evicted.
    #[test]
    fn test_history_cap_evicts_oldest() {
        let (env, client, _cid, admin) = setup();
        for i in 0..50u32 {
            let label = Bytes::from_slice(&env, format!("v{}", i + 2).as_bytes());
            client.set_fee(&admin, &(300 + (i % 200)), &Some(label));
        }
        let history = client.list_config_versions();
        assert_eq!(history.len(), 50);
        // Version 1 evicted; first remaining is version 2.
        assert_eq!(history.get(0).unwrap().version_number, 2);
    }

    /// Error codes are stable (backward compatibility guarantee).
    #[test]
    fn test_error_code_stability() {
        assert_eq!(error_codes::WAGER_BELOW_MINIMUM, 1);
        assert_eq!(error_codes::WAGER_ABOVE_MAXIMUM, 2);
        assert_eq!(error_codes::ACTIVE_GAME_EXISTS, 3);
        assert_eq!(error_codes::INSUFFICIENT_RESERVES, 4);
        assert_eq!(error_codes::CONTRACT_PAUSED, 5);
        assert_eq!(error_codes::NO_ACTIVE_GAME, 10);
        assert_eq!(error_codes::COMMITMENT_MISMATCH, 12);
        assert_eq!(error_codes::UNAUTHORIZED, 30);
        assert_eq!(error_codes::ALREADY_INITIALIZED, 51);
        assert_eq!(error_codes::VERSION_NOT_FOUND, 36);
    }
}
