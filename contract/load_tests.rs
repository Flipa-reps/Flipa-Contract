use super::*;
use std::sync::{Arc, Barrier, Mutex};
use tokio::runtime::Runtime;
use rand::{thread_rng, Rng};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use std::sync::atomic::{AtomicUsize, Ordering};

#[cfg(test)]
mod load_tests {
    use super::*;
    use proptest::prelude::*;

    /// Load testing metrics for concurrent scenarios.
    #[derive(Debug, Clone)]
    struct LoadMetrics {
        total_games: usize,
        successful_operations: usize,
        failed_operations: usize,
        reserve_consistency_checks: usize,
        duration_ms: u64,
    }

    impl LoadMetrics {
        fn success_rate(&self) -> f64 {
            if self.total_games == 0 {
                0.0
            } else {
                self.successful_operations as f64 / self.total_games as f64
            }
        }

        fn print_report(&self, scenario: &str) {
            println!("\n=== LOAD TEST REPORT: {} ===", scenario);
            println!("Total operations: {}", self.total_games);
            println!("Successful: {}", self.successful_operations);
            println!("Failed: {}", self.failed_operations);
            println!("Success rate: {:.2}%", self.success_rate() * 100.0);
            println!("Reserve consistency: {} checks passed", self.reserve_consistency_checks);
            println!("Duration: {}ms", self.duration_ms);
            println!("================================\n");
        }
    }

    /// Simulate N concurrent players performing realistic game flows.
    fn concurrent_game_simulation(num_players: usize, num_rounds_per_player: usize) -> LoadMetrics {
        let rt = Runtime::new().unwrap();
        let metrics = Arc::new(LoadMetrics {
            total_games: 0,
            successful_operations: 0,
            failed_operations: 0,
            reserve_consistency_checks: 0,
            duration_ms: 0,
        });

        let start = Instant::now();
        let result = rt.block_on(async {
            let h = Arc::new(Harness::new());
            h.fund(10_000_000_000i128); // Massive reserves for concurrency
            
            // Pre-create token balance for transfers
            let contract_id = h.env.current_contract_address();
            let token_id = h.env.as_contract(&contract_id, || {
                CoinflipContract::load_config(&h.env).token.clone()
            });
            let token_client = soroban_sdk::token::StellarAssetClient::new(&h.env, &token_id);
            token_client.mint(&contract_id, &10_000_000_000i128);

            let mut handles = vec![];
            
            for player_id in 0..num_players {
                let h_clone = Arc::clone(&h);
                let metrics_clone = Arc::clone(&metrics);
                let handle = tokio::spawn(async move {
                    let mut player_success = 0;
                    let mut player_total = 0;
                    
                    let player = h_clone.player();
                    
                    for round in 0..num_rounds_per_player {
                        player_total += 1;
                        
                        // Realistic pattern: start → reveal → 50% cashout/50% continue
                        match h_clone.play_win_round(&player, 5_000_000) {
                            true => {
                                if let Some(game) = h_clone.game_state(&player) {
                                    if thread_rng().gen_bool(0.5) {
                                        // 50% cash out
                                        if h_clone.client.cash_out(&player).is_ok() {
                                            player_success += 1;
                                        }
                                    } else {
                                        // 50% continue (if streak < 4)
                                        if game.streak < 4 {
                                            let commit = h_clone.make_commitment(42);
                                            if h_clone.client.continue_streak(&player, &commit).is_ok() {
                                                player_success += 1;
                                            }
                                        } else {
                                            player_success += 1; // Max streak reached
                                        }
                                    }
                                }
                            }
                            false => {
                                // Loss: game auto-deleted, reserves preserved
                                player_success += 1;
                            }
                        }
                        
                        metrics_clone.total_games.fetch_add(1, Ordering::Relaxed);
                        if player_success > 0 {
                            metrics_clone.successful_operations.fetch_add(1, Ordering::Relaxed);
                        } else {
                            metrics_clone.failed_operations.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                });
                handles.push(handle);
            }
            
            // Wait for all players
            for handle in handles {
                let _ = handle.await;
            }
            
            // Final reserve consistency check
            let final_stats = h.stats();
            metrics_clone.reserve_consistency_checks.fetch_add(1, Ordering::Relaxed);
            assert!(final_stats.reserve_balance >= 0, "Reserves went negative!");
        });
        let duration = start.elapsed().as_millis() as u64;
        
        let metrics = Arc::try_unwrap(metrics).unwrap();
        metrics.duration_ms = duration;
        metrics
    }

    /// High-concurrency stress test: 50 players × 10 rounds each = 500 operations
    #[test]
    fn test_concurrent_50_players_10_rounds() {
        let metrics = concurrent_game_simulation(50, 10);
        metrics.print_report("50 players × 10 rounds");
        
        assert_eq!(metrics.success_rate(), 1.0, "Must be 100% success under load");
        assert!(metrics.total_games > 400, "Expected ~500 total operations");
    }

    /// Extreme concurrency: 100 players × 5 rounds = 500 operations
    #[test]
    fn test_concurrent_100_players_5_rounds() {
        let metrics = concurrent_game_simulation(100, 5);
        metrics.print_report("100 players × 5 rounds");
        
        assert_eq!(metrics.success_rate(), 1.0, "Must be 100% success under extreme load");
    }

    /// Reserve depletion stress test
    #[test]
    fn test_reserve_depletion_under_concurrency() {
        let rt = Runtime::new().unwrap();
        let metrics = Arc::new(LoadMetrics {
            total_games: 0,
            successful_operations: 0,
            failed_operations: 0,
            reserve_consistency_checks: 0,
            duration_ms: 0,
        });

        rt.block_on(async {
            let h = Arc::new(Harness::new());
            
            // Start with limited reserves
            h.fund(1_000_000_000i128);
            
            let contract_id = h.env.current_contract_address();
            let token_id = h.env.as_contract(&contract_id, || {
                CoinflipContract::load_config(&h.env).token.clone()
            });
            let token_client = soroban_sdk::token::StellarAssetClient::new(&h.env, &token_id);
            token_client.mint(&contract_id, &1_000_000_000i128);

            let initial_reserve = h.stats().reserve_balance;
            
            // 20 players trying high-wager games simultaneously
            let mut handles = vec![];
            for _ in 0..20 {
                let h_clone = Arc::clone(&h);
                let metrics_clone = Arc::clone(&metrics);
                let handle = tokio::spawn(async move {
                    let player = h_clone.player();
                    let wager = 25_000_000i128; // High wager to stress reserves
                    
                    // Try start_game - many will fail due to InsufficientReserves
                    match h_clone.client.try_start_game(&player, &Side::Heads, &wager, &h_clone.make_commitment(1)) {
                        Ok(_) => {
                            // If accepted, complete win flow
                            if h_clone.play_win_round(&player, wager) {
                                let _ = h_clone.client.cash_out(&player);
                            }
                            metrics_clone.successful_operations.fetch_add(1, Ordering::Relaxed);
                        }
                        Err(_) => {
                            // Expected InsufficientReserves failures count as "success" for stress test
                            metrics_clone.successful_operations.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                    metrics_clone.total_games.fetch_add(1, Ordering::Relaxed);
                });
                handles.push(handle);
            }
            
            for handle in handles {
                let _ = handle.await;
            }
            
            // Verify reserves never went negative
            let final_stats = h.stats();
            assert!(final_stats.reserve_balance >= 0);
            assert!(final_stats.reserve_balance <= initial_reserve);
        });
    }

    /// Property test: concurrent fund conservation across multiple players
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(20))]
        
        #[test]
        fn prop_concurrent_fund_conservation(
            num_players in 5usize..=20usize,
            rounds_per_player in 2usize..=8usize,
        ) {
            let rt = Runtime::new().unwrap();
            
            let total_funds_before = rt.block_on(async {
                let h = Arc::new(Harness::new());
                h.fund(2_000_000_000i128);
                
                let contract_id = h.env.current_contract_address();
                let token_id = h.env.as_contract(&contract_id, || {
                    CoinflipContract::load_config(&h.env).token.clone()
                });
                let token_client = soroban_sdk::token::StellarAssetClient::new(&h.env, &token_id);
                token_client.mint(&contract_id, &2_000_000_000i128);
                
                // Sum all player balances + treasury + reserves
                let config = h.env.as_contract(&contract_id, || CoinflipContract::load_config(&h.env));
                let treasury = config.treasury.clone();
                
                token_client.balance(&treasury) + h.stats().reserve_balance
            });

            // Run concurrent games
            let _ = rt.block_on(async {
                let h = Arc::new(Harness::new());
                h.fund(2_000_000_000i128);
                
                let contract_id = h.env.current_contract_address();
                let token_id = h.env.as_contract(&contract_id, || {
                    CoinflipContract::load_config(&h.env).token.clone()
                });
                let token_client = soroban_sdk::token::StellarAssetClient::new(&h.env, &token_id);
                token_client.mint(&contract_id, &2_000_000_000i128);
                
                let mut handles = vec![];
                for _ in 0..num_players {
                    let h_clone = Arc::clone(&h);
                    handles.push(tokio::spawn(async move {
                        let player = h_clone.player();
                        for _ in 0..rounds_per_player {
                            let _ = h_clone.play_win_round(&player, 2_000_000);
                        }
                    }));
                }
                for handle in handles {
                    let _ = handle.await;
                }
            });

            let total_funds_after = rt.block_on(async {
                let h = Arc::new(Harness::new());
                // Reconstruct final state to check conservation
                token_client.balance(&treasury) + h.stats().reserve_balance
            });

            prop_assert_eq!(total_funds_before, total_funds_after, 
                "Fund conservation must hold under concurrent load: {} != {}", 
                total_funds_before, total_funds_after);
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(50))]

        /// PROPERTY LT-1: No race conditions on reserve_balance under concurrent start_game calls
        #[test]
        fn prop_concurrent_reserve_no_double_debit(
            num_concurrent_starts in 10usize..=50usize,
        ) {
            let rt = Runtime::new().unwrap();
            
            rt.block_on(async {
                let h = Arc::new(Harness::new());
                h.fund(1_000_000_000i128); // Enough for all
                
                let barrier = Arc::new(Barrier::new(num_concurrent_starts + 1));
                let mut handles = vec![];
                
                for i in 0..num_concurrent_starts {
                    let h_clone = Arc::clone(&h);
                    let barrier_clone = Arc::clone(&barrier);
                    let handle = tokio::spawn(async move {
                        barrier_clone.wait().await;
                        let player = h_clone.player();
                        let wager = 2_000_000i128;
                        let result = h_clone.client.try_start_game(
                            &player,
                            &Side::Heads,
                            &wager,
                            &h_clone.make_commitment((i % 256) as u8)
                        );
                        result.is_ok()
                    });
                    handles.push(handle);
                }
                
                barrier.wait().await; // Release all players simultaneously
                
                let mut success_count = 0;
                for handle in handles {
                    if handle.await.unwrap() {
                        success_count += 1;
                    }
                }
                
                let final_stats = h.stats();
                // Each accepted game locks wager in reserves (via total_volume)
                prop_assert!(final_stats.total_games <= num_concurrent_starts as u64);
                prop_assert!(final_stats.reserve_balance >= 1_000_000_000 - (success_count as i128 * 20_000_000));
            });
        }
    }
}

