# Implementation Plan: Granular Emergency Pause (Issue #509)

## Overview

Replace the single `ContractConfig.paused` boolean with a per-operation pause
system. Four new `OperationFlag` values (one per player-facing entry point),
a `PauseReason` string, an immutable `PauseRecord` log, and `PauseAnalytics`
counters are added to the contract. The existing `GlobalPause` and circuit-
breaker behaviour are preserved as a strict superset. A frontend polling layer
surfaces per-operation status to players.

## Tasks

- [x] 1. Add core types: `PausableOperation`, `PauseRecord`, `PauseAnalytics`, and `StorageKey` variants
  - Define `PausableOperation` enum with variants `StartGame`, `Reveal`, `CashOut`, `ContinueStreak`
  - Define `PauseRecord` struct: `operation`, `paused` (bool), `reason` (String/Bytes ≤256), `ledger` (u32)
  - Define `PauseAnalytics` struct: `pause_count` (u64), `unpause_count` (u64), `last_paused_ledger` (u32)
  - Add `StorageKey` variants: `OperationFlag(PausableOperation)`, `PauseRecords(PausableOperation)`, `PauseAnalytics(PausableOperation)`
  - Add `Error::InvalidPauseReason` variant with a stable error code (e.g. 34)
  - _Requirements: 1.1, 3.1, 3.3, 4.1, 4.6_

- [x] 2. Implement `set_operation_paused` entry point
  - [x] 2.1 Write `set_operation_paused(admin, operation, paused, reason)` in `lib.rs`
    - Verify caller == `ContractConfig.admin`; return `Error::Unauthorized` on mismatch without any state mutation
    - Validate `reason` byte length ≤ 256; return `Error::InvalidPauseReason` without state mutation if exceeded
    - Write `OperationFlag` for the operation to the supplied boolean (idempotent)
    - Append a `PauseRecord` to the operation's record list with current ledger sequence
    - Update `PauseAnalytics`: increment `pause_count` when `paused=true`, `unpause_count` when `paused=false`; update `last_paused_ledger` when `paused=true`
    - Emit event with topic `"pause_changed"` containing operation, new boolean, and reason
    - _Requirements: 1.2, 1.3, 1.4, 3.1, 3.2, 3.3, 3.4, 4.2, 4.3, 4.5, 6.1, 6.2, 6.4_

  - [ ]* 2.2 Write property test for `set_operation_paused` authorization
    - **Property 1: Unauthorized callers never mutate OperationFlag, PauseRecord, or PauseAnalytics**
    - **Validates: Requirements 1.3, 6.2, 6.4**

  - [ ]* 2.3 Write property test for `set_operation_paused` idempotency
    - **Property 2: Calling set_operation_paused with the same value twice leaves state identical to calling it once**
    - **Validates: Requirements 1.4**

  - [ ]* 2.4 Write property test for reason length validation
    - **Property 3: Any reason string exceeding 256 bytes is rejected without state mutation**
    - **Validates: Requirements 3.3**

- [x] 3. Implement query entry points: `get_operation_paused`, `get_pause_reason`, `get_pause_analytics`
  - Add `get_operation_paused(operation) -> bool` — reads `OperationFlag` from storage (default `false`)
  - Add `get_pause_reason(operation) -> String` — returns `PauseReason` from the most recent `PauseRecord`, or empty string if none
  - Add `get_pause_analytics(operation) -> PauseAnalytics` — returns the analytics record (default-initialised to zeros)
  - Ensure all three queries are callable by any address (no access restriction)
  - _Requirements: 1.5, 3.5, 4.4, 6.3_

- [x] 4. Initialise `PauseAnalytics` to zero at contract initialisation
  - In the `initialize` entry point, write a zeroed `PauseAnalytics` record for each of the four `PausableOperation` variants
  - _Requirements: 4.6_

- [x] 5. Enforce per-operation flags in player-facing entry points
  - [x] 5.1 Guard `start_game`: check `OperationFlag` for `StartGame`; return `Error::ContractPaused` before any state mutation if `true`
    - _Requirements: 2.1_

  - [x] 5.2 Guard `reveal`: check `OperationFlag` for `Reveal`; return `Error::ContractPaused` before phase transition if `true`
    - _Requirements: 2.2_

  - [x] 5.3 Guard `cash_out` and `claim_winnings`: check `OperationFlag` for `CashOut`; return `Error::ContractPaused` before fund transfer if `true`
    - _Requirements: 2.3_

  - [x] 5.4 Guard `continue_streak`: check `OperationFlag` for `ContinueStreak`; return `Error::ContractPaused` before state advance if `true`
    - _Requirements: 2.4_

  - [ ]* 5.5 Write property test for independent operation enforcement
    - **Property 4: Pausing one operation never blocks a different operation**
    - **Validates: Requirements 2.5**

  - [ ]* 5.6 Write property test for GlobalPause override
    - **Property 5: When ContractConfig.paused is true, all four operations return ContractPaused regardless of individual OperationFlag values**
    - **Validates: Requirements 2.6**

  - [ ]* 5.7 Write property test for circuit-breaker independence
    - **Property 6: Circuit-breaker (reserves ≤ min_reserve_threshold) blocks start_game independently of the StartGame OperationFlag**
    - **Validates: Requirements 2.7**

- [ ] 6. Checkpoint — Ensure all contract tests pass
  - Ensure all tests pass, ask the user if questions arise.

- [ ] 7. Preserve backward compatibility with `set_paused`
  - Verify `set_paused(admin, paused)` signature and access-control are unchanged
  - Confirm `set_paused(true)` sets `ContractConfig.paused = true` and that all four entry-point guards treat it as a master override (Requirement 2.6 already covered by task 5.6)
  - Confirm `set_paused(false)` sets `ContractConfig.paused = false` and leaves all four `OperationFlag` values unchanged
  - Confirm `set_paused` mutates only the `paused` field of `ContractConfig`
  - _Requirements: 5.1, 5.2, 5.3, 5.4_

  - [ ]* 7.1 Write property test for `set_paused` field isolation
    - **Property 7: set_paused mutates only ContractConfig.paused and no other config fields or OperationFlag values**
    - **Validates: Requirements 5.4**

- [ ] 8. Update `pause_tests.rs` to cover granular pause scenarios
  - [ ] 8.1 Add unit tests for each operation paused independently (four tests: one per operation)
    - Verify the paused operation returns `Error::ContractPaused`
    - Verify the other three operations remain unblocked
    - _Requirements: 2.1, 2.2, 2.3, 2.4, 2.5_

  - [ ] 8.2 Add unit tests for `PauseRecord` persistence and `get_pause_reason`
    - Verify reason is stored and returned correctly
    - Verify empty string returned when no record exists
    - _Requirements: 3.2, 3.5_

  - [ ] 8.3 Add unit tests for `PauseAnalytics` counters
    - Verify `pause_count` increments on pause, `unpause_count` on unpause
    - Verify `last_paused_ledger` updated correctly
    - Verify zero-initialisation at contract init
    - _Requirements: 4.1, 4.2, 4.3, 4.5, 4.6_

  - [ ]* 8.4 Write property test for analytics counter monotonicity
    - **Property 8: pause_count and unpause_count are non-decreasing across any sequence of set_operation_paused calls**
    - **Validates: Requirements 4.2, 4.3**

- [ ] 9. Checkpoint — Ensure all contract tests pass
  - Ensure all tests pass, ask the user if questions arise.

- [ ] 10. Frontend: query and display per-operation pause status
  - [ ] 10.1 Add `useOperationPauseStatus` hook in `frontend/hooks/`
    - On mount and after each wallet-connected transaction, call `get_operation_paused` for all four operations
    - Poll every 30 seconds while session is active
    - _Requirements: 7.1, 7.4_

  - [ ] 10.2 Add `useOperationPauseReason` hook (or extend 10.1) to fetch `get_pause_reason` for paused operations
    - _Requirements: 7.3_

  - [ ] 10.3 Update UI controls (`StartGame`, `Reveal`, `CashOut`, `ContinueStreak` buttons/components) to consume pause status
    - Disable the control and display a human-readable unavailability message when the corresponding flag is `true`
    - Display the reason string alongside the disabled control when non-empty
    - _Requirements: 7.2, 7.3_

  - [ ] 10.4 Handle `Error::ContractPaused` returned from a submitted transaction
    - Display an informative error message
    - Do not retry automatically
    - _Requirements: 7.5_

- [ ] 11. Final checkpoint — Ensure all tests pass
  - Ensure all tests pass, ask the user if questions arise.

## Notes

- Tasks marked with `*` are optional and can be skipped for a faster MVP
- Each task references specific requirements for traceability
- Checkpoints ensure incremental validation
- Property tests validate universal correctness properties; unit tests validate specific examples and edge cases
- The contract is Rust/Soroban; use `proptest` for property-based tests consistent with the existing test suite
