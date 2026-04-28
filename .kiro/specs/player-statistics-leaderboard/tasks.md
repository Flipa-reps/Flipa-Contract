# Implementation Plan: Player Statistics and Leaderboard

## Overview

Implement per-player statistics tracking and a multi-category leaderboard for the Tossd
Soroban coinflip contract (player-statistics-leaderboard spec). The work is split across
four layers: Soroban contract (Rust), backend read-model API (TypeScript), React frontend
UI, and property/unit tests for each layer.

## Tasks

- [ ] 1. Contract — Define new types and storage keys
  - Add `PlayerStats` struct to `contract/src/lib.rs` with fields: `games_played` (`u64`), `games_won` (`u64`), `games_lost` (`u64`), `total_wagered` (`i128`), `total_won` (`i128`), `best_streak` (`u32`), `privacy_opt_out` (`bool`)
  - Add `LeaderboardCategory` enum (`TotalWagered | GamesWon | BestStreak`) annotated with `#[contracttype]`
  - Add `LeaderboardEntry` struct with fields `player: Address`, `value: i128`, `rank: u32` annotated with `#[contracttype]`
  - Add `StorageKey::PlayerStats(Address)` and `StorageKey::LeaderboardIndex(LeaderboardCategory)` variants to the existing `StorageKey` enum
  - Add `MAX_LEADERBOARD_SIZE: u32 = 100` constant
  - Add error variants `PlayerNotFound = 60`, `LeaderboardLimitExceeded = 61`, `InvalidLeaderboardCategory = 62` to the `Error` enum with doc comments
  - _Requirements: 1.1, 1.2, 1.5, 3.1, 5.1, 5.2, 5.3, 5.4_

- [ ] 2. Contract — Implement storage helpers
  - [ ] 2.1 Implement `load_player_stats_new(env, player) -> PlayerStats` that returns a zeroed default if no record exists
  - [ ] 2.2 Implement `save_player_stats_new(env, player, stats)`
  - [ ] 2.3 Implement `load_leaderboard_index(env, category) -> Vec<LeaderboardEntry>` returning empty vec if missing
  - [ ] 2.4 Implement `save_leaderboard_index(env, category, index)`
  - _Requirements: 1.2, 1.3, 3.1_

- [ ] 3. Contract — Implement Stats_Updater
  - [ ] 3.1 Implement `update_player_stats(env, player, wager, payout, won, new_streak)` internal function
    - Load existing `PlayerStats` (default if missing)
    - Increment `games_played`, `total_wagered`; branch on `won` to update `games_won`/`total_won`/`best_streak` or `games_lost`
    - Save updated `PlayerStats`
    - If `!privacy_opt_out`, call `update_leaderboard_index`
    - _Requirements: 2.1, 2.2, 2.3, 2.4, 2.5, 2.6_

  - [ ] 3.2 Implement `update_leaderboard_index(env, player, stats)` internal function
    - For each of the three `LeaderboardCategory` variants, compute `value`, load index, remove existing player entry, find insertion point (descending), insert if within cap, trim to `MAX_LEADERBOARD_SIZE`, save
    - _Requirements: 3.2, 3.3, 3.4, 3.5_

  - [ ] 3.3 Wire `update_player_stats` into `reveal` (loss path), `cash_out`, and `claim_winnings` settlement paths
    - Call must occur after all fund transfers succeed and before the function returns `Ok`
    - _Requirements: 2.1, 2.2, 2.3, 2.4, 2.5, 2.6_

- [ ] 4. Contract — Implement new entry points
  - [ ] 4.1 Implement `get_player_stats(env: Env, player: Address) -> Option<PlayerStats>`
    - Return `Some(stats)` if `StorageKey::PlayerStats(player)` exists, else `None`
    - _Requirements: 1.4_

  - [ ] 4.2 Implement `get_leaderboard(env: Env, category: LeaderboardCategory, limit: u32) -> Result<Vec<LeaderboardEntry>, Error>`
    - Return `Err(LeaderboardLimitExceeded)` if `limit > MAX_LEADERBOARD_SIZE`
    - Load index, take first `limit` entries, assign `rank` starting at 1, return
    - _Requirements: 3.6, 3.7, 3.8, 3.9_

  - [ ] 4.3 Implement `set_privacy(env: Env, player: Address, opt_out: bool) -> Result<(), Error>`
    - Call `player.require_auth()`
    - Load `PlayerStats`, set `privacy_opt_out`, save
    - If `opt_out = true`: remove player from all three indexes
    - If `opt_out = false`: call `update_leaderboard_index` with current stats
    - _Requirements: 4.1, 4.2, 4.3_

- [ ] 5. Contract — Property and unit tests
  - [ ] 5.1 Create `contract/src/player_stats_tests.rs` with unit tests
    - Stats initialised to zero on first settlement
    - Win settlement increments `games_played`, `games_won`, `total_wagered`, `total_won`, `best_streak` correctly
    - Loss settlement increments `games_played`, `games_lost`, `total_wagered` only
    - `best_streak` not updated when new streak is lower than existing best
    - `set_privacy(true)` removes player from all three indexes; subsequent settlement does not re-insert
    - `set_privacy(false)` re-inserts player with current stats values
    - `get_leaderboard` returns entries in descending order of value
    - `get_leaderboard` with `limit > MAX_LEADERBOARD_SIZE` returns `LeaderboardLimitExceeded`
    - `get_player_stats` returns `None` for unknown player
    - _Requirements: 1.3, 1.4, 2.1, 2.2, 3.7, 3.8, 4.2, 4.3_

  - [ ]* 5.2 Write property test for games_played invariant (Property 1)
    - **Property 1: games_played invariant**
    - **Validates: Requirements 2.1, 2.2, 2.3, 2.4**
    - File: `contract/src/player_stats_tests.rs`

  - [ ]* 5.3 Write property test for total_wagered monotonicity (Property 2)
    - **Property 2: total_wagered monotonicity**
    - **Validates: Requirements 2.1, 2.2, 2.3, 2.4**
    - File: `contract/src/player_stats_tests.rs`

  - [ ]* 5.4 Write property test for best_streak non-regression (Property 3)
    - **Property 3: best_streak non-regression**
    - **Validates: Requirements 2.1, 3.3**
    - File: `contract/src/player_stats_tests.rs`

  - [ ]* 5.5 Write property test for leaderboard descending order (Property 4)
    - **Property 4: Leaderboard descending order**
    - **Validates: Requirements 3.3, 3.6, 3.7**
    - File: `contract/src/player_stats_tests.rs`

  - [ ]* 5.6 Write property test for leaderboard rank assignment (Property 5)
    - **Property 5: Leaderboard rank assignment**
    - **Validates: Requirements 3.6, 3.7**
    - File: `contract/src/player_stats_tests.rs`

  - [ ]* 5.7 Write property test for privacy exclusion (Property 6)
    - **Property 6: Privacy exclusion**
    - **Validates: Requirements 4.4, 4.5**
    - File: `contract/src/player_stats_tests.rs`

  - [ ]* 5.8 Write property test for privacy re-inclusion (Property 7)
    - **Property 7: Privacy re-inclusion**
    - **Validates: Requirements 4.3**
    - File: `contract/src/player_stats_tests.rs`

  - [ ]* 5.9 Write property test for stats update idempotence under privacy (Property 8)
    - **Property 8: Stats update idempotence under privacy**
    - **Validates: Requirements 4.4**
    - File: `contract/src/player_stats_tests.rs`

  - [ ]* 5.10 Write property test for leaderboard size cap (Property 9)
    - **Property 9: Leaderboard size cap**
    - **Validates: Requirements 3.4, 3.5**
    - File: `contract/src/player_stats_tests.rs`

  - [ ]* 5.11 Write property test for get_player_stats round-trip (Property 10)
    - **Property 10: get_player_stats round-trip**
    - **Validates: Requirements 1.4**
    - File: `contract/src/player_stats_tests.rs`

  - [ ]* 5.12 Write property test for leaderboard limit enforcement (Property 14)
    - **Property 14: Leaderboard limit enforcement**
    - **Validates: Requirements 3.8, 5.2**
    - File: `contract/src/player_stats_tests.rs`

  - [ ]* 5.13 Write property test for settlement atomicity (Property 15)
    - **Property 15: Settlement atomicity**
    - **Validates: Requirements 2.5, 2.6**
    - File: `contract/src/player_stats_tests.rs`

- [ ] 6. Checkpoint — Ensure all contract tests pass
  - Run `cargo test --manifest-path contract/Cargo.toml`; ensure all new and existing tests pass. Ask the user if questions arise.

- [ ] 7. Backend — Implement Leaderboard_API read-model endpoints
  - [ ] 7.1 Add `LeaderboardReadModel` and `PlayerStatsReadModel` TypeScript interfaces to `backend/src/types.ts`
    - Serialise all `i128` values as decimal strings; `u64` game counts as `number`
    - _Requirements: 6.7_

  - [ ] 7.2 Implement `GET /api/leaderboard/:category` in `backend/src/routes/leaderboard.ts`
    - Accept optional `limit` query param (default 10, max 100); return HTTP 400 for invalid category
    - Call `get_leaderboard` via Soroban RPC; cache result with 30-second TTL
    - Return last cached value on RPC failure; return HTTP 503 if no cache exists
    - _Requirements: 6.1, 6.3, 6.4, 6.5, 6.6_

  - [ ] 7.3 Implement `GET /api/players/:playerPublicKey/stats` in `backend/src/routes/leaderboard.ts`
    - Call `get_player_stats` via Soroban RPC; return HTTP 404 with `PLAYER_NOT_FOUND` if `None`
    - _Requirements: 6.2, 6.6_

  - [ ] 7.4 Implement `GET /api/leaderboard/export` and `POST /api/leaderboard/import` in `backend/src/routes/leaderboard.ts`
    - Export: return all three categories as `LeaderboardExport` JSON
    - Import: validate schema, return HTTP 422 with per-entry errors on invalid entries, overwrite cache on success
    - _Requirements: 7.1, 7.2, 7.3, 7.4_

  - [ ]* 7.5 Write property test for leaderboard export round-trip (Property 11)
    - **Property 11: Leaderboard export round-trip**
    - **Validates: Requirements 7.3**
    - File: `backend/src/__tests__/properties/leaderboardApi.property.test.ts`

  - [ ]* 7.6 Write property test for backend cache TTL consistency (Property 12)
    - **Property 12: Backend cache TTL consistency**
    - **Validates: Requirements 6.3, 6.4**
    - File: `backend/src/__tests__/properties/leaderboardApi.property.test.ts`

  - [ ]* 7.7 Write property test for i128 decimal string serialisation (Property 13)
    - **Property 13: i128 decimal string serialisation**
    - **Validates: Requirements 6.7**
    - File: `backend/src/__tests__/properties/leaderboardApi.property.test.ts`

  - [ ]* 7.8 Write unit tests for leaderboard API
    - `GET /api/leaderboard/total_wagered` returns correct shape and order
    - `GET /api/players/:pk/stats` returns 404 for unknown player
    - Invalid category returns HTTP 400
    - Cache hit returns cached data within TTL
    - RPC failure returns last cached value
    - File: `backend/src/__tests__/unit/leaderboard.test.ts`
    - _Requirements: 6.1, 6.2, 6.3, 6.4, 6.5_

- [ ] 8. Checkpoint — Ensure all backend tests pass
  - Run `vitest --run` in `backend/`; ensure all new and existing tests pass. Ask the user if questions arise.

- [ ] 9. Frontend — Implement LeaderboardPanel component
  - [ ] 9.1 Create `frontend/components/LeaderboardPanel.tsx` and `LeaderboardPanel.module.css`
    - Three tabs: Total Wagered, Games Won, Best Streak
    - On tab select: fetch `GET /api/leaderboard/:category?limit=10`; re-fetch every 30 seconds
    - Render rank, truncated address (first 6 + last 4 chars), formatted value (XLM for wagered/won, integer for games/streak)
    - Show loading skeleton while fetching; show error message + retry button on failure
    - _Requirements: 8.1, 8.2, 8.3, 8.6, 8.7_

  - [ ] 9.2 Create `frontend/components/PlayerStatsCard.tsx` and `PlayerStatsCard.module.css`
    - Fetch `GET /api/players/:playerPublicKey/stats` when wallet is connected
    - Display `games_played`, `games_won`, `games_lost`, `total_wagered` (XLM), `total_won` (XLM), `best_streak`
    - Include privacy toggle: checkbox/switch that calls `set_privacy` via wallet on change
    - _Requirements: 8.4, 8.5_

  - [ ]* 9.3 Write property test for leaderboard rendering order (Property 4 — frontend)
    - **Property 4: Leaderboard descending order (rendering)**
    - **Validates: Requirements 3.3, 8.2**
    - File: `frontend/tests/properties/leaderboardPanel.property.test.ts`

  - [ ]* 9.4 Write property test for rank display correctness (Property 5 — frontend)
    - **Property 5: Leaderboard rank assignment (rendering)**
    - **Validates: Requirements 3.6, 8.3**
    - File: `frontend/tests/properties/leaderboardPanel.property.test.ts`

  - [ ]* 9.5 Write unit tests for LeaderboardPanel
    - Tab switch triggers correct category fetch
    - Loading skeleton shown while fetching
    - Error message and retry button shown on fetch failure
    - Privacy toggle calls `set_privacy` with correct argument
    - File: `frontend/tests/LeaderboardPanel.test.tsx`
    - _Requirements: 8.1, 8.2, 8.5, 8.6, 8.7_

- [ ] 10. Final checkpoint — Ensure all tests pass
  - Run `cargo test --manifest-path contract/Cargo.toml` and `vitest --run` in `backend/` and `frontend/`; ensure all tests pass. Ask the user if questions arise.

## Notes

- Tasks marked with `*` are optional and can be skipped for a faster MVP
- Each task references specific requirements for traceability
- Contract property tests use `proptest` with `ProptestConfig::with_cases(100)` and must include the comment `// Feature: player-statistics-leaderboard, Property N: <property_text>`
- Backend/frontend property tests use `fast-check` with `{ numRuns: 100 }` and must include the comment `// Feature: player-statistics-leaderboard, Property N: <property_text>`
- All `i128` values must be serialised as decimal strings in JSON; `u64` game counts may be serialised as `number` (safe within JS integer range for realistic game counts)
- The `rank` field in stored `LeaderboardEntry` records is always 0; ranks are assigned at query time in `get_leaderboard`
- `set_privacy` requires `player.require_auth()` — the frontend must build and submit a signed transaction via the wallet, not a plain REST call
