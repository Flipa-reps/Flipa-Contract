# Requirements Document

## Introduction

This feature replaces the contract's single all-or-nothing `paused` flag with a
per-operation pause system. Administrators will be able to pause individual
operation types (new game creation, reveals, cash-outs, streak continuation)
independently, record a human-readable reason for each pause action, and query
analytics about pause history. The existing `ContractPaused` error code and
admin-only access control are preserved; the new system is a strict superset of
the current behaviour.

Closes #509.

---

## Glossary

- **Contract**: The Tossd CosmWasm/Soroban coinflip smart contract.
- **Admin**: The privileged address stored in `ContractConfig.admin`; the only
  caller permitted to invoke pause-management entry points.
- **Operation**: A discrete, player-facing contract entry point. The four
  pausable operations are `StartGame`, `Reveal`, `CashOut`, and
  `ContinueStreak`.
- **OperationFlag**: A boolean stored per `Operation` indicating whether that
  operation is currently paused (`true`) or active (`false`).
- **PauseReason**: A UTF-8 string (≤ 256 bytes) supplied by the Admin when
  setting an `OperationFlag`; stored alongside the flag for observability.
- **PauseRecord**: An immutable log entry capturing the `Operation`, the new
  `OperationFlag` value, the `PauseReason`, and the ledger sequence at which
  the change was made.
- **PauseAnalytics**: An aggregate view derived from `PauseRecord` entries,
  exposing per-operation pause counts, total paused ledgers, and the most
  recent reason for each operation.
- **GlobalPause**: The existing `ContractConfig.paused` boolean; retained for
  backward compatibility and treated as a master override that pauses all
  operations simultaneously.
- **Circuit_Breaker**: The existing `min_reserve_threshold` mechanism that
  auto-pauses `StartGame` when reserves fall at or below the threshold.

---

## Requirements

### Requirement 1: Per-Operation Pause Flags

**User Story:** As an Admin, I want to pause individual contract operations
independently, so that I can surgically disable a specific entry point during
an incident without blocking unaffected player activity.

#### Acceptance Criteria

1. THE Contract SHALL store a separate `OperationFlag` for each of the four
   operations: `StartGame`, `Reveal`, `CashOut`, and `ContinueStreak`.
2. WHEN the Admin calls `set_operation_paused(operation, paused, reason)`, THE
   Contract SHALL update the `OperationFlag` for that `Operation` to the
   supplied boolean value.
3. IF a non-Admin address calls `set_operation_paused`, THEN THE Contract SHALL
   return `Error::Unauthorized` and leave all `OperationFlag` values unchanged.
4. WHEN `set_operation_paused` is called with the same boolean value that is
   already stored for that `Operation`, THE Contract SHALL accept the call
   without error (idempotent).
5. THE Contract SHALL expose a `get_operation_paused(operation)` query that
   returns the current `OperationFlag` for the specified `Operation`.

---

### Requirement 2: Granular Pause Enforcement

**User Story:** As a player, I want unaffected operations to remain available
when only specific operations are paused, so that I can still settle my
in-flight game even during a partial pause.

#### Acceptance Criteria

1. WHEN the `OperationFlag` for `StartGame` is `true`, THE Contract SHALL
   return `Error::ContractPaused` for any `start_game` call and SHALL NOT
   create a new `GameState`.
2. WHEN the `OperationFlag` for `Reveal` is `true`, THE Contract SHALL return
   `Error::ContractPaused` for any `reveal` call and SHALL NOT transition the
   `GameState` phase.
3. WHEN the `OperationFlag` for `CashOut` is `true`, THE Contract SHALL return
   `Error::ContractPaused` for any `cash_out` or `claim_winnings` call and
   SHALL NOT transfer funds or delete the `GameState`.
4. WHEN the `OperationFlag` for `ContinueStreak` is `true`, THE Contract SHALL
   return `Error::ContractPaused` for any `continue_streak` call and SHALL NOT
   advance the `GameState`.
5. WHILE the `OperationFlag` for `StartGame` is `true`, THE Contract SHALL
   allow `reveal`, `cash_out`, `claim_winnings`, and `continue_streak` to
   execute normally, provided their own `OperationFlag` values are `false`.
6. WHEN the `GlobalPause` flag is `true`, THE Contract SHALL treat all four
   `OperationFlag` values as `true` regardless of their individually stored
   values, preserving backward-compatible all-or-nothing pause behaviour.
7. WHEN the `Circuit_Breaker` triggers (reserves ≤ `min_reserve_threshold`),
   THE Contract SHALL block `start_game` as if the `StartGame` `OperationFlag`
   were `true`, independently of the per-operation flags.

---

### Requirement 3: Pause Reason Logging

**User Story:** As an Admin, I want every pause or unpause action to record a
human-readable reason, so that post-incident reviews can reconstruct why each
operation was paused.

#### Acceptance Criteria

1. THE `set_operation_paused` entry point SHALL accept a `reason` parameter of
   type UTF-8 string with a maximum length of 256 bytes.
2. WHEN `set_operation_paused` is called, THE Contract SHALL persist a
   `PauseRecord` containing: the `Operation`, the new `OperationFlag` value,
   the `PauseReason` string, and the current ledger sequence number.
3. IF the `reason` parameter exceeds 256 bytes, THEN THE Contract SHALL return
   `Error::InvalidPauseReason` and SHALL NOT update the `OperationFlag` or
   write a `PauseRecord`.
4. THE Contract SHALL emit an event with topic `"pause_changed"` on every
   successful `set_operation_paused` call, including the `Operation`, the new
   boolean value, and the `PauseReason` in the event data.
5. THE Contract SHALL expose a `get_pause_reason(operation)` query that returns
   the `PauseReason` from the most recent `PauseRecord` for the specified
   `Operation`, or an empty string if no record exists.

---

### Requirement 4: Pause Analytics

**User Story:** As an Admin, I want to query aggregate pause statistics per
operation, so that I can identify which operations are paused most frequently
and for how long.

#### Acceptance Criteria

1. THE Contract SHALL maintain a `PauseAnalytics` record per `Operation`
   tracking: `pause_count` (number of times the flag was set to `true`),
   `unpause_count` (number of times the flag was set to `false`), and
   `last_paused_ledger` (ledger sequence of the most recent pause action).
2. WHEN `set_operation_paused` is called with `paused = true`, THE Contract
   SHALL increment `pause_count` for that `Operation` by 1.
3. WHEN `set_operation_paused` is called with `paused = false`, THE Contract
   SHALL increment `unpause_count` for that `Operation` by 1.
4. THE Contract SHALL expose a `get_pause_analytics(operation)` query that
   returns the current `PauseAnalytics` record for the specified `Operation`.
5. WHEN `set_operation_paused` is called with `paused = true`, THE Contract
   SHALL update `last_paused_ledger` for that `Operation` to the current ledger
   sequence number.
6. FOR ALL `Operation` values, the `PauseAnalytics` record SHALL be
   initialised with `pause_count = 0`, `unpause_count = 0`, and
   `last_paused_ledger = 0` at contract initialisation.

---

### Requirement 5: Backward Compatibility with GlobalPause

**User Story:** As an integrator, I want the existing `set_paused` entry point
and `ContractConfig.paused` field to continue working unchanged, so that
existing tooling and scripts do not break.

#### Acceptance Criteria

1. THE Contract SHALL retain the `set_paused(admin, paused)` entry point with
   its existing signature and access-control semantics.
2. WHEN `set_paused` is called with `paused = true`, THE Contract SHALL set
   `ContractConfig.paused` to `true` and SHALL block all four operations as
   described in Requirement 2, Criterion 6.
3. WHEN `set_paused` is called with `paused = false`, THE Contract SHALL set
   `ContractConfig.paused` to `false`; the per-operation `OperationFlag` values
   SHALL remain unchanged.
4. THE Contract SHALL preserve the existing property that `set_paused` mutates
   only the `paused` field of `ContractConfig` and no other config fields.

---

### Requirement 6: Admin Access Control Consistency

**User Story:** As a security auditor, I want all pause-management entry points
to enforce the same admin-only access control, so that no privilege-escalation
path exists through the new granular API.

#### Acceptance Criteria

1. THE Contract SHALL require that the caller of `set_operation_paused` matches
   `ContractConfig.admin` before executing any state mutation.
2. IF the caller of `set_operation_paused` does not match `ContractConfig.admin`,
   THEN THE Contract SHALL return `Error::Unauthorized`, SHALL NOT write a
   `PauseRecord`, and SHALL NOT emit a `"pause_changed"` event.
3. THE Contract SHALL require that the caller of `get_operation_paused` and
   `get_pause_analytics` is any valid address (these are read-only queries with
   no access restriction).
4. WHEN an unauthorized `set_operation_paused` call is rejected, THE Contract
   SHALL leave `PauseAnalytics` counters unchanged.

---

### Requirement 7: Frontend Pause Status Display

**User Story:** As a player, I want the UI to clearly indicate which operations
are currently unavailable, so that I understand why a specific action is
disabled without contacting support.

#### Acceptance Criteria

1. THE Frontend SHALL query `get_operation_paused` for each of the four
   operations on page load and after each wallet-connected transaction.
2. WHEN an `OperationFlag` is `true` for a given operation, THE Frontend SHALL
   disable the corresponding UI control and display a human-readable message
   indicating that the operation is temporarily unavailable.
3. WHEN `get_pause_reason` returns a non-empty string for a paused operation,
   THE Frontend SHALL display that reason string alongside the disabled control.
4. THE Frontend SHALL poll `get_operation_paused` at an interval of no more
   than 30 seconds while the user session is active, so that the UI reflects
   re-enabled operations within 30 seconds of the Admin unpausing them.
5. IF a player submits a transaction for a paused operation and the contract
   returns `Error::ContractPaused`, THEN THE Frontend SHALL display an
   informative error message and SHALL NOT retry the transaction automatically.
