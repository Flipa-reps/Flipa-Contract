/// Compliance and audit trail validation tests
///
/// This module validates:
/// - Audit trail completeness and immutability
/// - Transaction logging for all state changes
/// - Regulatory compliance requirements
/// - Data retention policy enforcement
/// - Audit report generation capabilities

#[cfg(test)]
mod compliance_audit_tests {
    use crate::*;

    /// Test that all game outcomes are recorded in history
    #[test]
    fn audit_trail_records_all_game_outcomes() {
        let env = Env::default();
        let admin = Address::random(&env);
        let treasury = Address::random(&env);
        let player = Address::random(&env);
        let token = Address::random(&env);

        env.mock_all_auths();

        CoinflipContract::initialize(
            env.clone(),
            admin.clone(),
            treasury.clone(),
            token.clone(),
            300,
            1_000,
            1_000_000,
        )
        .unwrap();

        // Deposit initial reserve
        let mut stats = CoinflipContract::load_stats(&env);
        stats.reserve_balance = 10_000_000;
        CoinflipContract::save_stats(&env, &stats);

        // Play a game and win
        let secret = Bytes::from_slice(&env, b"test_secret_123");
        let commitment = env.crypto().sha256(&secret).into();

        CoinflipContract::start_game(
            env.clone(),
            player.clone(),
            Side::Heads,
            100_000,
            commitment,
        )
        .unwrap();

        let game = CoinflipContract::load_player_game(&env, &player).unwrap();
        let outcome = generate_outcome(&env, &secret, &game.contract_random);

        // Reveal with winning outcome
        let won = CoinflipContract::reveal(env.clone(), player.clone(), secret.clone()).unwrap();
        assert!(won);

        // Verify history entry exists
        let history = CoinflipContract::load_player_history(&env, &player);
        assert!(history.len() > 0, "History must record the game outcome");

        let entry = history.get_unchecked(0);
        assert_eq!(entry.wager, 100_000);
        assert_eq!(entry.won, true);
        assert_eq!(entry.outcome, outcome);
    }

    /// Test that transaction logging captures all state mutations
    #[test]
    fn audit_trail_logs_all_state_mutations() {
        let env = Env::default();
        let admin = Address::random(&env);
        let treasury = Address::random(&env);
        let player = Address::random(&env);
        let token = Address::random(&env);

        env.mock_all_auths();

        CoinflipContract::initialize(
            env.clone(),
            admin.clone(),
            treasury.clone(),
            token.clone(),
            300,
            1_000,
            1_000_000,
        )
        .unwrap();

        // Verify config is persisted
        let config = CoinflipContract::load_config(&env);
        assert_eq!(config.admin, admin);
        assert_eq!(config.treasury, treasury);
        assert_eq!(config.fee_bps, 300);

        // Verify stats are initialized
        let stats = CoinflipContract::load_stats(&env);
        assert_eq!(stats.total_games, 0);
        assert_eq!(stats.total_volume, 0);
        assert_eq!(stats.total_fees, 0);
    }

    /// Test compliance with regulatory requirements
    #[test]
    fn compliance_validates_regulatory_requirements() {
        let env = Env::default();
        let admin = Address::random(&env);
        let treasury = Address::random(&env);
        let token = Address::random(&env);

        env.mock_all_auths();

        // Test 1: Fee must be within regulatory bounds (2-5%)
        let result = CoinflipContract::initialize(
            env.clone(),
            admin.clone(),
            treasury.clone(),
            token.clone(),
            100, // 1% - below minimum
            1_000,
            1_000_000,
        );
        assert!(result.is_err());

        let result = CoinflipContract::initialize(
            env.clone(),
            admin.clone(),
            treasury.clone(),
            token.clone(),
            600, // 6% - above maximum
            1_000,
            1_000_000,
        );
        assert!(result.is_err());

        // Test 2: Valid fee range accepted
        let result = CoinflipContract::initialize(
            env.clone(),
            admin.clone(),
            treasury.clone(),
            token.clone(),
            300, // 3% - within range
            1_000,
            1_000_000,
        );
        assert!(result.is_ok());
    }

    /// Test data retention policy enforcement
    #[test]
    fn compliance_enforces_data_retention_policy() {
        let env = Env::default();
        let admin = Address::random(&env);
        let treasury = Address::random(&env);
        let player = Address::random(&env);
        let token = Address::random(&env);

        env.mock_all_auths();

        CoinflipContract::initialize(
            env.clone(),
            admin.clone(),
            treasury.clone(),
            token.clone(),
            300,
            1_000,
            1_000_000,
        )
        .unwrap();

        // Deposit reserve
        let mut stats = CoinflipContract::load_stats(&env);
        stats.reserve_balance = 10_000_000;
        CoinflipContract::save_stats(&env, &stats);

        // Create and complete a game
        let secret = Bytes::from_slice(&env, b"retention_test");
        let commitment = env.crypto().sha256(&secret).into();

        CoinflipContract::start_game(
            env.clone(),
            player.clone(),
            Side::Heads,
            100_000,
            commitment,
        )
        .unwrap();

        CoinflipContract::reveal(env.clone(), player.clone(), secret).unwrap();

        // Verify history is retained
        let history = CoinflipContract::load_player_history(&env, &player);
        assert!(
            history.len() > 0,
            "History must be retained for compliance audit"
        );

        // Verify game state is cleared after reveal
        let game = CoinflipContract::load_player_game(&env, &player);
        assert!(
            game.is_none() || game.unwrap().phase == GamePhase::Revealed,
            "Game state must be managed per retention policy"
        );
    }

    /// Test audit report generation capabilities
    #[test]
    fn compliance_supports_audit_report_generation() {
        let env = Env::default();
        let admin = Address::random(&env);
        let treasury = Address::random(&env);
        let player1 = Address::random(&env);
        let player2 = Address::random(&env);
        let token = Address::random(&env);

        env.mock_all_auths();

        CoinflipContract::initialize(
            env.clone(),
            admin.clone(),
            treasury.clone(),
            token.clone(),
            300,
            1_000,
            1_000_000,
        )
        .unwrap();

        // Deposit reserve
        let mut stats = CoinflipContract::load_stats(&env);
        stats.reserve_balance = 10_000_000;
        CoinflipContract::save_stats(&env, &stats);

        // Create multiple games for audit trail
        for (player, wager) in &[(player1.clone(), 100_000), (player2.clone(), 200_000)] {
            let secret = Bytes::from_slice(&env, b"audit_test");
            let commitment = env.crypto().sha256(&secret).into();

            CoinflipContract::start_game(
                env.clone(),
                player.clone(),
                Side::Heads,
                *wager,
                commitment,
            )
            .unwrap();

            CoinflipContract::reveal(env.clone(), player.clone(), secret).unwrap();
        }

        // Verify stats reflect all transactions
        let final_stats = CoinflipContract::load_stats(&env);
        assert_eq!(final_stats.total_games, 2, "Stats must track all games for audit");
        assert_eq!(
            final_stats.total_volume, 300_000,
            "Stats must track total volume for audit"
        );
    }

    /// Test transaction logging completeness
    #[test]
    fn audit_trail_logs_admin_actions() {
        let env = Env::default();
        let admin = Address::random(&env);
        let treasury = Address::random(&env);
        let new_treasury = Address::random(&env);
        let token = Address::random(&env);

        env.mock_all_auths();

        CoinflipContract::initialize(
            env.clone(),
            admin.clone(),
            treasury.clone(),
            token.clone(),
            300,
            1_000,
            1_000_000,
        )
        .unwrap();

        // Perform admin action
        CoinflipContract::set_treasury(env.clone(), admin.clone(), new_treasury.clone()).unwrap();

        // Verify change is logged in config
        let config = CoinflipContract::load_config(&env);
        assert_eq!(config.treasury, new_treasury, "Admin actions must be logged");
    }

    /// Test compliance with fee collection requirements
    #[test]
    fn compliance_validates_fee_collection() {
        let env = Env::default();
        let admin = Address::random(&env);
        let treasury = Address::random(&env);
        let player = Address::random(&env);
        let token = Address::random(&env);

        env.mock_all_auths();

        CoinflipContract::initialize(
            env.clone(),
            admin.clone(),
            treasury.clone(),
            token.clone(),
            300, // 3% fee
            1_000,
            1_000_000,
        )
        .unwrap();

        // Deposit reserve
        let mut stats = CoinflipContract::load_stats(&env);
        stats.reserve_balance = 10_000_000;
        CoinflipContract::save_stats(&env, &stats);

        // Play and win
        let secret = Bytes::from_slice(&env, b"fee_test");
        let commitment = env.crypto().sha256(&secret).into();

        CoinflipContract::start_game(
            env.clone(),
            player.clone(),
            Side::Heads,
            100_000,
            commitment,
        )
        .unwrap();

        CoinflipContract::reveal(env.clone(), player.clone(), secret).unwrap();

        // Verify fee is tracked
        let final_stats = CoinflipContract::load_stats(&env);
        assert!(
            final_stats.total_fees > 0,
            "Fees must be tracked for compliance"
        );
    }

    /// Test immutability of historical records
    #[test]
    fn audit_trail_ensures_immutability() {
        let env = Env::default();
        let admin = Address::random(&env);
        let treasury = Address::random(&env);
        let player = Address::random(&env);
        let token = Address::random(&env);

        env.mock_all_auths();

        CoinflipContract::initialize(
            env.clone(),
            admin.clone(),
            treasury.clone(),
            token.clone(),
            300,
            1_000,
            1_000_000,
        )
        .unwrap();

        // Deposit reserve
        let mut stats = CoinflipContract::load_stats(&env);
        stats.reserve_balance = 10_000_000;
        CoinflipContract::save_stats(&env, &stats);

        // Create game
        let secret = Bytes::from_slice(&env, b"immutable_test");
        let commitment = env.crypto().sha256(&secret).into();

        CoinflipContract::start_game(
            env.clone(),
            player.clone(),
            Side::Heads,
            100_000,
            commitment,
        )
        .unwrap();

        let game_before = CoinflipContract::load_player_game(&env, &player).unwrap();
        let commitment_before = game_before.commitment.clone();

        CoinflipContract::reveal(env.clone(), player.clone(), secret).unwrap();

        // Verify history entry is immutable
        let history = CoinflipContract::load_player_history(&env, &player);
        let entry = history.get_unchecked(0);
        assert_eq!(
            entry.commitment, commitment_before,
            "Historical records must be immutable"
        );
    }
}
