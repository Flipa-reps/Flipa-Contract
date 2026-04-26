# Requirements Document

## Introduction

This feature adds configuration versioning and atomic rollback to the Tossd
coinflip contract. Every admin-initiated change to `ContractConfig` creates an
immutable versioned snapshot. The admin can list available versions, compare
any two versions, and atomically revert the live configuration to any prior
snapshot. Rollback is all-or-nothing: either every field of `ContractConfig`
is replaced by the target snapshot, or the contract state is left completely
unchanged.

Closes #508.

---

## Glossary

- **Contract**: The Tossd Soroban coinflip smart contract.
- **Admin**: The privileged address stored in `ContractConfig.admin`; the only
  caller permitted to invoke config-management entry points.
- **ContractConfig**: The existing struct holding all contract-wide settings
  (`admin`, `treasury`, `token`, `fee_bps`, `min_wager`, `max_wager`,
  `paused`, `shutdown_mode`, `multipliers`, `min_reserve_threshold`,
  `oracle_vrf_pk`).
- **ConfigVersion**: An immutable snapshot of `ContractConfig` paired with a
  monotonically increasing `version_number` (u32), the ledger sequence at
  which the snapshot was taken (`ledger`), and a short human-readable
  `label` string (≤ 64 bytes).
- **Version_Store**: The persistent storage structure that holds the ordered
  list of `ConfigVersion` entries, keyed by `StorageKey::ConfigHistory`.
- **Version_Number**: A u32 counter starting at 1, incremented by 1 for each
  new snapshot. Version 0 is reserved to mean "no history".
- **ConfigDiff**: A value-level description of the fields that differ between
  two `ConfigVersion` entries, returned by the comparison query.
- **Atomic_Rollback**: A rollback operation that writes the target
  `ContractConfig` snapshot to persistent storage in a single transaction,
  with no intermediate state visible on-chain.

---

## Requirements

### Requirement 1: Config Version Storage

**User Story:** As an Admin, I want every config change to be automatically
snapshotted, so that a complete audit trail of configuration history is
available for review and recovery.

#### Acceptance Criteria

1. WHEN the Admin calls any config-mutating entry point (`set_fee`,
   `set_wager_limits`, `set_treasury`, `set_paused`, `set_multipliers`,
   `update_config`), THE Contract SHALL create a new `ConfigVersion` snapshot
   of the resulting `ContractConfig` and append it to the `Version_Store`.
2. THE Contract SHALL assign each new `ConfigVersion` a `version_number` equal
   to the previous highest `version_number` plus 1, starting at 1 for the
   first snapshot.
3. THE Contract SHALL record the current ledger sequence number in the
   `ConfigVersion.ledger` field at the time the snapshot is created.
4. THE Contract SHALL accept an optional `label` parameter (≤ 64 bytes UTF-8)
   on each config-mutating entry point and store it in the `ConfigVersion`.
   WHEN no label is supplied, THE Contract SHALL store an empty string.
5. IF the `label` parameter exceeds 64 bytes, THEN THE Contract SHALL return
   `Error::InvalidVersionLabel` and SHALL NOT apply the config change or
   create a snapshot.
6. THE Contract SHALL retain a maximum of 50 `ConfigVersion` entries in the
   `Version_Store`. WHEN a new snapshot would exceed this limit, THE Contract
   SHALL evict the oldest entry (lowest `version_number`) before appending.
7. THE Contract SHALL also create an initial `ConfigVersion` snapshot during
   `initialize`, capturing the genesis configuration as version 1.

---

### Requirement 2: Version Listing

**User Story:** As an Admin, I want to query the list of available config
versions, so that I can identify which version to roll back to.

#### Acceptance Criteria

1. THE Contract SHALL expose a `list_config_versions()` query that returns all
   `ConfigVersion` entries currently held in the `Version_Store`, ordered by
   ascending `version_number`.
2. WHEN the `Version_Store` is empty, THE `list_config_versions` query SHALL
   return an empty list without error.
3. THE Contract SHALL expose a `get_config_version(version_number: u32)` query
   that returns the `ConfigVersion` with the matching `version_number`.
4. IF `get_config_version` is called with a `version_number` that does not
   exist in the `Version_Store`, THEN THE Contract SHALL return
   `Error::VersionNotFound`.
5. THE `list_config_versions` and `get_config_version` queries SHALL be
   callable by any address without access restriction.

---

### Requirement 3: Rollback Function

**User Story:** As an Admin, I want to revert the live configuration to a
previous version, so that I can quickly recover from a misconfiguration.

#### Acceptance Criteria

1. THE Contract SHALL expose a `rollback_config(admin, version_number: u32)`
   entry point that replaces the live `ContractConfig` with the snapshot
   stored in the `ConfigVersion` identified by `version_number`.
2. WHEN `rollback_config` succeeds, THE Contract SHALL create a new
   `ConfigVersion` snapshot of the restored configuration (with an
   auto-generated label of the form `"rollback to v{version_number}"`) and
   append it to the `Version_Store`, so the rollback itself is auditable.
3. IF a non-Admin address calls `rollback_config`, THEN THE Contract SHALL
   return `Error::Unauthorized` and SHALL NOT modify `ContractConfig` or the
   `Version_Store`.
4. IF `rollback_config` is called with a `version_number` that does not exist
   in the `Version_Store`, THEN THE Contract SHALL return
   `Error::VersionNotFound` and SHALL NOT modify `ContractConfig`.
5. THE Contract SHALL emit an event with topic `"config_rollback"` on every
   successful `rollback_config` call, including the target `version_number`
   and the new `version_number` assigned to the rollback snapshot.

---

### Requirement 4: Version Comparison

**User Story:** As an Admin, I want to diff two config versions, so that I can
understand exactly what changed between them before deciding to roll back.

#### Acceptance Criteria

1. THE Contract SHALL expose a `compare_config_versions(version_a: u32,
   version_b: u32)` query that returns a `ConfigDiff` describing every
   `ContractConfig` field whose value differs between the two versions.
2. THE `ConfigDiff` SHALL include, for each differing field, the field name,
   the value in `version_a`, and the value in `version_b`.
3. WHEN all fields are identical between the two versions, THE
   `compare_config_versions` query SHALL return an empty `ConfigDiff` without
   error.
4. IF either `version_a` or `version_b` does not exist in the `Version_Store`,
   THEN THE Contract SHALL return `Error::VersionNotFound`.
5. THE `compare_config_versions` query SHALL be callable by any address without
   access restriction.
6. FOR ALL pairs of `ConfigVersion` entries `(a, b)`, the `ConfigDiff`
   returned by `compare_config_versions(a, b)` SHALL contain exactly the same
   field names as `compare_config_versions(b, a)`, with `version_a` and
   `version_b` values swapped (symmetric diff property).

---

### Requirement 5: Atomic Rollback Guarantee

**User Story:** As an Admin, I want rollback to be all-or-nothing, so that the
contract is never left in a partially-reverted state after a failed rollback.

#### Acceptance Criteria

1. WHEN `rollback_config` is called, THE Contract SHALL write the target
   `ContractConfig` snapshot to persistent storage in a single storage
   operation, with no intermediate partial state persisted between the read
   of the snapshot and the write of the live config.
2. IF any storage operation within `rollback_config` fails, THEN THE Contract
   SHALL leave `ContractConfig` and the `Version_Store` in the state they were
   in before the call, with no partial writes applied.
3. THE Contract SHALL NOT expose any entry point that modifies a subset of
   `ContractConfig` fields as part of a rollback; the entire struct is always
   replaced atomically.
4. WHILE a `rollback_config` transaction is executing, THE Contract SHALL NOT
   allow any concurrent config-mutating call to observe an intermediate config
   state (guaranteed by Soroban's single-threaded execution model).
5. FOR ALL valid `version_number` values in the `Version_Store`, calling
   `rollback_config(version_number)` followed by `get_config_version` on the
   newly created rollback snapshot SHALL return a `ContractConfig` that is
   field-for-field equal to the original snapshot at `version_number`
   (round-trip property).

---

### Requirement 6: Admin Access Control

**User Story:** As a security auditor, I want all version-mutating entry points
to enforce admin-only access, so that no unprivileged caller can alter config
history or trigger a rollback.

#### Acceptance Criteria

1. THE Contract SHALL require that the caller of `rollback_config` matches
   `ContractConfig.admin` before executing any state mutation.
2. IF the caller of `rollback_config` does not match `ContractConfig.admin`,
   THEN THE Contract SHALL return `Error::Unauthorized`, SHALL NOT write a new
   `ConfigVersion`, and SHALL NOT emit a `"config_rollback"` event.
3. THE Contract SHALL allow any address to call `list_config_versions`,
   `get_config_version`, and `compare_config_versions` without restriction,
   as these are read-only queries.
4. WHEN an unauthorized `rollback_config` call is rejected, THE Contract SHALL
   leave the `Version_Store` and `ContractConfig` completely unchanged.
