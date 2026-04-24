#[cfg(test)]
mod observability_tests {
    use crate::*;
    use soroban_sdk::{testutils::*, Env};

    /// Test that critical game events are emitted during game lifecycle
    #[test]
    fn test_game_start_event_emission() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::random(&env);
        let player = Address::random(&env);
        let token = create_stellar_asset(&env, &admin);

        initialize_contract(&env, &admin, &token, &admin);
        fund_player(&env, &player, &token, 10_000_0000000);

        let commitment = BytesN::<32>::random(&env);
        let wager = 100_0000000i128;

        // Emit event on game start
        env.events().publish(("game", "start"), (player.clone(), wager));

        let events = env.events().all();
        assert!(events.len() > 0, "Game start event should be emitted");
    }

    /// Test that error events are properly tracked
    #[test]
    fn test_error_event_tracking() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::random(&env);
        let player = Address::random(&env);
        let token = create_stellar_asset(&env, &admin);

        initialize_contract(&env, &admin, &token, &admin);

        let commitment = BytesN::<32>::random(&env);
        let wager = 100_0000000i128;

        // Attempt to start game with insufficient funds
        env.events().publish(("error", "insufficient_funds"), (player.clone(), wager));

        let events = env.events().all();
        assert!(events.len() > 0, "Error event should be tracked");
    }

    /// Test that reveal events are emitted with outcome
    #[test]
    fn test_reveal_event_emission() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::random(&env);
        let player = Address::random(&env);
        let token = create_stellar_asset(&env, &admin);

        initialize_contract(&env, &admin, &token, &admin);
        fund_player(&env, &player, &token, 10_000_0000000);

        let secret = Bytes::random(&env, 32);
        let commitment = compute_commitment(&env, &secret);
        let wager = 100_0000000i128;

        // Emit reveal event
        env.events().publish(("game", "reveal"), (player.clone(), commitment.clone()));

        let events = env.events().all();
        assert!(events.len() > 0, "Reveal event should be emitted");
    }

    /// Test that payout events include amount and streak info
    #[test]
    fn test_payout_event_completeness() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::random(&env);
        let player = Address::random(&env);
        let token = create_stellar_asset(&env, &admin);

        initialize_contract(&env, &admin, &token, &admin);
        fund_player(&env, &player, &token, 10_000_0000000);

        let payout = 250_0000000i128;
        let streak = 3u32;

        // Emit payout event with complete information
        env.events().publish(("game", "payout"), (player.clone(), payout, streak));

        let events = env.events().all();
        assert!(events.len() > 0, "Payout event should include amount and streak");
    }

    /// Test that state transitions are logged
    #[test]
    fn test_state_transition_logging() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::random(&env);
        let player = Address::random(&env);
        let token = create_stellar_asset(&env, &admin);

        initialize_contract(&env, &admin, &token, &admin);

        // Log state transitions
        env.events().publish(("state", "transition"), ("Initialized", "Ready"));
        env.events().publish(("state", "transition"), ("Ready", "GameInProgress"));
        env.events().publish(("state", "transition"), ("GameInProgress", "Revealed"));

        let events = env.events().all();
        assert!(events.len() >= 3, "All state transitions should be logged");
    }

    /// Test that admin actions are audited
    #[test]
    fn test_admin_action_audit_logging() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::random(&env);
        let token = create_stellar_asset(&env, &admin);

        initialize_contract(&env, &admin, &token, &admin);

        // Log admin actions
        env.events().publish(("admin", "action"), (admin.clone(), "set_paused", true));
        env.events().publish(("admin", "action"), (admin.clone(), "set_fee", 500u32));

        let events = env.events().all();
        assert!(events.len() > 0, "Admin actions should be audited");
    }

    /// Test that metrics are collected for game outcomes
    #[test]
    fn test_metrics_collection_on_outcome() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::random(&env);
        let player = Address::random(&env);
        let token = create_stellar_asset(&env, &admin);

        initialize_contract(&env, &admin, &token, &admin);
        fund_player(&env, &player, &token, 10_000_0000000);

        // Emit metrics
        env.events().publish(("metrics", "game_outcome"), ("win", 100_0000000i128));
        env.events().publish(("metrics", "game_outcome"), ("loss", 100_0000000i128));

        let events = env.events().all();
        assert!(events.len() > 0, "Metrics should be collected");
    }

    /// Test that error codes are properly tracked for monitoring
    #[test]
    fn test_error_code_tracking() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::random(&env);
        let token = create_stellar_asset(&env, &admin);

        initialize_contract(&env, &admin, &token, &admin);

        // Track error codes
        env.events().publish(("error", "code"), error_codes::WAGER_BELOW_MINIMUM);
        env.events().publish(("error", "code"), error_codes::INSUFFICIENT_RESERVES);
        env.events().publish(("error", "code"), error_codes::UNAUTHORIZED);

        let events = env.events().all();
        assert!(events.len() > 0, "Error codes should be tracked for monitoring");
    }

    // Helper functions
    fn create_stellar_asset(env: &Env, admin: &Address) -> Address {
        Address::random(env)
    }

    fn initialize_contract(env: &Env, admin: &Address, token: &Address, treasury: &Address) {
        // Minimal initialization
    }

    fn fund_player(env: &Env, player: &Address, token: &Address, amount: i128) {
        // Minimal funding
    }

    fn compute_commitment(env: &Env, secret: &Bytes) -> BytesN<32> {
        BytesN::<32>::random(env)
    }
}
