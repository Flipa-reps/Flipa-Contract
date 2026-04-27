# Load Testing Contract Concurrent Usage (#378)

## Plan Steps
- [x] Create branch `add-load-testing-contract-concurrent-usage`
- [x] Update `Cargo.toml` → add tokio dependency
- [x] Create `load_tests.rs` → sequential load scenarios (Soroban Env is not Send)
- [x] Scenarios: game starts, reveals, cash-outs, continues
- [x] Reserve depletion stress tests
- [x] Metrics: ≥95% success rate, state consistency assertions
- [ ] `cargo test --release` verification
- [ ] Commit: `test: add load testing...`
- [ ] PR creation

**Note**: Soroban's `Env` is not `Send`, so tests use sequential loops with
per-player unique seeds rather than OS threads. Metrics (throughput, p95/p99
latency) are still collected and printed for each scenario.

**Run**:
```bash
cargo test --test load_tests --release
# Heavy 1000-cycle test:
cargo test --test load_tests --release -- --ignored
```
