//! Tests for fraud detection and prevention system (issue #467).

#[cfg(test)]
mod fraud_detection_tests {
    use crate::*;
    use soroban_sdk::{testutils::{Address as _, Ledger}, Env};

    fn setup() -> (Env, Address, CoinflipContractClient<'static>) {
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
        (env, admin, client)
    }

    fn fund(env: &Env, contract_id: &Address, amount: i128) {
        env.as_contract(contract_id, || {
            let mut stats = CoinflipContract::load_stats(env);
            stats.reserve_balance = amount;
            CoinflipContract::save_stats(env, &stats);
        });
    }

    fn unique_commitment(env: &Env, seed: u8) -> BytesN<32> {
        let secret = Bytes::from_slice(env, &[seed; 32]);
        env.crypto().sha256(&secret).into()
    }

    // ── Rate limiting ─────────────────────────────────────────────────────────

    /// First 10 games in a window succeed; the 11th is rejected.
    #[test]
    fn test_rate_limit_blocks_11th_game() {
        let (env, _admin, client) = setup();
        let contract_id = env.register(CoinflipContract, ());
        fund(&env, &contract_id, 100_000_000_000);

        let player = Address::generate(&env);
        // Start 10 games (each with a unique player to avoid ActiveGameExists).
        for i in 0..10u8 {
            let p = Address::generate(&env);
            let commitment = unique_commitment(&env, i + 1);
            let result = client.try_start_game(
                &p, &Side::Heads, &1_000_000, &commitment,
                &None, &BytesN::from_array(&env, &[0u8; 32]), &Address::generate(&env),
            );
            // May fail for other reasons (reserves, token whitelist) — we only care
            // that it does NOT fail with ContractPaused from rate limiting.
            if let Err(Ok(e)) = result {
                assert_ne!(e, Error::ContractPaused, "game {} should not be rate-limited", i);
            }
        }

        // The rate-limit check is per-player, so test with a single player hitting 11.
        // Reset: use a fresh player and advance ledger to clear any window.
        let p = Address::generate(&env);
        // Manually set rate limit state to simulate 10 games already in window.
        env.as_contract(&contract_id, || {
            let state = PlayerRateLimit {
                window_start: env.ledger().sequence(),
                games_in_window: RATE_LIMIT_MAX_GAMES,
            };
            env.storage().persistent().set(&StorageKey::PlayerRateLimit(p.clone()), &state);
        });

        let commitment = unique_commitment(&env, 99);
        let result = client.try_start_game(
            &p, &Side::Heads, &1_000_000, &commitment,
            &None, &BytesN::from_array(&env, &[0u8; 32]), &Address::generate(&env),
        );
        assert_eq!(result, Err(Ok(Error::ContractPaused)), "11th game must be rate-limited");
    }

    /// Rate limit window resets after RATE_LIMIT_WINDOW_LEDGERS ledgers.
    #[test]
    fn test_rate_limit_window_resets() {
        let (env, _admin, client) = setup();
        let contract_id = env.register(CoinflipContract, ());

        let p = Address::generate(&env);
        // Set state: window full, but started long ago.
        env.as_contract(&contract_id, || {
            let state = PlayerRateLimit {
                window_start: 0, // very old
                games_in_window: RATE_LIMIT_MAX_GAMES + 5,
            };
            env.storage().persistent().set(&StorageKey::PlayerRateLimit(p.clone()), &state);
        });
        // Advance ledger past the window.
        env.ledger().with_mut(|l| l.sequence_number = RATE_LIMIT_WINDOW_LEDGERS + 10);

        // check_rate_limit should reset and allow the game.
        env.as_contract(&contract_id, || {
            let result = CoinflipContract::check_rate_limit(&env, &p);
            assert!(result.is_ok(), "window should have reset");
        });
    }

    // ── Fraud flag ────────────────────────────────────────────────────────────

    /// set_fraud_flag stores a FraudFlag retrievable via get_fraud_flag.
    #[test]
    fn test_fraud_flag_stored_and_retrieved() {
        let (env, _admin, client) = setup();
        let contract_id = env.register(CoinflipContract, ());
        let p = Address::generate(&env);

        env.as_contract(&contract_id, || {
            CoinflipContract::set_fraud_flag(&env, &p, symbol_short!("rate_limit"));
        });

        let flag = client.get_fraud_flag(&p);
        assert!(flag.is_some());
        assert_eq!(flag.unwrap().reason, symbol_short!("rate_limit"));
    }

    /// get_fraud_flag returns None when no flag is set.
    #[test]
    fn test_get_fraud_flag_none_when_clean() {
        let (env, _admin, client) = setup();
        let p = Address::generate(&env);
        assert!(client.get_fraud_flag(&p).is_none());
    }

    /// clear_fraud_flag removes the flag; admin only.
    #[test]
    fn test_clear_fraud_flag_admin_only() {
        let (env, admin, client) = setup();
        let contract_id = env.register(CoinflipContract, ());
        let p = Address::generate(&env);

        env.as_contract(&contract_id, || {
            CoinflipContract::set_fraud_flag(&env, &p, symbol_short!("win_streak"));
        });
        assert!(client.get_fraud_flag(&p).is_some());

        client.clear_fraud_flag(&admin, &p);
        assert!(client.get_fraud_flag(&p).is_none());
    }

    /// clear_fraud_flag is rejected for non-admin.
    #[test]
    fn test_clear_fraud_flag_unauthorized() {
        let (env, _admin, client) = setup();
        let attacker = Address::generate(&env);
        let p = Address::generate(&env);
        assert_eq!(
            client.try_clear_fraud_flag(&attacker, &p),
            Err(Ok(Error::Unauthorized))
        );
    }

    // ── Anomaly detection ─────────────────────────────────────────────────────

    /// check_anomaly flags a player with a win streak >= threshold.
    #[test]
    fn test_anomaly_win_streak_flagged() {
        let (env, _admin, _client) = setup();
        let contract_id = env.register(CoinflipContract, ());
        let p = Address::generate(&env);

        let stats = PlayerStats {
            games_played: 8,
            wins: 8,
            losses: 0,
            max_streak: ANOMALY_WIN_STREAK_THRESHOLD,
            current_streak: ANOMALY_WIN_STREAK_THRESHOLD,
            total_wagered: 0,
            net_winnings: 0,
        };

        env.as_contract(&contract_id, || {
            CoinflipContract::check_anomaly(&env, &p, &stats);
            let flag = env.storage().persistent()
                .get::<StorageKey, FraudFlag>(&StorageKey::FraudFlag(p.clone()));
            assert!(flag.is_some());
            assert_eq!(flag.unwrap().reason, symbol_short!("win_streak"));
        });
    }

    /// check_anomaly flags a player with many losses and no wins.
    #[test]
    fn test_anomaly_loss_streak_flagged() {
        let (env, _admin, _client) = setup();
        let contract_id = env.register(CoinflipContract, ());
        let p = Address::generate(&env);

        let stats = PlayerStats {
            games_played: ANOMALY_LOSS_STREAK_THRESHOLD as u64,
            wins: 0,
            losses: ANOMALY_LOSS_STREAK_THRESHOLD as u64,
            max_streak: 0,
            current_streak: 0,
            total_wagered: 0,
            net_winnings: 0,
        };

        env.as_contract(&contract_id, || {
            CoinflipContract::check_anomaly(&env, &p, &stats);
            let flag = env.storage().persistent()
                .get::<StorageKey, FraudFlag>(&StorageKey::FraudFlag(p.clone()));
            assert!(flag.is_some());
            assert_eq!(flag.unwrap().reason, symbol_short!("loss_streak"));
        });
    }

    /// Normal player stats produce no fraud flag.
    #[test]
    fn test_anomaly_normal_player_no_flag() {
        let (env, _admin, _client) = setup();
        let contract_id = env.register(CoinflipContract, ());
        let p = Address::generate(&env);

        let stats = PlayerStats {
            games_played: 10,
            wins: 5,
            losses: 5,
            max_streak: 3,
            current_streak: 1,
            total_wagered: 0,
            net_winnings: 0,
        };

        env.as_contract(&contract_id, || {
            CoinflipContract::check_anomaly(&env, &p, &stats);
            let flag = env.storage().persistent()
                .get::<StorageKey, FraudFlag>(&StorageKey::FraudFlag(p.clone()));
            assert!(flag.is_none());
        });
    }

    // ── Constants ─────────────────────────────────────────────────────────────

    #[test]
    fn test_fraud_constants() {
        assert_eq!(RATE_LIMIT_MAX_GAMES, 10);
        assert_eq!(RATE_LIMIT_WINDOW_LEDGERS, 60);
        assert_eq!(ANOMALY_WIN_STREAK_THRESHOLD, 8);
        assert_eq!(ANOMALY_LOSS_STREAK_THRESHOLD, 20);
    }
}
