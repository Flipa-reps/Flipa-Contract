# Contract TODOs

## Issue #379: Add snapshot tests for contract state serialization ✅ PLAN APPROVED

**Status**: Plan approved, implementation started (2024-XX-XX)

### Completed:
- [x] Created detailed edit plan for snapshot tests
- [x] User approved plan

### In Progress:
- [ ] Create `contract/src/snapshot_tests.rs` with insta snapshots for:
  - ContractConfig (default + edges)
  - ContractStats (zero + volumes) 
  - GameState (all phases/sides/streaks)
  - All enums (Side, GamePhase, StorageKey, Error)
- [ ] Round-trip borsh serialization tests
- [ ] Add `insta = "1.40"` to Cargo.toml [dev-dependencies]
- [ ] Integrate via `#[cfg(test)] mod snapshot_tests;` in lib.rs
- [ ] Generate baseline snapshots: `cargo test snapshot_tests`
- [ ] Create branch: `add-snapshot-tests-contract-state-serialization`
- [ ] Commit + PR

### Next:
Run `cargo test snapshot_tests -- --nocapture` to review/approve snapshots.
Update workflow: `cargo test update_snapshots` for intentional changes.

**Catch unintended refactors**: Snapshot mismatch → build fail (CI-protected).

---

## Other Issues:
- Load test tokio concurrency improvements
- Prop test coverage: 95%+ on core paths

