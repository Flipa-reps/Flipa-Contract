# Design Document: Config Versioning and Rollback

## Overview

This feature adds immutable configuration snapshots and atomic rollback to the Tossd
Soroban coinflip contract (issue #508). Every admin-initiated `ContractConfig` mutation
automatically appends a `ConfigVersion` to a persistent history store. The admin can
list versions, retrieve a specific version, diff two versions, and atomically revert the
live config to any prior snapshot.

All new logic lives inside the existing `lib.rs`. No new files are required.

---

## Architecture

```mermaid
flowchart TD
    subgraph Entry Points (admin-only mutations)
        SF[set_fee]
        SWL[set_wager_limits]
        ST[set_treasury]
        SP[set_paused]
        SM[set_multipliers]
        UC[update_config]
        RC[rollback_config]
        INIT[initialize]
    end

    subgraph Internal Helper
        SC[snapshot_config\ninternal fn]
    end

    subgraph Storage
        CFG[(StorageKey::Config\nContractConfig)]
        HIST[(StorageKey::ConfigHistory\nVec&lt;ConfigVersion&gt;)]
    end

    subgraph Read-Only Queries (any caller)
        LCV[list_config_versions]
        GCV[get_config_version]
        CCV[compare_config_versions]
    end

    SF & SWL & ST & SP & SM & UC & INIT -->|write config| CFG
    SF & SWL & ST & SP & SM & UC & INIT -->|call after write| SC
    RC -->|read snapshot| HIST
    RC -->|write config| CFG
    RC -->|call after write| SC
    SC -->|append + evict| HIST
    LCV & GCV & CCV -->|read| HIST
```

**Key decisions:**

- `snapshot_config` is a private helper called by every mutating entry point *after* the
  config write succeeds. This keeps the snapshot logic in one place and guarantees the
  snapshot always reflects the committed state.
- The history is stored as a single `Vec<ConfigVersion>` under one storage key. With a
  50-entry cap the serialized size stays well within Soroban's entry size limits.
- `rollback_config` writes the restored `ContractConfig` in a single `env.storage().persistent().set(...)` call, satisfying the atomicity requirement. Soroban's
  single-threaded execution model ensures no intermediate state is observable.

---

## Components and Interfaces

### 3.1 `snapshot_config` (internal helper)

```rust
fn snapshot_config(env: &Env, label: Bytes) {
    let config: ContractConfig = env.storage().persistent()
        .get(&StorageKey::Config).unwrap();
    let mut history: Vec<ConfigVersion> = env.storage().persistent()
        .get(&StorageKey::ConfigHistory)
        .unwrap_or_else(|| Vec::new(env));

    let next_version = history.last()
        .map(|v| v.version_number + 1)
        .unwrap_or(1);

    let snapshot = ConfigVersion {
        version_number: next_version,
        ledger: env.ledger().sequence(),
        label,
        config,
    };

    history.push_back(snapshot);

    // Evict oldest entry when cap is exceeded
    if history.len() > MAX_CONFIG_HISTORY {
        history.pop_front();
    }

    env.storage().persistent().set(&StorageKey::ConfigHistory, &history);
}
```

Called at the end of every config-mutating entry point, after the config write.

### 3.2 `list_config_versions`

```rust
pub fn list_config_versions(env: Env) -> Vec<ConfigVersion>
```

Returns the full `Vec<ConfigVersion>` from `StorageKey::ConfigHistory`, or an empty
`Vec` if the key is absent. No auth required.

### 3.3 `get_config_version`

```rust
pub fn get_config_version(env: Env, version_number: u32) -> Result<ConfigVersion, Error>
```

Iterates the history vec and returns the first entry whose `version_number` matches.
Returns `Error::VersionNotFound` if no match. No auth required.

### 3.4 `rollback_config`

```rust
pub fn rollback_config(env: Env, admin: Address, version_number: u32) -> Result<(), Error>
```

1. `admin.require_auth()`
2. Verify `admin == config.admin` → `Error::Unauthorized`
3. Find target `ConfigVersion` → `Error::VersionNotFound`
4. Write `target.config` to `StorageKey::Config` (single storage op)
5. Call `snapshot_config` with label `"rollback to v{version_number}"`
6. Emit `config_rollback` event with `(target_version, new_version)`

### 3.5 `compare_config_versions`

```rust
pub fn compare_config_versions(
    env: Env,
    version_a: u32,
    version_b: u32,
) -> Result<Vec<ConfigDiffEntry>, Error>
```

Fetches both versions (returns `Error::VersionNotFound` for either missing), then
compares each field of `ContractConfig` and returns a `Vec<ConfigDiffEntry>` containing
only the differing fields. Returns an empty vec when configs are identical. No auth
required.

### 3.6 Integration with existing entry points

Each of the following functions gains an additional `label: Option<Bytes>` parameter.
The label is validated (≤ 64 bytes) before any state mutation. After writing the config,
`snapshot_config` is called with the resolved label (empty `Bytes` when `None`).

| Entry point | Change |
|---|---|
| `initialize` | Add `snapshot_config` call at end; no label param (uses empty label) |
| `set_fee` | Add `label: Option<Bytes>` param; validate; call `snapshot_config` after write |
| `set_wager_limits` | Same |
| `set_treasury` | Same |
| `set_paused` | Same |
| `set_multipliers` | Same |
| `update_config` | Same |

Label validation (extracted to avoid duplication):

```rust
fn validate_label(label: &Option<Bytes>) -> Result<Bytes, Error> {
    match label {
        None => Ok(Bytes::new(env)),
        Some(b) if b.len() > 64 => Err(Error::InvalidVersionLabel),
        Some(b) => Ok(b.clone()),
    }
}
```

---

## Data Models

### `ConfigVersion`

```rust
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConfigVersion {
    /// Monotonically increasing counter starting at 1.
    pub version_number: u32,
    /// Ledger sequence at snapshot creation time.
    pub ledger: u32,
    /// Human-readable label (≤ 64 bytes UTF-8). Empty when not supplied.
    pub label: Bytes,
    /// Full copy of ContractConfig at the time of the snapshot.
    pub config: ContractConfig,
}
```

### `ConfigDiffEntry`

```rust
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConfigDiffEntry {
    /// Field name as a short symbol (e.g. "fee_bps", "paused").
    pub field: Symbol,
    /// Serialized value from version_a.
    pub value_a: Bytes,
    /// Serialized value from version_b.
    pub value_b: Bytes,
}
```

`value_a` and `value_b` are XDR-encoded representations of the field values, produced
via `env.to_xdr(...)`. This keeps the type generic without requiring a separate enum
per field.

### `StorageKey` additions

```rust
pub enum StorageKey {
    // ... existing variants ...

    /// Ordered list of config snapshots (Vec<ConfigVersion>, max 50 entries).
    ConfigHistory,
}
```

### New error variants

```rust
pub enum Error {
    // ... existing variants ...

    /// Label supplied to a config-mutating entry point exceeds 64 bytes.
    /// Code: 35
    InvalidVersionLabel = 35,

    /// Requested version_number does not exist in the ConfigHistory store.
    /// Code: 36
    VersionNotFound = 36,
}
```

And in `error_codes`:

```rust
pub const INVALID_VERSION_LABEL: u32 = 35;
pub const VERSION_NOT_FOUND: u32 = 36;
```

### Constants

```rust
/// Maximum number of ConfigVersion entries retained in ConfigHistory.
pub const MAX_CONFIG_HISTORY: u32 = 50;

/// Maximum byte length of a ConfigVersion label.
pub const MAX_LABEL_BYTES: u32 = 64;
```

---

## Correctness Properties

*A property is a characteristic or behavior that should hold true across all valid
executions of a system — essentially, a formal statement about what the system should do.
Properties serve as the bridge between human-readable specifications and
machine-verifiable correctness guarantees.*

### Property 1: Snapshot on every mutation

*For any* valid `ContractConfig` state and any config-mutating entry point
(`set_fee`, `set_wager_limits`, `set_treasury`, `set_paused`, `set_multipliers`,
`update_config`, `rollback_config`), after a successful call the `ConfigHistory` store
must contain one more entry than before the call, and the last entry's `config` field
must be field-for-field equal to the live `ContractConfig` after the mutation.

**Validates: Requirements 1.1, 1.3**

---

### Property 2: Version number monotonicity

*For any* sequence of N successful config-mutating calls starting from an empty history,
the `version_number` of the Nth snapshot must equal N, and for any two consecutive
entries `a` and `b` in the history (in order), `b.version_number == a.version_number + 1`.

**Validates: Requirements 1.2**

---

### Property 3: Label round-trip and validation

*For any* byte string `label` of length ≤ 64, calling a config-mutating entry point
with that label must succeed and the resulting `ConfigVersion.label` must equal `label`.
*For any* byte string of length > 64, the call must return `Error::InvalidVersionLabel`
and the `ContractConfig` and `ConfigHistory` must remain unchanged.

**Validates: Requirements 1.4, 1.5**

---

### Property 4: History cap invariant

*For any* sequence of config-mutating calls, the length of `ConfigHistory` must never
exceed 50. When the 51st snapshot would be appended, the entry with the lowest
`version_number` must be evicted first, so the store always holds the 50 most recent
snapshots.

**Validates: Requirements 1.6**

---

### Property 5: List ordering

*For any* `ConfigHistory` state, `list_config_versions()` must return entries in
strictly ascending `version_number` order, with no gaps or duplicates in the returned
slice.

**Validates: Requirements 2.1, 2.2**

---

### Property 6: VersionNotFound for missing versions

*For any* `version_number` that does not exist in `ConfigHistory`, all three of
`get_config_version(version_number)`, `rollback_config(admin, version_number)`, and
`compare_config_versions(version_number, any)` must return `Error::VersionNotFound`
and leave all contract state unchanged.

**Validates: Requirements 2.4, 3.4, 4.4**

---

### Property 7: Rollback round-trip

*For any* `version_number` present in `ConfigHistory`, calling
`rollback_config(admin, version_number)` must succeed, and the live `ContractConfig`
immediately after the call must be field-for-field equal to `ConfigHistory[version_number].config`.
Additionally, `get_config_version` on the newly created rollback snapshot must return a
`ContractConfig` equal to the original target snapshot.

**Validates: Requirements 3.1, 5.5**

---

### Property 8: Rollback audit snapshot

*For any* successful `rollback_config(admin, version_number)` call, a new
`ConfigVersion` must be appended to `ConfigHistory` with:
- `version_number` equal to the previous maximum plus 1
- `label` equal to `"rollback to v{version_number}"` (where `version_number` is the
  target)
- `config` field-for-field equal to the restored `ContractConfig`

**Validates: Requirements 3.2**

---

### Property 9: Unauthorized rollback rejection

*For any* address that is not equal to `ContractConfig.admin`, calling
`rollback_config` must return `Error::Unauthorized`, and both `ContractConfig` and
`ConfigHistory` must remain byte-for-byte unchanged. No `config_rollback` event must
be emitted.

**Validates: Requirements 3.3, 6.1, 6.2, 6.4**

---

### Property 10: Diff symmetry

*For any* two valid version numbers `a` and `b` in `ConfigHistory`,
`compare_config_versions(a, b)` and `compare_config_versions(b, a)` must return
`ConfigDiffEntry` lists with exactly the same set of `field` values, and for each
field the `value_a`/`value_b` in the first call must equal the `value_b`/`value_a`
in the second call (values are swapped, field names are identical).

**Validates: Requirements 4.6**

---

### Property 11: Rollback event emission

*For any* successful `rollback_config(admin, version_number)` call, exactly one event
with topic `"config_rollback"` must be emitted, containing the target `version_number`
and the `version_number` of the newly created rollback snapshot.

**Validates: Requirements 3.5**

---

## Error Handling

| Scenario | Behaviour |
|---|---|
| `label` > 64 bytes on any mutating call | Return `Error::InvalidVersionLabel`; no config write; no snapshot |
| `get_config_version` with unknown version | Return `Error::VersionNotFound`; no state change |
| `rollback_config` with unknown version | Return `Error::VersionNotFound`; no state change |
| `rollback_config` by non-admin | Return `Error::Unauthorized`; no state change; no event |
| `compare_config_versions` with either version missing | Return `Error::VersionNotFound` |
| `ConfigHistory` absent from storage (first call) | Treat as empty `Vec`; create first snapshot with `version_number = 1` |
| `ConfigHistory` at 50-entry cap | Evict `pop_front()` before `push_back()`; never panic |

---

## Testing Strategy

### Dual Testing Approach

Unit tests cover specific examples, integration points, and error conditions.
Property-based tests verify universal correctness across all valid inputs.
Both are required; neither replaces the other.

### Property-Based Testing

**Library:** [`proptest`](https://github.com/proptest-rs/proptest) (already common in
Soroban ecosystem). Minimum **100 iterations** per property test.

Each property test must include a comment referencing the design property:
```rust
// Feature: config-versioning-rollback, Property N: <property_text>
```

Each correctness property (1–11) must be implemented by a single `proptest!` block.

**Test file:** `Tossd/contract/src/config_versioning_tests.rs`

```rust
use proptest::prelude::*;

// Feature: config-versioning-rollback, Property 1: snapshot on every mutation
proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]
    #[test]
    fn prop_snapshot_on_every_mutation(
        fee_bps in 200u32..=500u32,
        label in proptest::collection::vec(any::<u8>(), 0..=64usize),
    ) {
        // set up env, call set_fee, verify history grew by 1 and last entry matches config
    }
}

// Feature: config-versioning-rollback, Property 2: version number monotonicity
proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]
    #[test]
    fn prop_version_monotonicity(
        mutations in proptest::collection::vec(any::<u32>(), 1..=20usize),
    ) {
        // apply N mutations, verify version_number of Nth entry == N
    }
}

// Feature: config-versioning-rollback, Property 7: rollback round-trip
proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]
    #[test]
    fn prop_rollback_round_trip(
        target_idx in 0usize..10usize,
        num_mutations in 1usize..=10usize,
    ) {
        // apply num_mutations, pick target_idx, rollback, verify live config == snapshot config
    }
}

// Feature: config-versioning-rollback, Property 10: diff symmetry
proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]
    #[test]
    fn prop_diff_symmetry(
        va in 1u32..=10u32,
        vb in 1u32..=10u32,
    ) {
        // compare(a,b) field names == compare(b,a) field names; values swapped
    }
}
```

### Unit Tests (in `config_versioning_tests.rs`)

- `test_initialize_creates_version_1` — after `initialize`, history has exactly one
  entry with `version_number = 1` (Req 1.7)
- `test_label_too_long_rejected` — label of 65 bytes returns `InvalidVersionLabel`,
  config unchanged (Req 1.5)
- `test_list_empty_history` — `list_config_versions` on fresh contract returns `[]`
  (Req 2.2)
- `test_get_version_not_found` — `get_config_version(999)` returns `VersionNotFound`
  (Req 2.4)
- `test_rollback_unauthorized` — non-admin caller returns `Unauthorized`, state
  unchanged, no event (Req 3.3, 6.2)
- `test_rollback_emits_event` — successful rollback emits `config_rollback` event with
  correct fields (Req 3.5)
- `test_rollback_audit_label` — rollback snapshot label equals `"rollback to v{n}"`
  (Req 3.2)
- `test_compare_identical_versions` — comparing a version to itself returns empty diff
  (Req 4.3)
- `test_read_only_queries_no_auth` — any address can call `list_config_versions`,
  `get_config_version`, `compare_config_versions` (Req 6.3)
- `test_history_cap_evicts_oldest` — after 51 mutations, history length == 50 and
  version 1 is gone (Req 1.6)
