# Implementation Plan: Config Versioning and Rollback (#508)

## Overview

Add immutable config snapshots and atomic rollback to `lib.rs`. Every admin config mutation appends a `ConfigVersion` to a persistent history store. Admins can list, retrieve, diff, and roll back to any prior snapshot.

## Tasks

- [ ] 1. Add types, constants, and error variants
  - Add `ConfigVersion` struct with `version_number: u32`, `ledger: u32`, `label: Bytes`, `config: ContractConfig`
  - Add `ConfigDiffEntry` struct with `field: Symbol`, `value_a: Bytes`, `value_b: Bytes`
  - Add `StorageKey::ConfigHistory` variant to the `StorageKey` enum
  - Add `Error::InvalidVersionLabel = 35` and `Error::VersionNotFound = 36` to the `Error` enum
  - Add `error_codes::INVALID_VERSION_LABEL: u32 = 35` and `error_codes::VERSION_NOT_FOUND: u32 = 36`
  - Add `MAX_CONFIG_HISTORY: u32 = 50` and `MAX_LABEL_BYTES: u32 = 64` constants
  - _Requirements: 1.1, 1.2, 1.5, 1.6, 2.4, 3.4_

- [ ] 2. Implement `snapshot_config` internal helper
  - Write private `fn snapshot_config(env: &Env, label: Bytes)` in `lib.rs`
  - Read current `ContractConfig` from `StorageKey::Config`
  - Load `Vec<ConfigVersion>` from `StorageKey::ConfigHistory` (default empty vec)
  - Compute `next_version = last.version_number + 1` or `1` if empty
  - Push new `ConfigVersion` snapshot; evict `pop_front()` when `len > MAX_CONFIG_HISTORY`
  - Persist updated history to `StorageKey::ConfigHistory`
  - _Requirements: 1.1, 1.2, 1.3, 1.6_

  - [ ]* 2.1 Write property test for snapshot on every mutation (Property 1)
    - **Property 1: Snapshot on every mutation**
    - **Validates: Requirements 1.1, 1.3**

  - [ ]* 2.2 Write property test for version number monotonicity (Property 2)
    - **Property 2: Version number monotonicity**
    - **Validates: Requirements 1.2**

  - [ ]* 2.3 Write property test for history cap invariant (Property 4)
    - **Property 4: History cap invariant — history length never exceeds 50**
    - **Validates: Requirements 1.6**

- [ ] 3. Implement read-only query entry points
  - [ ] 3.1 Implement `list_config_versions(env: Env) -> Vec<ConfigVersion>`
    - Return full history vec from `StorageKey::ConfigHistory`; return empty vec when absent
    - No auth required
    - _Requirements: 2.1, 2.2_

  - [ ] 3.2 Implement `get_config_version(env: Env, version_number: u32) -> Result<ConfigVersion, Error>`
    - Iterate history and return matching entry; return `Error::VersionNotFound` if absent
    - No auth required
    - _Requirements: 2.3, 2.4, 2.5_

  - [ ] 3.3 Implement `compare_config_versions(env: Env, version_a: u32, version_b: u32) -> Result<Vec<ConfigDiffEntry>, Error>`
    - Fetch both versions (return `Error::VersionNotFound` for either missing)
    - Compare each `ContractConfig` field; collect differing fields as `ConfigDiffEntry` with XDR-encoded values via `env.to_xdr(...)`
    - Return empty vec when configs are identical
    - No auth required
    - _Requirements: 4.1, 4.2, 4.3, 4.4, 4.5_

  - [ ]* 3.4 Write property test for list ordering (Property 5)
    - **Property 5: list_config_versions returns entries in strictly ascending version_number order**
    - **Validates: Requirements 2.1, 2.2**

  - [ ]* 3.5 Write property test for VersionNotFound (Property 6)
    - **Property 6: VersionNotFound returned for all missing version queries**
    - **Validates: Requirements 2.4, 3.4, 4.4**

  - [ ]* 3.6 Write property test for diff symmetry (Property 10)
    - **Property 10: compare_config_versions(a, b) and compare_config_versions(b, a) return same field names with swapped values**
    - **Validates: Requirements 4.6**

- [ ] 4. Implement `rollback_config` entry point
  - Write `pub fn rollback_config(env: Env, admin: Address, version_number: u32) -> Result<(), Error>`
  - Call `admin.require_auth()`; verify `admin == config.admin` → `Error::Unauthorized`
  - Look up target `ConfigVersion` → `Error::VersionNotFound` if absent
  - Write `target.config` to `StorageKey::Config` in a single `persistent().set(...)` call
  - Call `snapshot_config` with label `"rollback to v{version_number}"`
  - Emit event with topic `"config_rollback"` containing target version and new snapshot version number
  - _Requirements: 3.1, 3.2, 3.3, 3.4, 3.5, 5.1, 5.2, 5.3, 6.1, 6.2_

  - [ ]* 4.1 Write property test for rollback round-trip (Property 7)
    - **Property 7: After rollback_config(v), live ContractConfig equals ConfigHistory[v].config**
    - **Validates: Requirements 3.1, 5.5**

  - [ ]* 4.2 Write property test for rollback audit snapshot (Property 8)
    - **Property 8: Successful rollback appends a new ConfigVersion with correct label and config**
    - **Validates: Requirements 3.2**

  - [ ]* 4.3 Write property test for unauthorized rollback rejection (Property 9)
    - **Property 9: Non-admin rollback_config returns Unauthorized; state unchanged; no event**
    - **Validates: Requirements 3.3, 6.1, 6.2, 6.4**

  - [ ]* 4.4 Write property test for rollback event emission (Property 11)
    - **Property 11: Successful rollback emits exactly one config_rollback event with correct fields**
    - **Validates: Requirements 3.5**

- [ ] 5. Integrate `snapshot_config` into `initialize`
  - At the end of `initialize` (after config write), call `snapshot_config(env, Bytes::new(env))`
  - No label parameter added to `initialize`
  - _Requirements: 1.7_

- [ ] 6. Integrate `snapshot_config` into config-mutating entry points
  - Add `fn validate_label(env: &Env, label: &Option<Bytes>) -> Result<Bytes, Error>` helper (returns empty `Bytes` for `None`, validates ≤ 64 bytes, returns `Error::InvalidVersionLabel` if exceeded)
  - Add `label: Option<Bytes>` parameter to `set_fee`, `set_wager_limits`, `set_treasury`, `set_paused`, `set_multipliers`
  - In each: call `validate_label` before any state mutation; call `snapshot_config` after the config write
  - _Requirements: 1.1, 1.4, 1.5_

  - [ ]* 6.1 Write property test for label round-trip and validation (Property 3)
    - **Property 3: Labels ≤ 64 bytes are stored verbatim; labels > 64 bytes return InvalidVersionLabel with no state change**
    - **Validates: Requirements 1.4, 1.5**

- [ ] 7. Add unit tests in `config_versioning_tests.rs`
  - Create `Tossd/contract/src/config_versioning_tests.rs`
  - Add module declaration in `lib.rs`: `#[cfg(test)] mod config_versioning_tests;`
  - Implement the following unit tests:
    - `test_initialize_creates_version_1` — history has one entry with `version_number = 1` after `initialize` (Req 1.7)
    - `test_label_too_long_rejected` — 65-byte label returns `InvalidVersionLabel`; config unchanged (Req 1.5)
    - `test_list_empty_history` — `list_config_versions` on fresh contract returns `[]` (Req 2.2)
    - `test_get_version_not_found` — `get_config_version(999)` returns `VersionNotFound` (Req 2.4)
    - `test_rollback_unauthorized` — non-admin caller returns `Unauthorized`; state unchanged; no event (Req 3.3, 6.2)
    - `test_rollback_emits_event` — successful rollback emits `config_rollback` event with correct fields (Req 3.5)
    - `test_rollback_audit_label` — rollback snapshot label equals `"rollback to v{n}"` (Req 3.2)
    - `test_compare_identical_versions` — comparing a version to itself returns empty diff (Req 4.3)
    - `test_read_only_queries_no_auth` — any address can call `list_config_versions`, `get_config_version`, `compare_config_versions` (Req 6.3)
    - `test_history_cap_evicts_oldest` — after 51 mutations, history length == 50 and version 1 is absent (Req 1.6)
  - _Requirements: 1.5, 1.6, 1.7, 2.2, 2.4, 3.2, 3.3, 3.5, 4.3, 6.2, 6.3_

- [ ] 8. Final checkpoint
  - Ensure all tests pass, ask the user if questions arise.

## Notes

- Tasks marked with `*` are optional and can be skipped for a faster MVP
- All new code lives in `lib.rs` (and the new test file); no new source files required
- Property tests use `proptest` with a minimum of 100 iterations per property
- Each property test must include a comment: `// Feature: config-versioning-rollback, Property N: <text>`
