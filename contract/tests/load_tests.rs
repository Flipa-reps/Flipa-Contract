use coinflip_contract::*;
use soroban_sdk::{Env, Address, Bytes, BytesN, testutils::Address as _};
use std::time::Instant;
use std::sync::{Arc, Mutex};

#[cfg(test)]
mod load_tests {
    use super::*;

    // Test harness for load testing
    struct Harness {
        env: Env,
        client: CoinflipContractClient<'static>,
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
            client.initialize(&admin, &treasury, &token, &300, &1_000_000, &100_000_000);
            Self { env, client }
        }

        fn player(&self) -> Address {
            Address::generate(&self.env)
        }

        fn make_commitment(&self, seed: u8) -> BytesN<32> {
            let secret = Bytes::from_slice(&self.env, &[seed; 32]);
            self.env.crypto().sha256(&secret).into()
        }

        fn make_secret(&self, seed: u8) -> Bytes {
            Bytes::from_slice(&self.env, &[seed; 32])
        }

        fn fund(&self, amount: i128) {
            self.env.as_contract(&self.client.address, || {
                let key = StorageKey::Stats;
                let mut stats: ContractStats = self.env.storage().persistent().get(&key).unwrap();
                stats.reserve_balance = amount;
                self.env.storage().persistent().set(&key, &stats);
            });
        }

        fn play_win_round(&self, player: &Address, wager: i128) -> bool {
            let commitment = self.make_commitment(1);
            self.client.start_game(player, &Side::Heads, &wager, &commitment);
            let secret = self.make_secret(1);
            self.client.reveal(player, &secret)
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
            let throughput = (total as f64 / duration_ms as f64) * 1000.0;
            let (p95, p99) = Self::percentiles(latencies);
            Self { total, success, failed, duration_ms, throughput, p95_ms: p95, p99_ms: p99 }
        }

        fn percentiles(mut latencies: &[u64]) -> (u64, u64) {
            if latencies.is_empty() { return (0, 0); }
            let mut sorted = latencies.to_vec();
            sorted.sort_unstable();
            let p95_idx = (sorted.len() as f64 * 0.95) as usize;
            let p99_idx = (sorted.len() as f64 * 0.99) as usize;
            (sorted[p95_idx.min(sorted.len() - 1)], sorted[p99_idx.min(sorted.len() - 1)])
        }

        fn print(&self, scenario: &str) {
            println!("\n=== {} ===", scenario);
            println!("Total: {} | Success: {} | Failed: {}", self.total, self.success, self.failed);
            println!("Duration: {}ms | Throughput: {:.2} ops/s", self.duration_ms, self.throughput);
            println!("Latency p95: {}ms | p99: {}ms", self.p95_ms, self.p99_ms);
        }
    }

    #[test]
    fn test_100_concurrent_actors() {
        let h = Arc::new(Harness::new());
        h.fund(10_000_000_000);
        
        let start = Instant::now();
        let results = Arc::new(Mutex::new((0usize, 0usize, Vec::new())));
        let mut handles = vec![];

        for i in 0..100 {
            let h = Arc::clone(&h);
            let results = Arc::clone(&results);
            let handle = std::thread::spawn(move || {
                let op_start = Instant::now();
                let player = h.player();
                let success = h.play_win_round(&player, 5_000_000) && h.client.cash_out(&player).is_ok();
                let latency = op_start.elapsed().as_millis() as u64;
                
                let mut r = results.lock().unwrap();
                if success { r.0 += 1; } else { r.1 += 1; }
                r.2.push(latency);
            });
            handles.push(handle);
        }

        for h in handles { h.join().unwrap(); }
        let duration = start.elapsed().as_millis() as u64;
        let (success, failed, latencies) = {
            let r = results.lock().unwrap();
            (r.0, r.1, r.2.clone())
        };

        let metrics = LoadMetrics::new(100, success, failed, duration, &latencies);
        metrics.print("100 Concurrent Actors");
        assert!(success >= 95);
    }

    #[test]
    fn test_500_concurrent_actors() {
        let h = Arc::new(Harness::new());
        h.fund(50_000_000_000);
        
        let start = Instant::now();
        let results = Arc::new(Mutex::new((0usize, 0usize, Vec::new())));
        let mut handles = vec![];

        for _ in 0..500 {
            let h = Arc::clone(&h);
            let results = Arc::clone(&results);
            let handle = std::thread::spawn(move || {
                let op_start = Instant::now();
                let player = h.player();
                let success = h.play_win_round(&player, 5_000_000) && h.client.cash_out(&player).is_ok();
                let latency = op_start.elapsed().as_millis() as u64;
                
                let mut r = results.lock().unwrap();
                if success { r.0 += 1; } else { r.1 += 1; }
                r.2.push(latency);
            });
            handles.push(handle);
        }

        for h in handles { h.join().unwrap(); }
        let duration = start.elapsed().as_millis() as u64;
        let (success, failed, latencies) = {
            let r = results.lock().unwrap();
            (r.0, r.1, r.2.clone())
        };

        let metrics = LoadMetrics::new(500, success, failed, duration, &latencies);
        metrics.print("500 Concurrent Actors");
        assert!(success >= 475);
    }

    #[test]
    #[ignore] // Run with: cargo test --release -- --ignored
    fn test_1000_concurrent_actors() {
        let h = Arc::new(Harness::new());
        h.fund(100_000_000_000);
        
        let start = Instant::now();
        let results = Arc::new(Mutex::new((0usize, 0usize, Vec::new())));
        let mut handles = vec![];

        for _ in 0..1000 {
            let h = Arc::clone(&h);
            let results = Arc::clone(&results);
            let handle = std::thread::spawn(move || {
                let op_start = Instant::now();
                let player = h.player();
                let success = h.play_win_round(&player, 5_000_000) && h.client.cash_out(&player).is_ok();
                let latency = op_start.elapsed().as_millis() as u64;
                
                let mut r = results.lock().unwrap();
                if success { r.0 += 1; } else { r.1 += 1; }
                r.2.push(latency);
            });
            handles.push(handle);
        }

        for h in handles { h.join().unwrap(); }
        let duration = start.elapsed().as_millis() as u64;
        let (success, failed, latencies) = {
            let r = results.lock().unwrap();
            (r.0, r.1, r.2.clone())
        };

        let metrics = LoadMetrics::new(1000, success, failed, duration, &latencies);
        metrics.print("1000 Concurrent Actors");
        assert!(success >= 950);
    }

    #[test]
    fn test_reserve_depletion_concurrent() {
        let h = Arc::new(Harness::new());
        h.fund(200_000_000); // Limited reserves
        
        let start = Instant::now();
        let results = Arc::new(Mutex::new((0usize, 0usize, Vec::new())));
        let mut handles = vec![];

        for i in 0..100 {
            let h = Arc::clone(&h);
            let results = Arc::clone(&results);
            let handle = std::thread::spawn(move || {
                let op_start = Instant::now();
                let player = h.player();
                let commit = h.make_commitment(i as u8);
                let success = h.client.try_start_game(&player, &Side::Heads, &10_000_000, &commit).is_ok();
                if success { let _ = h.client.cash_out(&player); }
                let latency = op_start.elapsed().as_millis() as u64;
                
                let mut r = results.lock().unwrap();
                if success { r.0 += 1; } else { r.1 += 1; }
                r.2.push(latency);
            });
            handles.push(handle);
        }

        for h in handles { h.join().unwrap(); }
        let duration = start.elapsed().as_millis() as u64;
        let (success, failed, latencies) = {
            let r = results.lock().unwrap();
            (r.0, r.1, r.2.clone())
        };

        let metrics = LoadMetrics::new(100, success, failed, duration, &latencies);
        metrics.print("Reserve Depletion Concurrent");
        
        let stats = h.stats();
        assert!(stats.reserve_balance >= 0);
        assert!(failed > 0, "Expected some rejections due to reserve limits");
    }
}
