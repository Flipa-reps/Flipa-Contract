# Load Testing Contract Concurrent Usage (#378)

## Plan Steps
- [x] Create branch `add-load-testing-contract-concurrent-usage`
- [ ] Update `Cargo.toml` → add tokio dependency
- [ ] Create `load_tests.rs` → 100 concurrent players
- [ ] Scenarios: game starts, reveals, cash-outs, continues
- [ ] Reserve depletion stress tests
- [ ] Metrics: 100% success rate, state consistency
- [ ] `cargo test --release` verification
- [ ] Commit: `test: add load testing...`
- [ ] PR creation

**Next**: Update Cargo.toml + create load_tests.rs
