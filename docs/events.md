# Tossd Contract Event Schema

All state-changing contract functions emit a Soroban event atomically with the
state write.  Events are never emitted on guard failures — their presence
implies the corresponding state change succeeded.

## Topic Convention

Every event uses a two-element topic tuple:

```
(Symbol("tossd"), Symbol("<action>"))
```

Off-chain indexers can subscribe to all Tossd events by filtering on the first
topic (`"tossd"`) and further narrow by action using the second topic.

---

## Events

### `initialized` — Contract initialised

Emitted once by `initialize`.

| Field       | Type      | Description                          |
|-------------|-----------|--------------------------------------|
| `admin`     | `Address` | Administrator address                |
| `treasury`  | `Address` | Fee collection address               |
| `token`     | `Address` | SAC token address                    |
| `fee_bps`   | `u32`     | Protocol fee in basis points (2–5 %) |
| `min_wager` | `i128`    | Inclusive minimum wager in stroops   |
| `max_wager` | `i128`    | Inclusive maximum wager in stroops   |

Topics: `("tossd", "init")`

---

### `started` — Game created

Emitted by `start_game` on success.

| Field        | Type         | Description                              |
|--------------|--------------|------------------------------------------|
| `player`     | `Address`    | Player address                           |
| `side`       | `Side`       | Chosen side (`Heads` or `Tails`)         |
| `wager`      | `i128`       | Wager amount in stroops                  |
| `commitment` | `BytesN<32>` | SHA-256 hash of the player's secret      |
| `ledger`     | `u32`        | Ledger sequence at game creation         |

Topics: `("tossd", "started")`

---

### `revealed` — Outcome determined

Emitted by `reveal` on both win and loss paths.

| Field     | Type      | Description                                      |
|-----------|-----------|--------------------------------------------------|
| `player`  | `Address` | Player address                                   |
| `won`     | `bool`    | `true` if the player won                         |
| `streak`  | `u32`     | Post-reveal streak (1+ on win, 0 on loss)        |
| `outcome` | `Side`    | Derived outcome (`Heads` or `Tails`)             |

Topics: `("tossd", "revealed")`

Notes:
- On a win, `streak` is the incremented value (≥ 1).
- On a loss, `streak` is always `0` and the game record is deleted.

---

### `settled` — Game settled

Emitted by `cash_out` and `claim_winnings` on success.

| Field    | Type      | Description                                                  |
|----------|-----------|--------------------------------------------------------------|
| `player` | `Address` | Player address                                               |
| `payout` | `i128`    | Net payout to the player in stroops (gross minus fee)        |
| `fee`    | `i128`    | Protocol fee collected in stroops                            |
| `streak` | `u32`     | Streak level at settlement                                   |
| `method` | `Symbol`  | `"cash_out"` or `"claim_winnings"`                           |

Topics: `("tossd", "settled")`

Notes:
- `payout + fee == gross_payout` where `gross = wager × multiplier / 10_000`.
- `claim_winnings` additionally emits SAC token transfer events after this event.

---

### `continued` — Streak continued

Emitted by `continue_streak` on success.

| Field            | Type         | Description                              |
|------------------|--------------|------------------------------------------|
| `player`         | `Address`    | Player address                           |
| `streak`         | `u32`        | Current streak (preserved, not reset)    |
| `new_commitment` | `BytesN<32>` | New commitment for the next round        |

Topics: `("tossd", "continued")`

---

### `reclaimed` — Timed-out wager reclaimed

Emitted by `reclaim_wager` on success.

| Field    | Type      | Description                                      |
|----------|-----------|--------------------------------------------------|
| `player` | `Address` | Player address                                   |
| `wager`  | `i128`    | Reclaimed wager amount in stroops                |
| `ledger` | `u32`     | Ledger sequence at original game creation        |

Topics: `("tossd", "reclaimed")`

---

### `admin` — Admin configuration change

Emitted by `set_paused`, `set_treasury`, `set_wager_limits`, and `set_fee`.

| Field    | Type      | Description                                                        |
|----------|-----------|--------------------------------------------------------------------|
| `action` | `Symbol`  | One of `"set_paused"`, `"set_treasury"`, `"set_wager_limits"`, `"set_fee"` |
| `admin`  | `Address` | Admin address that performed the action                            |

Topics: `("tossd", "admin")`

Notes:
- The new configuration values are not included in the event payload; read
  `get_config()` after the event to obtain the updated state.
- Events are only emitted on success; unauthorized calls produce no event.

---

## Indexer Notes

- All events are emitted **after** state is written, so reading contract state
  in the same ledger as the event will always reflect the updated values.
- Events are never emitted on guard failures (error returns).
- The `symbol_short!` macro limits symbols to 9 characters; all topic symbols
  used here are within that limit.
- For `claim_winnings`, SAC token transfer events (`transfer`) will appear
  after the `settled` event in the same transaction's event log.
