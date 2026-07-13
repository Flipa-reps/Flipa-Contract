# Requirements Document

## Introduction

This feature adds per-player statistics tracking and a multi-category leaderboard to
the Flipa Soroban coinflip contract. Every game settlement atomically updates a
`PlayerStats` record stored in persistent contract storage. Three leaderboard indexes
(by total wagered, by games won, and by best streak) are maintained as sorted vectors
and exposed through new read-only contract entry points. A privacy control lets players
opt out of leaderboard inclusion while still accumulating stats on-chain. The feature
also extends the backend read-model API and the React frontend with a leaderboard UI
panel.

Closes the player statistics and leaderboard work tracked in issue
"Implement player statistics tracking and leaderboard system".

---

## Glossary

- **Contract**: The Flipa Soroban coinflip smart contract written in Rust using the
  Soroban SDK (`no_std`).
- **PlayerStats**: The on-chain per-player statistics record stored under
  `StorageKey::PlayerStats(Address)` in persistent storage. Fields: `games_played`
  (`u64`), `games_won` (`u64`), `games_lost` (`u64`), `total_wagered` (`i128`),
  `total_won` (`i128`), `best_streak` (`u32`), `privacy_opt_out` (`bool`).
- **LeaderboardCategory**: An enum with three variants — `TotalWagered`, `GamesWon`,
  `BestStreak` — that selects which sorted leaderboard index to query.
- **LeaderboardEntry**: A single ranked record returned by `get_leaderboard`. Fields:
  `player` (`Address`), `value` (`i128` for `TotalWagered` and `total_won`; `u64` for
  `GamesWon`; `u32` for `BestStreak` — serialised as `i128` in the struct for
  uniformity), `rank` (`u32`).
- **LeaderboardIndex**: A `Vec<LeaderboardEntry>` stored in persistent contract storage
  under `StorageKey::LeaderboardIndex(LeaderboardCategory)`, maintained in descending
  order of `value`. Capped at `MAX_LEADERBOARD_SIZE` entries.
- **MAX_LEADERBOARD_SIZE**: A configurable constant (default `100`) that caps the
  number of entries in each `LeaderboardIndex`.
- **Stats_Updater**: The internal contract logic that atomically updates `PlayerStats`
  and the three `LeaderboardIndex` entries on every game settlement.
- **Privacy_Controller**: The contract logic behind the `set_privacy` entry point that
  toggles `PlayerStats.privacy_opt_out` and removes or re-inserts the player in all
  three `LeaderboardIndex` entries accordingly.
- **Leaderboard_API**: The backend read-model service that exposes
  `GET /api/leaderboard/:category` and `GET /api/players/:playerPublicKey/stats`
  endpoints, backed by a 30-second TTL cache of on-chain data.
- **Leaderboard_UI**: The React/TypeScript frontend component that renders the
  leaderboard panel and per-player stats card.
- **Settlement**: Any contract call that finalises a game outcome and transfers funds:
  `reveal` (on a loss), `cash_out`, or `claim_winnings`.
- **Soroban_RPC**: The Stellar Soroban JSON-RPC endpoint used to read on-chain state.

---

## Requirements

### Requirement 1: PlayerStats Struct and Storage

**User Story:** As a player, I want my game statistics to be tracked on-chain, so that
I have a verifiable record of my activity.

#### Acceptance Criteria

1. THE Contract SHALL define a `PlayerStats` struct with fields: `games_played` (`u64`),
   `games_won` (`u64`), `games_lost` (`u64`), `total_wagered` (`i128`),
   `total_won` (`i128`), `best_streak` (`u32`), and `privacy_opt_out` (`bool`).
2. THE Contract SHALL store each player's `PlayerStats` under a dedicated
   `StorageKey::PlayerStats(Address)` key in persistent storage.
3. WHEN a player has no prior `PlayerStats` record, THE Contract SHALL initialise all
   numeric fields to zero and `privacy_opt_out` to `false` before applying any update.
4. THE Contract SHALL expose a `get_player_stats(player: Address) -> Option<PlayerStats>`
   entry point that returns `Some(PlayerStats)` if the player has at least one settled
   game, or `None` if no record exists.
5. THE Contract SHALL document the `PlayerStats` schema in a Rust doc comment on the
   struct definition, listing each field name, type, and semantics.

---

### Requirement 2: Atomic Stats Update on Settlement

**User Story:** As a player, I want my statistics to be updated immediately when a game
settles, so that my record is always consistent with on-chain outcomes.

#### Acceptance Criteria

1. WHEN a game settles as a win (streak > 0 after `reveal`), THE Stats_Updater SHALL
   atomically increment `games_played` by 1, increment `games_won` by 1, add the wager
   to `total_wagered`, add the net payout to `total_won`, and update `best_streak` to
   the maximum of the current `best_streak` and the new streak value.
2. WHEN a game settles as a loss (`reveal` produces streak = 0), THE Stats_Updater SHALL
   atomically increment `games_played` by 1, increment `games_lost` by 1, and add the
   wager to `total_wagered`.
3. WHEN a player calls `cash_out`, THE Stats_Updater SHALL atomically increment
   `games_played` by 1, increment `games_won` by 1, add the wager to `total_wagered`,
   and add the net payout to `total_won`.
4. WHEN a player calls `claim_winnings`, THE Stats_Updater SHALL apply the same
   increments as Criterion 3.
5. IF a settlement transaction fails (contract returns an error), THEN THE Stats_Updater
   SHALL NOT modify any `PlayerStats` field, preserving the pre-settlement state.
6. THE Stats_Updater SHALL update `PlayerStats` and the `LeaderboardIndex` entries in
   the same storage transaction so that both are always consistent.

---

### Requirement 3: Leaderboard Indexes

**User Story:** As a player, I want to see ranked leaderboards for total wagered, games
won, and best streak, so that I can compare my performance against other players.

#### Acceptance Criteria

1. THE Contract SHALL maintain three separate `LeaderboardIndex` entries in persistent
   storage, one for each `LeaderboardCategory`: `TotalWagered`, `GamesWon`, and
   `BestStreak`.
2. WHEN a player's `PlayerStats` are updated after settlement, THE Stats_Updater SHALL
   update all three `LeaderboardIndex` entries to reflect the new values, inserting or
   repositioning the player's entry in each sorted list.
3. THE Contract SHALL keep each `LeaderboardIndex` sorted in descending order of
   `value` at all times.
4. WHEN a `LeaderboardIndex` already contains `MAX_LEADERBOARD_SIZE` entries and the
   updated player value exceeds the lowest entry's value, THE Stats_Updater SHALL
   replace the lowest entry with the updated player entry.
5. WHEN a `LeaderboardIndex` already contains `MAX_LEADERBOARD_SIZE` entries and the
   updated player value does not exceed the lowest entry's value, THE Stats_Updater
   SHALL NOT add the player to that index.
6. THE Contract SHALL assign `rank` values starting at 1 (rank 1 = highest value) when
   returning `LeaderboardEntry` records from `get_leaderboard`.
7. THE Contract SHALL expose a `get_leaderboard(category: LeaderboardCategory, limit: u32)
   -> Vec<LeaderboardEntry>` entry point that returns up to `limit` entries from the
   requested `LeaderboardIndex`, ordered by descending `value`.
8. IF `limit` exceeds `MAX_LEADERBOARD_SIZE`, THEN THE Contract SHALL return
   `Error::LeaderboardLimitExceeded`.
9. IF `category` is not a valid `LeaderboardCategory` variant, THEN THE Contract SHALL
   return `Error::InvalidLeaderboardCategory`.

---

### Requirement 4: Privacy Controls

**User Story:** As a player, I want to opt out of the leaderboard, so that my address
and statistics are not publicly surfaced in ranked queries.

#### Acceptance Criteria

1. THE Contract SHALL expose a `set_privacy(player: Address, opt_out: bool)` entry
   point that requires the caller to be the `player` address (via `player.require_auth()`).
2. WHEN `set_privacy` is called with `opt_out = true`, THE Privacy_Controller SHALL set
   `PlayerStats.privacy_opt_out` to `true` and SHALL remove the player's entry from all
   three `LeaderboardIndex` entries if present.
3. WHEN `set_privacy` is called with `opt_out = false`, THE Privacy_Controller SHALL set
   `PlayerStats.privacy_opt_out` to `false` and SHALL re-insert the player into all
   three `LeaderboardIndex` entries using the player's current `PlayerStats` values.
4. WHILE `PlayerStats.privacy_opt_out` is `true`, THE Stats_Updater SHALL continue to
   update all `PlayerStats` fields on settlement but SHALL NOT insert or update the
   player's entry in any `LeaderboardIndex`.
5. THE `get_leaderboard` entry point SHALL exclude all players whose
   `PlayerStats.privacy_opt_out` is `true` from the returned `Vec<LeaderboardEntry>`.
6. THE `get_player_stats` entry point SHALL return the full `PlayerStats` record
   regardless of the `privacy_opt_out` flag, so that the player can always read their
   own stats.

---

### Requirement 5: New Error Codes

**User Story:** As a developer, I want descriptive error codes for leaderboard
operations, so that callers can handle failure cases programmatically.

#### Acceptance Criteria

1. THE Contract SHALL define `Error::PlayerNotFound` (code 60) returned by
   `get_player_stats` when no record exists for the requested address.
2. THE Contract SHALL define `Error::LeaderboardLimitExceeded` (code 61) returned by
   `get_leaderboard` when `limit` exceeds `MAX_LEADERBOARD_SIZE`.
3. THE Contract SHALL define `Error::InvalidLeaderboardCategory` (code 62) returned
   when an unrecognised `LeaderboardCategory` variant is supplied.
4. FOR EACH new error code, THE Contract SHALL include a Rust doc comment on the enum
   variant that names the entry point(s) that return it and the condition that triggers
   it.

---

### Requirement 6: Backend Read-Model API

**User Story:** As a frontend developer, I want REST endpoints for player stats and
leaderboard data, so that the UI can display them without calling the Soroban RPC
directly.

#### Acceptance Criteria

1. THE Leaderboard_API SHALL expose `GET /api/leaderboard/:category` accepting an
   optional `limit` query parameter (default 10, max 100) and returning a
   `LeaderboardReadModel[]` as JSON within 500 ms.
2. THE Leaderboard_API SHALL expose `GET /api/players/:playerPublicKey/stats` returning
   a `PlayerStatsReadModel` as JSON, or HTTP 404 with `code: 'PLAYER_NOT_FOUND'` if no
   record exists.
3. THE Leaderboard_API SHALL cache Soroban RPC responses for leaderboard queries with a
   30-second TTL and SHALL serve cached data on subsequent requests within the TTL
   window.
4. WHEN the Soroban_RPC is unreachable, THE Leaderboard_API SHALL return the last
   cached leaderboard value if available, or HTTP 503 with `code: 'RPC_UNAVAILABLE'`
   if no cached value exists.
5. IF `:category` is not one of `total_wagered`, `games_won`, or `best_streak`, THEN
   THE Leaderboard_API SHALL return HTTP 400 with `code: 'INVALID_CATEGORY'`.
6. ALL leaderboard and player-stats read endpoints SHALL be publicly accessible without
   authentication headers and SHALL NOT return HTTP 401 for unauthenticated requests.
7. ALL `i128` values (wagers, winnings) SHALL be serialised as decimal strings in JSON
   to avoid JavaScript `number` precision loss.

---

### Requirement 7: Leaderboard Stats Serialization

**User Story:** As a developer, I want to export and import leaderboard snapshots for
off-chain analytics, so that I can analyse historical rankings without querying the
contract repeatedly.

#### Acceptance Criteria

1. THE Leaderboard_API SHALL expose `GET /api/leaderboard/export` that returns all
   three leaderboard categories serialised as a single JSON object with keys
   `total_wagered`, `games_won`, and `best_streak`, each containing a
   `LeaderboardReadModel[]`.
2. THE Leaderboard_API SHALL expose `POST /api/leaderboard/import` that accepts the
   same JSON shape and stores it as a cached snapshot, overwriting the current cache.
3. FOR ALL valid leaderboard export payloads, exporting then importing then exporting
   SHALL produce an equivalent JSON object (round-trip property).
4. THE Leaderboard_API SHALL validate each imported entry against the
   `LeaderboardReadModel` schema and SHALL return HTTP 422 with a per-entry error list
   for any invalid entries.

---

### Requirement 8: Frontend Leaderboard UI

**User Story:** As a player, I want to view the leaderboard and my own stats in the
game UI, so that I can track my ranking and progress.

#### Acceptance Criteria

1. THE Leaderboard_UI SHALL display a tabbed panel with three tabs corresponding to
   `TotalWagered`, `GamesWon`, and `BestStreak` leaderboard categories.
2. WHEN a tab is selected, THE Leaderboard_UI SHALL fetch and display the top 10
   entries for that category from `GET /api/leaderboard/:category`.
3. THE Leaderboard_UI SHALL display each entry's rank, truncated player address (first
   6 and last 4 characters), and value formatted appropriately (XLM for wagered/won,
   integer for games won and streak).
4. THE Leaderboard_UI SHALL display the connected player's own `PlayerStatsReadModel`
   in a stats card below the leaderboard panel, fetched from
   `GET /api/players/:playerPublicKey/stats`.
5. THE Leaderboard_UI SHALL include a privacy toggle that calls `set_privacy` via the
   wallet when the player opts in or out of the leaderboard.
6. WHEN the Leaderboard_UI is loading data, THE Leaderboard_UI SHALL display a loading
   skeleton and SHALL NOT show stale data as current.
7. IF the leaderboard fetch fails, THE Leaderboard_UI SHALL display an error message
   and a retry button.
