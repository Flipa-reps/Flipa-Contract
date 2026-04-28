# Design Document: Player Statistics and Leaderboard

## Overview

This feature adds per-player statistics tracking and a multi-category leaderboard to
the Tossd Soroban coinflip contract. It consists of four integrated layers:

1. **Contract layer (Rust / Soroban)** — new `PlayerStats` struct, three sorted
   `LeaderboardIndex` entries in persistent storage, atomic stats updates on every
   game settlement, and three new entry points: `get_player_stats`, `get_leaderboard`,
   `set_privacy`.
2. **Backend read-model layer (Node.js / TypeScript)** — two new REST endpoints
   (`GET /api/leaderboard/:category`, `GET /api/players/:playerPublicKey/stats`) with
   a 30-second TTL cache, plus export/import endpoints for off-chain snapshots.
3. **Frontend UI layer (React / TypeScript)** — a `LeaderboardPanel` component with
   three category tabs, a `PlayerStatsCard` component, and a privacy toggle that
   invokes `set_privacy` via the wallet.
4. **Testing layer** — property-based tests using `proptest` for the Rust contract and
   `fast-check` for the TypeScript backend/frontend.

---

## Architecture

```mermaid
graph TD
    subgraph Soroban Contract (Rust / no_std)
        SU[Stats_Updater\natomic on settlement]
        PC[Privacy_Controller\nset_privacy entry point]
        PS[(PlayerStats\npersistent storage)]
        LI[(LeaderboardIndex x3\npersistent storage)]
        EP[Entry Points\nget_player_stats\nget_leaderboard\nset_privacy]
    end

    subgraph Backend (Node.js / TypeScript)
        RM[Leaderboard_API\nRead-Model Service]
        CACHE[(Leaderboard Cache\nTTL 30 s)]
        EX[Export / Import\nEndpoints]
    end

    subgraph Frontend (React / TypeScript)
        LP[LeaderboardPanel\ncomponent]
        PSC[PlayerStatsCard\ncomponent]
        PT[Privacy Toggle\nset_privacy via wallet]
    end

    subgraph Stellar Network
        RPC[Soroban RPC]
        W[Stellar Wallet\nFreighter / xBull]
    end

    SU -->|write| PS
    SU -->|write| LI
    PC -->|write| PS
    PC -->|write| LI
    EP -->|read| PS
    EP -->|read| LI
    RM -->|getLedgerEntries| RPC
    RM -->|cache hit| CACHE
    LP -->|REST| RM
    PSC -->|REST| RM
    PT -->|sign + submit| W
    W -->|set_privacy XDR| RPC
    RPC -->|on-chain state| EP
```

**Key design decisions:**

- **Atomic storage writes**: `PlayerStats` and all three `LeaderboardIndex` entries are
  written in the same Soroban host function invocation. Soroban's single-threaded
  execution model guarantees atomicity within a transaction.
- **Sorted Vec, not a map**: Each `LeaderboardIndex` is a `Vec<LeaderboardEntry>`
  maintained in descending order. Insertion is O(N) but N ≤ 100, which is acceptable
  given Soroban's instruction budget. A sorted vec avoids the overhead of a separate
  index structure.
- **Incremental update**: On each settlement the Stats_Updater finds the player's
  existing position in each index (linear scan), removes it, inserts the updated entry
  at the correct sorted position, and trims to `MAX_LEADERBOARD_SIZE`. This keeps the
  index consistent without a full rebuild.
- **Privacy as a flag + index removal**: `privacy_opt_out = true` removes the player
  from all three indexes immediately. Stats continue to accumulate so the player can
  re-join the leaderboard at any time with accurate values.
- **30-second backend cache**: Leaderboard data changes at most once per game
  settlement. A 30-second TTL is a reasonable trade-off between freshness and RPC load.
- **i128 serialised as decimal string**: All `i128` values (wagers, winnings) are
  serialised as decimal strings in JSON to avoid JavaScript `number` precision loss,
  consistent with the existing backend API convention.

---

## Components and Interfaces

### 2.1 Contract: New Types

```rust
/// Per-player statistics record.
/// Stored under StorageKey::PlayerStats(Address) in persistent storage.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlayerStats {
    pub games_played:    u64,
    pub games_won:       u64,
    pub games_lost:      u64,
    pub total_wagered:   i128,
    pub total_won:       i128,
    pub best_streak:     u32,
    pub privacy_opt_out: bool,
}

/// Selects which leaderboard index to query.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LeaderboardCategory {
    TotalWagered,
    GamesWon,
    BestStreak,
}

/// A single ranked entry in a leaderboard index.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LeaderboardEntry {
    pub player: Address,
    pub value:  i128,   // TotalWagered → total_wagered; GamesWon → games_won as i128;
                        // BestStreak → best_streak as i128
    pub rank:   u32,    // 1-based; assigned at query time
}
```

### 2.2 Contract: New StorageKey Variants

```rust
pub enum StorageKey {
    // ... existing variants ...

    /// Per-player statistics (PlayerStats), keyed by player address.
    PlayerStats(Address),

    /// Sorted leaderboard index (Vec<LeaderboardEntry>), keyed by category.
    LeaderboardIndex(LeaderboardCategory),
}
```

### 2.3 Contract: New Entry Points

```rust
impl CoinflipContract {
    /// Returns the PlayerStats for `player`, or None if no record exists.
    pub fn get_player_stats(env: Env, player: Address) -> Option<PlayerStats>;

    /// Returns up to `limit` leaderboard entries for `category`, ranked 1-based
    /// in descending order of value. Privacy-opted-out players are excluded.
    /// Errors: LeaderboardLimitExceeded if limit > MAX_LEADERBOARD_SIZE,
    ///         InvalidLeaderboardCategory if category is unrecognised.
    pub fn get_leaderboard(
        env: Env,
        category: LeaderboardCategory,
        limit: u32,
    ) -> Result<soroban_sdk::Vec<LeaderboardEntry>, Error>;

    /// Opts the caller in or out of the leaderboard.
    /// Requires player.require_auth().
    /// opt_out = true  → removes player from all three indexes.
    /// opt_out = false → re-inserts player into all three indexes.
    pub fn set_privacy(env: Env, player: Address, opt_out: bool) -> Result<(), Error>;
}
```

### 2.4 Contract: New Error Codes

```rust
pub enum Error {
    // ... existing variants ...

    /// No PlayerStats record exists for the requested address.
    /// Returned by: get_player_stats (when used as a hard error variant).
    /// Code: 60
    PlayerNotFound = 60,

    /// limit argument to get_leaderboard exceeds MAX_LEADERBOARD_SIZE.
    /// Code: 61
    LeaderboardLimitExceeded = 61,

    /// Unrecognised LeaderboardCategory variant.
    /// Code: 62
    InvalidLeaderboardCategory = 62,
}
```

### 2.5 Contract: Stats_Updater Internal Logic

The Stats_Updater is called at the end of every settlement path (`reveal` on loss,
`cash_out`, `claim_winnings`). Pseudocode:

```
fn update_player_stats(env, player, wager, payout, won, new_streak):
    stats = load_player_stats(env, player)  // returns default if missing
    stats.games_played += 1
    stats.total_wagered += wager
    if won:
        stats.games_won += 1
        stats.total_won += payout
        if new_streak > stats.best_streak:
            stats.best_streak = new_streak
    else:
        stats.games_lost += 1
    save_player_stats(env, player, stats)
    if !stats.privacy_opt_out:
        update_leaderboard_index(env, player, stats)
```

```
fn update_leaderboard_index(env, player, stats):
    for category in [TotalWagered, GamesWon, BestStreak]:
        value = match category:
            TotalWagered → stats.total_wagered
            GamesWon     → stats.games_won as i128
            BestStreak   → stats.best_streak as i128
        index = load_leaderboard_index(env, category)
        // remove existing entry for player (if any)
        index.retain(|e| e.player != player)
        // find insertion point (descending order)
        pos = index.partition_point(|e| e.value >= value)
        if pos < MAX_LEADERBOARD_SIZE:
            index.insert(pos, LeaderboardEntry { player, value, rank: 0 })
            if index.len() > MAX_LEADERBOARD_SIZE:
                index.truncate(MAX_LEADERBOARD_SIZE)
        save_leaderboard_index(env, category, index)
```

### 2.6 Backend: Leaderboard_API Routes

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| `GET` | `/api/leaderboard/:category` | None | Top N entries for a category |
| `GET` | `/api/players/:playerPublicKey/stats` | None | Per-player stats |
| `GET` | `/api/leaderboard/export` | None | Export all three categories |
| `POST` | `/api/leaderboard/import` | None | Import cached snapshot |

`:category` must be one of `total_wagered`, `games_won`, `best_streak`.

### 2.7 Backend: TypeScript Interfaces

```typescript
interface PlayerStatsReadModel {
  playerPublicKey: string;
  gamesPlayed:     number;       // u64 as number (safe: max ~1.8e19, JS safe up to 2^53)
  gamesWon:        number;
  gamesLost:       number;
  totalWagered:    string;       // i128 as decimal string
  totalWon:        string;       // i128 as decimal string
  bestStreak:      number;       // u32
  privacyOptOut:   boolean;
}

interface LeaderboardReadModel {
  rank:   number;                // u32, 1-based
  player: string;                // Stellar G-address
  value:  string;                // i128 as decimal string (all categories)
}

interface LeaderboardExport {
  total_wagered: LeaderboardReadModel[];
  games_won:     LeaderboardReadModel[];
  best_streak:   LeaderboardReadModel[];
}
```

### 2.8 Frontend: React Components

```
frontend/components/
  LeaderboardPanel.tsx      — tabbed panel (TotalWagered | GamesWon | BestStreak)
  LeaderboardPanel.module.css
  PlayerStatsCard.tsx       — per-player stats summary + privacy toggle
  PlayerStatsCard.module.css
```

`LeaderboardPanel` fetches from `GET /api/leaderboard/:category?limit=10` on tab
selection and re-fetches every 30 seconds. `PlayerStatsCard` fetches from
`GET /api/players/:playerPublicKey/stats` when the wallet is connected.

---

## Data Models

### PlayerStats (on-chain)

| Field | Type | Description |
|-------|------|-------------|
| `games_played` | `u64` | Total games settled (wins + losses) |
| `games_won` | `u64` | Games where the player won (streak > 0 at settlement) |
| `games_lost` | `u64` | Games where the player lost (streak = 0 at settlement) |
| `total_wagered` | `i128` | Sum of all wagers in stroops |
| `total_won` | `i128` | Sum of all net payouts received in stroops |
| `best_streak` | `u32` | Highest streak value ever achieved |
| `privacy_opt_out` | `bool` | `true` = excluded from leaderboard queries |

**Invariant**: `games_played == games_won + games_lost` at all times.

### LeaderboardIndex (on-chain)

Each of the three indexes is a `Vec<LeaderboardEntry>` stored under
`StorageKey::LeaderboardIndex(LeaderboardCategory)`. The vec is always sorted in
descending order of `value` and contains at most `MAX_LEADERBOARD_SIZE` entries.
The `rank` field in stored entries is always 0; ranks are assigned at query time.

### Backend SQLite Cache Schema

```sql
CREATE TABLE leaderboard_cache (
  category    TEXT NOT NULL,          -- 'total_wagered' | 'games_won' | 'best_streak'
  fetched_at  TEXT NOT NULL,          -- ISO-8601 timestamp
  payload     TEXT NOT NULL,          -- JSON array of LeaderboardReadModel
  PRIMARY KEY (category)
);

CREATE TABLE player_stats_cache (
  player_pubkey TEXT PRIMARY KEY,
  fetched_at    TEXT NOT NULL,
  payload       TEXT NOT NULL         -- JSON PlayerStatsReadModel
);
```

---

## Correctness Properties

*A property is a characteristic or behavior that should hold true across all valid
executions of a system — essentially, a formal statement about what the system should do.
Properties serve as the bridge between human-readable specifications and
machine-verifiable correctness guarantees.*

### Property 1: games_played invariant

*For any* sequence of game settlements for a single player, `games_played` must equal
`games_won + games_lost` after every settlement.

**Validates: Requirements 2.1, 2.2, 2.3, 2.4**

---

### Property 2: total_wagered monotonicity

*For any* sequence of game settlements, `total_wagered` must be non-decreasing. Each
settlement must increase `total_wagered` by exactly the wager amount for that game.

**Validates: Requirements 2.1, 2.2, 2.3, 2.4**

---

### Property 3: best_streak non-regression

*For any* sequence of game settlements, `best_streak` must be non-decreasing. It must
equal the maximum streak value observed across all winning settlements.

**Validates: Requirements 2.1, 3.3**

---

### Property 4: Leaderboard descending order

*For any* call to `get_leaderboard(category, limit)` that returns N entries, the
`value` field of entry at index `i` must be greater than or equal to the `value` at
index `i+1` for all `0 <= i < N-1`.

**Validates: Requirements 3.3, 3.6, 3.7**

---

### Property 5: Leaderboard rank assignment

*For any* call to `get_leaderboard(category, limit)` that returns N entries, the `rank`
field of the entry at index `i` must equal `i + 1` (1-based, contiguous).

**Validates: Requirements 3.6, 3.7**

---

### Property 6: Privacy exclusion

*For any* player with `privacy_opt_out = true`, no call to `get_leaderboard` for any
`LeaderboardCategory` must return an entry whose `player` field matches that player's
address.

**Validates: Requirements 4.4, 4.5**

---

### Property 7: Privacy re-inclusion

*For any* player who calls `set_privacy(opt_out = false)` after previously opting out,
a subsequent call to `get_leaderboard` must include that player's entry (provided their
stats qualify for the top `MAX_LEADERBOARD_SIZE`) with values matching their current
`PlayerStats`.

**Validates: Requirements 4.3**

---

### Property 8: Stats update idempotence under privacy

*For any* player with `privacy_opt_out = true`, settling a game must update
`PlayerStats` fields correctly (games_played, total_wagered, etc.) but must not insert
or update the player's entry in any `LeaderboardIndex`.

**Validates: Requirements 4.4**

---

### Property 9: Leaderboard size cap

*For any* sequence of settlements by distinct players, each `LeaderboardIndex` must
contain at most `MAX_LEADERBOARD_SIZE` entries at all times.

**Validates: Requirements 3.4, 3.5**

---

### Property 10: get_player_stats round-trip

*For any* player address that has at least one settled game, calling `get_player_stats`
must return `Some(stats)` where `stats` matches the values written by the most recent
`update_player_stats` call for that player.

**Validates: Requirements 1.4**

---

### Property 11: Leaderboard export round-trip

*For any* valid `LeaderboardExport` payload, exporting then importing then exporting
must produce a JSON object equivalent to the original export (same entries, same order,
same field values).

**Validates: Requirements 7.3**

---

### Property 12: Backend cache TTL consistency

*For any* sequence of `GET /api/leaderboard/:category` calls within a 30-second window,
all responses must return the same `LeaderboardReadModel[]` snapshot (cache hit). After
the TTL expires, the next call must fetch fresh state from the Soroban RPC.

**Validates: Requirements 6.3, 6.4**

---

### Property 13: i128 decimal string serialisation

*For any* `PlayerStatsReadModel` or `LeaderboardReadModel` returned by the backend,
all `i128`-derived fields (`totalWagered`, `totalWon`, `value`) must be serialised as
decimal strings that round-trip through `BigInt(str)` without precision loss.

**Validates: Requirements 6.7**

---

### Property 14: Leaderboard limit enforcement

*For any* call to `get_leaderboard(category, limit)` where `limit > MAX_LEADERBOARD_SIZE`,
the contract must return `Error::LeaderboardLimitExceeded` (code 61) without reading
or modifying any storage entry.

**Validates: Requirements 3.8, 5.2**

---

### Property 15: Settlement atomicity

*For any* settlement that returns an error, the `PlayerStats` and all three
`LeaderboardIndex` entries must be identical to their pre-settlement state (no partial
writes).

**Validates: Requirements 2.5, 2.6**

---

## Error Handling

| Scenario | Component | Error / HTTP | Code | Behaviour |
|----------|-----------|-------------|------|-----------|
| `get_player_stats` for unknown player | Contract | `Error::PlayerNotFound` | 60 | Return `None` (soft) or error (hard) |
| `get_leaderboard` limit > MAX | Contract | `Error::LeaderboardLimitExceeded` | 61 | Reject before reading storage |
| Invalid `LeaderboardCategory` | Contract | `Error::InvalidLeaderboardCategory` | 62 | Reject before reading storage |
| `set_privacy` called by non-player | Contract | `Error::Unauthorized` | 30 | Reject; no state change |
| Settlement fails mid-execution | Contract | existing error codes | — | No `PlayerStats` or index mutation |
| Invalid `:category` path param | Leaderboard_API | HTTP 400 | `INVALID_CATEGORY` | Reject before RPC call |
| Player not found | Leaderboard_API | HTTP 404 | `PLAYER_NOT_FOUND` | Return 404 |
| Soroban RPC unreachable | Leaderboard_API | HTTP 503 | `RPC_UNAVAILABLE` | Return last cached value or 503 |
| Import schema validation failure | Leaderboard_API | HTTP 422 | — | Per-entry error list |
| Leaderboard fetch fails in UI | Leaderboard_UI | — | — | Show error message + retry button |

---

## Testing Strategy

### Dual Testing Approach

Both unit tests and property-based tests are required. Unit tests cover specific
examples, integration points, and error conditions. Property-based tests verify
universal correctness across all valid inputs.

### Property-Based Testing — Contract (Rust)

**Library:** [`proptest`](https://github.com/proptest-rs/proptest). Minimum **100
iterations** per property test.

Each property test must include a comment referencing the design property:
```rust
// Feature: player-statistics-leaderboard, Property N: <property_text>
```

**Contract property tests** (`contract/src/player_stats_tests.rs`):
- Properties 1, 2, 3 — stats field invariants
- Properties 4, 5 — leaderboard ordering and rank assignment
- Properties 6, 7, 8 — privacy controls
- Properties 9 — leaderboard size cap
- Properties 10 — get_player_stats round-trip
- Properties 14, 15 — limit enforcement and settlement atomicity

```rust
// Example: Property 1 — games_played invariant
proptest! {
    #[test]
    fn prop_games_played_invariant(
        wins in 0u64..1000,
        losses in 0u64..1000,
    ) {
        // Feature: player-statistics-leaderboard, Property 1: games_played invariant
        let mut stats = PlayerStats::default();
        for _ in 0..wins   { apply_win(&mut stats, 100, 150, 1); }
        for _ in 0..losses { apply_loss(&mut stats, 100); }
        prop_assert_eq!(stats.games_played, stats.games_won + stats.games_lost);
    }
}
```

### Property-Based Testing — Backend / Frontend (TypeScript)

**Library:** [`fast-check`](https://github.com/dubzzz/fast-check). Minimum **100
iterations** per property test.

Each property test must include a comment referencing the design property:
```typescript
// Feature: player-statistics-leaderboard, Property N: <property_text>
```

**Backend property tests** (`backend/src/__tests__/properties/`):
- `leaderboardApi.property.test.ts` — Properties 11, 12, 13
- `leaderboardApi.property.test.ts` — Property 14 (limit enforcement via API)

**Frontend property tests** (`frontend/tests/properties/`):
- `leaderboardPanel.property.test.ts` — Properties 4, 5 (rendering order and rank)

```typescript
// Example: Property 11 — leaderboard export round-trip
test('Property 11: leaderboard export round-trip', () => {
  // Feature: player-statistics-leaderboard, Property 11: leaderboard export round-trip
  fc.assert(
    fc.property(
      fc.array(fc.record({
        rank:   fc.integer({ min: 1, max: 100 }),
        player: fc.string({ minLength: 56, maxLength: 56 }),
        value:  fc.bigInt({ min: 0n }).map(String),
      }), { maxLength: 100 }),
      (entries) => {
        const exported: LeaderboardExport = {
          total_wagered: entries,
          games_won:     entries,
          best_streak:   entries,
        };
        const imported = importSnapshot(exported);
        const reExported = exportSnapshot(imported);
        return JSON.stringify(reExported) === JSON.stringify(exported);
      }
    ),
    { numRuns: 100 }
  );
});
```

### Unit Tests

**Contract unit tests** (`contract/src/player_stats_tests.rs`):
- Stats initialised to zero on first settlement
- Win settlement increments correct fields
- Loss settlement increments correct fields
- `best_streak` updated only when new streak exceeds current best
- `set_privacy(true)` removes player from all three indexes
- `set_privacy(false)` re-inserts player with current stats
- `get_leaderboard` returns entries in descending order
- `get_leaderboard` with limit > MAX returns `LeaderboardLimitExceeded`
- `get_player_stats` returns `None` for unknown player

**Backend unit tests** (`backend/src/__tests__/unit/leaderboard.test.ts`):
- `GET /api/leaderboard/total_wagered` returns correct shape
- `GET /api/players/:pk/stats` returns 404 for unknown player
- Invalid category returns HTTP 400
- Cache hit returns cached data within TTL
- RPC failure returns last cached value

**Frontend unit tests** (`frontend/tests/LeaderboardPanel.test.tsx`):
- Tab switch triggers correct category fetch
- Loading skeleton shown while fetching
- Error message and retry button shown on fetch failure
- Privacy toggle calls `set_privacy` with correct argument

### Test Configuration

```rust
// proptest example configuration
proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]
    // ... tests ...
}
```

```typescript
// vitest.config.ts (backend)
export default {
  test: {
    globals: true,
    environment: 'node',
    include: ['src/__tests__/**/*.test.ts'],
  }
}
```

---

## Dependencies

| Dependency | Layer | Purpose |
|------------|-------|---------|
| `soroban-sdk` | Contract | Storage, types, auth |
| `proptest` | Contract tests | Property-based testing |
| `fast-check` | Backend/Frontend tests | Property-based testing |
| `vitest` | Backend/Frontend tests | Test runner |
| `@stellar/stellar-sdk` | Backend | RPC calls, XDR decoding |
| `recharts` (existing) | Frontend | Chart rendering (if stats charts added) |
