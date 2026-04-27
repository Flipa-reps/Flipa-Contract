# Contract Upgrade and Versioning System

Every admin configuration change is automatically snapshotted into an immutable
history, enabling full audit trails, version comparison, and one-step rollback.

## Version tracking

A `ConfigVersion` snapshot is appended to `StorageKey::ConfigHistory` after
every successful call to `set_fee`, `set_treasury`, `set_wager_limits`,
`set_paused`, `set_multipliers`, or `update_config`.

```
initialize  →  version 1
set_fee     →  version 2
set_paused  →  version 3
rollback(1) →  version 4  (audit snapshot of the restored config)
```

History is capped at **50 entries** (`MAX_CONFIG_HISTORY`). When the cap is
reached the oldest entry is evicted (FIFO).

### ConfigVersion fields

| Field            | Type            | Description                              |
|------------------|-----------------|------------------------------------------|
| `version_number` | `u32`           | Monotonically increasing (starts at 1)   |
| `ledger`         | `u32`           | Ledger sequence at write time            |
| `label`          | `Bytes` (≤64 B) | Optional human-readable change note      |
| `config`         | `ContractConfig`| Full config snapshot                     |

## Upgrade mechanism

Soroban contracts are upgraded by deploying a new WASM binary via
`env.deployer().update_current_contract_wasm(new_wasm_hash)`. The config
versioning system ensures the live configuration is preserved across WASM
upgrades because it is stored in persistent storage, not in the binary.

**Migration helpers:**
- `list_config_versions` — enumerate all snapshots
- `get_config_version(n)` — retrieve a specific version
- `compare_config_versions(a, b)` — diff two versions field-by-field

## Rollback

```rust
// Revert live config to version 1
client.rollback_config(&admin, &1);
```

`rollback_config`:
1. Requires admin auth.
2. Looks up the target version in history (`Error::VersionNotFound` if missing).
3. Writes the target config atomically to `StorageKey::Config`.
4. Appends a new audit snapshot labelled `"rollback to vN"`.
5. Emits a `(tossd, config_rollback)` event with `(target_version, new_version)`.

Rollback is **non-destructive** — the history is never deleted, so you can
roll forward again by rolling back to a later version.

## Label validation

Labels must be ≤ 64 bytes (`MAX_LABEL_BYTES`). Passing a longer label returns
`Error::InvalidVersionLabel` and leaves the config unchanged.

## Backward compatibility

- Error codes are stable across upgrades (see `error_codes` module).
- In-flight games snapshot `fee_bps` and `multipliers` at `start_game` time,
  so admin changes never retroactively alter active game payouts.
- `oracle_vrf_pk = [0u8; 32]` disables VRF verification for deployments that
  do not use an oracle.

## Relevant code

| Symbol                    | Location              | Description                        |
|---------------------------|-----------------------|------------------------------------|
| `ConfigVersion`           | `lib.rs`              | Snapshot struct                    |
| `ConfigDiffEntry`         | `lib.rs`              | Field diff entry                   |
| `MAX_CONFIG_HISTORY`      | `lib.rs`              | History cap (50)                   |
| `MAX_LABEL_BYTES`         | `lib.rs`              | Label length limit (64)            |
| `snapshot_config`         | `lib.rs`              | Internal snapshot helper           |
| `rollback_config`         | `lib.rs`              | Admin rollback entry point         |
| `list_config_versions`    | `lib.rs`              | Query all versions                 |
| `get_config_version`      | `lib.rs`              | Query single version               |
| `compare_config_versions` | `lib.rs`              | Diff two versions                  |
| `config_versioning_tests` | `src/config_versioning_tests.rs` | Full test suite        |
| `upgrade_migration_tests` | `src/upgrade_migration_tests.rs` | Migration/rollback tests |
