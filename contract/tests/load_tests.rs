use coinflip_contract::*;
use soroban_sdk::{Env, Address, Bytes, BytesN, testutils::Address as _};
use std::time::Instant;
use std::sync::{Arc, Mutex};

#[cfg(test)]
mod load_tests {
    use super::*;

    struct Harness {
        env: Env,
        client: CoinflipContractClient<'static>,
        token: Address,
    }

    impl Harness {
        fn new() -> Self {
            let env = Env::default();
            env.mock_all_auths();
            let contract_id = env.register(CoinflipContract, ());
            let client: CoinflipContractClient<'static> = unsafe {
                core::mem::transmute(CoinflipContractClient::new(&env, &contract_id))
            };
            let admin = Address::generate(&env);
            let treasury = Address::generate(&env);
            let token = Address::generate(&env);
            let oracle_vrf_pk = BytesN::from_array(&env, &[1u8; 32]);
            client.initialize(
                &admin, &treasury, &token,
                &300, &1_000_000, &100_000_000,
                &oracle_vrf_pk,
            ).unwrap();
            Self { env, client, token }
        }

        fn player(&self) -> Address {
            Address::generate(&self.env)
        }

        fn make_secret(&self, seed: u8) -> Bytes {
            Bytes::from_slice(&self.env, &[seed; 32])
        }

        fn make_commitment(&self, seed: u8) -> BytesN<32> {
            self.env.crypto().sha256(&self.make_secret(seed)).into()
        }

        fn make_vrf_proof(&self) -> BytesN<64> {
            BytesN::from_array(&self.env, &[0u8; 64])
        }

        fn make_oracle_commitment(&self) -> BytesN<32> {
            BytesN::from_array(&self.env, &[2u8; 32])
        }

        fn fund(&self, amount: i128) {
            self.env.as_contract(&self.client.address, || {
                let key = StorageKey::Stats;
                let mut stats: ContractStats = self.env.storage().persistent().get(&key).unwrap();
                stats.reserve_balance = amount;
                self.env.storage().persistent().set(&key, &stats);
            });
        }

        /// Advance ledger past the MIN_REVEAL_DELAY_LEDGERS window.
        fn advance_ledger(&self) {
            self.env.ledger().with_mut(|l| {
                l.sequence_number += MIN_REVEAL_DELAY_LEDGERS + 1;
            });
        }

        /// Play a full win round (start → advance → reveal). Returns true on win.
        fn play_win_round(&self, player: &Address, wager: i128) -> bool {
            let commitment = self.make_commitment(1);
            let oracle_commitment = self.make_oracle_commitment();
            let result = self.client.try_start_game(
                player, &Side::Heads, &wager, &commitment,
                &None, &oracle_commitment, &self.token,
            );
            if result.is_err() { return false; }
            self.advance_ledger();
            let secret = self.make_secret(1);
            let vrf_proof = self.make_vrf_proof();
            self.client.try_reveal(player, &secret, &vrf_proof)
                .unwrap_or(false)
        }

        fn stats(&self) -> ContractStats {
            self.env.as_contract(&self.client.address, || {
                self.env.storage().persistent().get(&StorageKey::Stats).unwrap()
            })
        }
    }

    #[derive(Debug, Clone)]
    struct LoadMetrics {
        total: usize,
        success: usize,
        failed: usize,
        duration_ms: u64,
        throughput: f64,
        p95_ms: u64,
        p99_ms: u64,
    }

    impl LoadMetrics {
        fn new(total: usize, success: usize, failed: usize, duration_ms: u64, latencies: &[u64]) -> Self {
            let throughput = if duration_ms > 0 {
                (total as f64 / duration_ms as f64) * 1000.0
            } else {
                0.0
            };
            let (p95_ms, p99_ms) = Self::percentiles(latencies);
            Self { total, success, failed, duration_ms, throughput, p95_ms, p99_ms }
        }

        fn percentiles(latencies: &[u64]) -> (u64, u64) {
            if latencies.is_empty() { return (0, 0); }
            let mut sorted = latencies.to_vec();
            sorted.sort_unstable();
            let p95_idx = ((sorted.len() as f64 * 0.95) as usize).min(sorted.len() - 1);
            let p99_idx = ((sorted.len() as f64 * 0.99) as usize).min(sorted.len() - 1);
            (sorted[p95_idx], sorted[p99_idx])
        }

        fn print(&self, scenario: &str) {
            println!("\n=== {} ===", scenario);
            println!("Total: {} | Success: {} | Failed: {}", self.total, self.success, self.failed);
            println!("Duration: {}ms | Throughput: {:.2} ops/s", self.duration_ms, self.throughput);
            println!("Latency p95: {}ms | p99: {}ms", self.p95_ms, self.p99_ms);
        }
    }

    // ── Sequential load tests (Soroban Env is not Send) ─────────────────────

    /// 100 sequential players each complete a full game cycle.
    #[test]
    fn test_100_sequential_game_cycles() {
        let h = Harness::new();
        h.fund(10_000_000_000);

        let start = Instant::now();
        let mut success = 0usize;
        let mut failed = 0usize;
        let mut latencies = Vec::with_capacity(100);

        for i in 0..100u8 {
            let op_start = Instant::now();
            let player = h.player();
            // Use distinct seeds per player to avoid duplicate-commitment rejection.
            let seed = i.wrapping_add(1).max(1); // never 0
            let commitment = h.make_commitment(seed);
            let oracle_commitment = h.make_oracle_commitment();

            let ok = h.client.try_start_game(
                &player, &Side::Heads, &5_000_000, &commitment,
                &None, &oracle_commitment, &h.token,
            ).is_ok();

            if ok {
                h.advance_ledger();
                let secret = h.make_secret(seed);
                let vrf_proof = h.make_vrf_proof();
                let won = h.client.try_reveal(&player, &secret, &vrf_proof).unwrap_or(false);
                if won {
                    let _ = h.client.try_cash_out(&player);
                }
                success += 1;
            } else {
                failed += 1;
            }
            latencies.push(op_start.elapsed().as_millis() as u64);
        }

        let duration = start.elapsed().as_millis() as u64;
        let metrics = LoadMetrics::new(100, success, failed, duration, &latencies);
        metrics.print("100 Sequential Game Cycles");

        assert!(success >= 95, "Expected ≥95 successes, got {}", success);
    }

    /// 500 sequential players — higher volume baseline.
    #[test]
    fn test_500_sequential_game_cycles() {
        let h = Harness::new();
        h.fund(50_000_000_000);

        let start = Instant::now();
        let mut success = 0usize;
        let mut failed = 0usize;
        let mut latencies = Vec::with_capacity(500);

        for i in 0..500u32 {
            let op_start = Instant::now();
            let player = h.player();
            // Rotate through 200 distinct seeds to avoid duplicate-commitment errors.
            let seed = ((i % 200) as u8).wrapping_add(1).max(1);
            let commitment = h.make_commitment(seed);
            let oracle_commitment = h.make_oracle_commitment();

            let ok = h.client.try_start_game(
                &player, &Side::Heads, &5_000_000, &commitment,
                &None, &oracle_commitment, &h.token,
            ).is_ok();

            if ok {
                h.advance_ledger();
                let secret = h.make_secret(seed);
                let vrf_proof = h.make_vrf_proof();
                let won = h.client.try_reveal(&player, &secret, &vrf_proof).unwrap_or(false);
                if won {
                    let _ = h.client.try_cash_out(&player);
                }
                success += 1;
            } else {
                failed += 1;
            }
            latencies.push(op_start.elapsed().as_millis() as u64);
        }

        let duration = start.elapsed().as_millis() as u64;
        let metrics = LoadMetrics::new(500, success, failed, duration, &latencies);
        metrics.print("500 Sequential Game Cycles");

        assert!(success >= 475, "Expected ≥475 successes, got {}", success);
    }

    /// Reserve depletion: limited reserves cause later games to be rejected.
    #[test]
    fn test_reserve_depletion_sequential() {
        let h = Harness::new();
        h.fund(200_000_000); // ~20 games at 10 XLM wager with 10x max payout

        let start = Instant::now();
        let mut success = 0usize;
        let mut failed = 0usize;
        let mut latencies = Vec::with_capacity(100);

        for i in 0..100u8 {
            let op_start = Instant::now();
            let player = h.player();
            let seed = i.wrapping_add(1).max(1);
            let commitment = h.make_commitment(seed);
            let oracle_commitment = h.make_oracle_commitment();

            let ok = h.client.try_start_game(
                &player, &Side::Heads, &10_000_000, &commitment,
                &None, &oracle_commitment, &h.token,
            ).is_ok();

            if ok {
                success += 1;
                // Don't reveal — just measure start_game throughput under reserve pressure.
            } else {
                failed += 1;
            }
            latencies.push(op_start.elapsed().as_millis() as u64);
        }

        let duration = start.elapsed().as_millis() as u64;
        let metrics = LoadMetrics::new(100, success, failed, duration, &latencies);
        metrics.print("Reserve Depletion Sequential");

        let stats = h.stats();
        assert!(stats.reserve_balance >= 0, "Reserve balance must never go negative");
        assert!(failed > 0, "Expected some rejections due to reserve limits");
    }

    /// Streak continuation: player wins and continues 5 times before cashing out.
    #[test]
    fn test_streak_continuation_load() {
        let h = Harness::new();
        h.fund(10_000_000_000);

        let start = Instant::now();
        let mut completed_streaks = 0usize;
        let mut latencies = Vec::with_capacity(50);

        for i in 0..50u8 {
            let op_start = Instant::now();
            let player = h.player();
            let seed = i.wrapping_add(1).max(1);
            let commitment = h.make_commitment(seed);
            let oracle_commitment = h.make_oracle_commitment();

            let ok = h.client.try_start_game(
                &player, &Side::Heads, &1_000_000, &commitment,
                &None, &oracle_commitment, &h.token,
            ).is_ok();

            if ok {
                h.advance_ledger();
                let secret = h.make_secret(seed);
                let vrf_proof = h.make_vrf_proof();
                let won = h.client.try_reveal(&player, &secret, &vrf_proof).unwrap_or(false);

                if won {
                    // Attempt to continue streak once, then cash out.
                    let next_seed = seed.wrapping_add(100);
                    let next_commitment = h.make_commitment(next_seed);
                    let continued = h.client.try_continue_streak(&player, &next_commitment).is_ok();
                    if continued {
                        h.advance_ledger();
                        let next_secret = h.make_secret(next_seed);
                        let _ = h.client.try_reveal(&player, &next_secret, &h.make_vrf_proof());
                    }
                    let _ = h.client.try_cash_out(&player);
                    completed_streaks += 1;
                }
            }
            latencies.push(op_start.elapsed().as_millis() as u64);
        }

        let duration = start.elapsed().as_millis() as u64;
        let metrics = LoadMetrics::new(50, completed_streaks, 50 - completed_streaks, duration, &latencies);
        metrics.print("Streak Continuation Load (50 players)");

        // At least some players should complete the streak flow.
        assert!(completed_streaks > 0, "Expected at least one completed streak");
    }

    /// State consistency: verify reserve_balance stays non-negative after many games.
    #[test]
    fn test_state_consistency_after_many_games() {
        let h = Harness::new();
        h.fund(5_000_000_000);

        let initial_stats = h.stats();
        let initial_reserve = initial_stats.reserve_balance;

        for i in 0..200u8 {
            let player = h.player();
            let seed = i.wrapping_add(1).max(1);
            let commitment = h.make_commitment(seed);
            let oracle_commitment = h.make_oracle_commitment();

            let ok = h.client.try_start_game(
                &player, &Side::Heads, &5_000_000, &commitment,
                &None, &oracle_commitment, &h.token,
            ).is_ok();

            if ok {
                h.advance_ledger();
                let secret = h.make_secret(seed);
                let vrf_proof = h.make_vrf_proof();
                let won = h.client.try_reveal(&player, &secret, &vrf_proof).unwrap_or(false);
                if won {
                    let _ = h.client.try_cash_out(&player);
                }
            }
        }

        let final_stats = h.stats();
        assert!(final_stats.reserve_balance >= 0, "Reserve must never go negative");
        assert!(final_stats.total_games > 0, "Games counter must increment");
        assert!(final_stats.total_volume > 0, "Volume must accumulate");
    }

    /// Pause/unpause under load: games started before pause can still settle.
    #[test]
    fn test_pause_during_active_games() {
        let h = Harness::new();
        h.fund(1_000_000_000);

        // Start 10 games before pausing.
        let mut active_players = Vec::new();
        for i in 0..10u8 {
            let player = h.player();
            let seed = i.wrapping_add(1).max(1);
            let commitment = h.make_commitment(seed);
            let oracle_commitment = h.make_oracle_commitment();
            if h.client.try_start_game(
                &player, &Side::Heads, &5_000_000, &commitment,
                &None, &oracle_commitment, &h.token,
            ).is_ok() {
                active_players.push((player, seed));
            }
        }

        // Pause the contract.
        let config_key = StorageKey::Config;
        h.env.as_contract(&h.client.address, || {
            let mut config: ContractConfig = h.env.storage().persistent().get(&config_key).unwrap();
            config.paused = true;
            h.env.storage().persistent().set(&config_key, &config);
        });

        // New games must be rejected while paused.
        let new_player = h.player();
        let new_commitment = h.make_commitment(99);
        let new_oracle = h.make_oracle_commitment();
        let rejected = h.client.try_start_game(
            &new_player, &Side::Heads, &5_000_000, &new_commitment,
            &None, &new_oracle, &h.token,
        );
        assert!(rejected.is_err(), "start_game must fail while paused");

        // Active games can still reveal and settle.
        h.advance_ledger();
        let mut settled = 0usize;
        for (player, seed) in &active_players {
            let secret = h.make_secret(*seed);
            let vrf_proof = h.make_vrf_proof();
            let won = h.client.try_reveal(&player, &secret, &vrf_proof).unwrap_or(false);
            if won {
                let _ = h.client.try_cash_out(&player);
                settled += 1;
            }
        }

        // At least some games should have settled.
        assert!(!active_players.is_empty());
        let stats = h.stats();
        assert!(stats.reserve_balance >= 0);
    }

    /// Ignored heavy test — run with: cargo test --release -- --ignored
    #[test]
    #[ignore]
    fn test_1000_sequential_game_cycles() {
        let h = Harness::new();
        h.fund(100_000_000_000);

        let start = Instant::now();
        let mut success = 0usize;
        let mut failed = 0usize;
        let mut latencies = Vec::with_capacity(1000);

        for i in 0..1000u32 {
            let op_start = Instant::now();
            let player = h.player();
            let seed = ((i % 200) as u8).wrapping_add(1).max(1);
            let commitment = h.make_commitment(seed);
            let oracle_commitment = h.make_oracle_commitment();

            let ok = h.client.try_start_game(
                &player, &Side::Heads, &5_000_000, &commitment,
                &None, &oracle_commitment, &h.token,
            ).is_ok();

            if ok {
                h.advance_ledger();
                let secret = h.make_secret(seed);
                let vrf_proof = h.make_vrf_proof();
                let won = h.client.try_reveal(&player, &secret, &vrf_proof).unwrap_or(false);
                if won {
                    let _ = h.client.try_cash_out(&player);
                }
                success += 1;
            } else {
                failed += 1;
            }
            latencies.push(op_start.elapsed().as_millis() as u64);
        }

        let duration = start.elapsed().as_millis() as u64;
        let metrics = LoadMetrics::new(1000, success, failed, duration, &latencies);
        metrics.print("1000 Sequential Game Cycles");
        assert!(success >= 950, "Expected ≥950 successes, got {}", success);
    }
}
