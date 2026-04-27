# Fraud Detection and Prevention System

The contract implements three complementary fraud-prevention layers:
rate limiting, anomaly detection, and admin alerts via on-chain events.

## Rate limiting

Every `start_game` call passes through `check_rate_limit` before any other
logic runs.

| Constant                    | Value | Meaning                              |
|-----------------------------|-------|--------------------------------------|
| `RATE_LIMIT_MAX_GAMES`      | 10    | Max games per window                 |
| `RATE_LIMIT_WINDOW_LEDGERS` | 60    | Sliding window (~5 minutes at 5 s/ledger) |

**Mechanism:**
1. Load `PlayerRateLimit` from `StorageKey::PlayerRateLimit(player)`.
2. If the window has expired (`current_ledger - window_start ≥ 60`), reset it.
3. Increment `games_in_window`.
4. If `games_in_window > 10`, call `set_fraud_flag(player, "rate_limit")` and
   return `Err(Error::ContractPaused)` — the game is rejected.

State is stored per-player in persistent storage and extended on every access.

## Anomaly detection

`check_anomaly` is called after every game outcome (win or loss) and inspects
the player's cumulative `PlayerStats`.

| Constant                        | Value | Trigger                                  |
|---------------------------------|-------|------------------------------------------|
| `ANOMALY_WIN_STREAK_THRESHOLD`  | 8     | Current win streak ≥ 8 consecutive wins  |
| `ANOMALY_LOSS_STREAK_THRESHOLD` | 20    | 20+ losses with zero wins                |

Anomaly detection **does not block** the game — it only sets a flag for admin
review. This avoids false positives disrupting legitimate players.

## Admin alert system

`set_fraud_flag` persists a `FraudFlag` and emits an on-chain event:

```
event topic:  (Symbol("tossd"), Symbol("fraud_flag"))
event data:   (player: Address, reason: Symbol)
```

Reason codes:
- `Symbol("rate_limit")` — rate limit exceeded
- `Symbol("win_streak")` — unusually long win streak
- `Symbol("loss_streak")` — unusually long loss streak with no wins

Off-chain monitoring services can subscribe to these events and trigger
manual review workflows.

## Admin entry points

### `get_fraud_flag(player) → Option<FraudFlag>`

Returns the current flag for `player`, or `None`. No auth required — readable
by anyone for transparency.

```rust
let flag = client.get_fraud_flag(&player);
// FraudFlag { flagged_at: 12345, reason: Symbol("rate_limit") }
```

### `clear_fraud_flag(admin, player) → Result<(), Error>`

Clears the flag after admin review. Requires admin auth.

```rust
client.clear_fraud_flag(&admin, &player);
```

## FraudFlag struct

| Field        | Type     | Description                          |
|--------------|----------|--------------------------------------|
| `flagged_at` | `u32`    | Ledger sequence when flag was set    |
| `reason`     | `Symbol` | Short reason code (see above)        |

## Privacy considerations

- Flags are keyed by player address, which is already public on Stellar.
- No personal data beyond the on-chain address is stored.
- Flags can be cleared by the admin after review.
- Rate-limit state (`PlayerRateLimit`) stores only ledger sequence and a count,
  not game content.

## Relevant code

| Symbol                      | Location   | Description                          |
|-----------------------------|------------|--------------------------------------|
| `check_rate_limit`          | `lib.rs`   | Rate limit enforcement (called in `start_game`) |
| `check_anomaly`             | `lib.rs`   | Anomaly detection (called after reveal) |
| `set_fraud_flag`            | `lib.rs`   | Flag + event emission                |
| `get_fraud_flag`            | `lib.rs`   | Admin query                          |
| `clear_fraud_flag`          | `lib.rs`   | Admin clear                          |
| `PlayerRateLimit`           | `lib.rs`   | Per-player rate-limit state          |
| `FraudFlag`                 | `lib.rs`   | Fraud flag struct                    |
| `RATE_LIMIT_MAX_GAMES`      | `lib.rs`   | 10 games per window                  |
| `RATE_LIMIT_WINDOW_LEDGERS` | `lib.rs`   | 60-ledger window                     |
| `ANOMALY_WIN_STREAK_THRESHOLD` | `lib.rs` | 8 consecutive wins                  |
| `ANOMALY_LOSS_STREAK_THRESHOLD`| `lib.rs` | 20 losses with no wins              |
