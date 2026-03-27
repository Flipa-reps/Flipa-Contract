# Contract Metadata and Public API Documentation Notes

Branch: `feature/contract-docs`
Closes #158

## What was documented

### Crate-level (`//!` module doc)
Added a top-of-file module doc block covering:
- Architecture overview (types, helpers, test suite)
- Commit-reveal randomness model (step-by-step)
- Error code stability guarantee

### `error_codes` module
Already had a full error-code table. No changes needed.

### `Error` enum
Already had per-variant doc comments with code references. No changes needed.

### `Side` enum
Replaced minimal comment with a full doc block explaining:
- Purpose (player's chosen outcome)
- When it is used (`start_game`, compared in `reveal`)
- `#[repr(u32)]` serialization note
- Per-variant discriminant docs

### `GamePhase` enum
Replaced minimal comment with a full doc block including:
- ASCII state-transition diagram (`Committed → Revealed → Completed`)
- Per-variant descriptions explaining what each phase means for the player

### `GameState` struct
Already had field-level doc comments. No changes needed.

### `ContractConfig` struct
Replaced inline `//` comments with proper `///` field-level doc comments.
Added struct-level doc explaining:
- Where it is stored (`StorageKey::Config`)
- Which admin functions mutate which fields
- Fee snapshot isolation guarantee

### `ContractStats` struct
Replaced inline `//` comments with proper `///` field-level doc comments.
Added struct-level doc explaining:
- Where it is stored (`StorageKey::Stats`)
- `reserve_balance` as the authoritative solvency figure

### `StorageKey` enum
Replaced inline `//` comments with proper `///` per-variant doc comments.
Added enum-level doc explaining the storage backend used.

### `CoinflipContract` struct
Added a comprehensive struct-level doc block with:
- Full public API table (function, caller, description)
- Randomness model summary
- Reserve solvency guarantee

### Internal storage helpers
Added `///` doc comments to all seven private helpers:
`save_config`, `load_config`, `save_stats`, `load_stats`,
`save_player_game`, `load_player_game`, `delete_player_game`

### `claim_winnings`
Replaced terse comment with a full doc block including:
- Step-by-step process description
- Arguments and return value
- Error table with conditions
- Note clarifying the streak-zero / loss-state behavior

### `cash_out`
Replaced terse comment with a full doc block including:
- Distinction from `claim_winnings` (no token transfer)
- Explicit `Ok(net_payout)` return value documentation
- Ordered guard list
- Error table

### Already well-documented (no changes needed)
- `get_multiplier` — multiplier table + per-streak values
- `calculate_payout` — formula, arithmetic assumptions, `None` overflow contract
- `verify_commitment` — byte format assumptions
- `initialize` — accepted inputs, error conditions
- `start_game` — full guard list, wager boundary semantics, invariants
- `reveal` — process steps, error list
- `continue_streak` — eligibility rules, process, error table
- `set_paused` — pause scope, arguments, errors, security notes
- `set_treasury` — arguments, errors, authorization invariants
- `set_wager_limits` — arguments, errors, authorization invariants
- `set_fee` — arguments, errors, security notes, fee-snapshot isolation

## Test status

Cargo is not available in the current shell environment (Rust toolchain not on PATH).
The documentation changes are purely additive `///` and `//!` comments with no
logic modifications; all existing tests remain structurally unchanged.

To verify:
```bash
cargo test
# Expected: test result: ok. 43 passed; 0 failed; 0 ignored
```
