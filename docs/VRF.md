# VRF Integration

Tossd uses a Verifiable Random Function (VRF) as a third independent randomness
source, layered on top of the existing commit-reveal scheme.  The result is a
three-party randomness protocol where **player**, **contract**, and **oracle**
must all cooperate to predict the outcome.

## How it works

### Three-party randomness

```
outcome = SHA-256(player_secret || (contract_random XOR vrf_output))
```

| Contribution      | Source                                    | Committed when?         |
|-------------------|-------------------------------------------|-------------------------|
| `player_secret`   | Player's off-chain random value           | `start_game` (as hash)  |
| `contract_random` | SHA-256(ledger_sequence) at game start    | `start_game`            |
| `vrf_output`      | SHA-256(oracle_proof) at reveal time      | `reveal`                |

No single party can bias the outcome:
- The player cannot change their secret after committing.
- The contract cannot change the ledger sequence after the game starts.
- The oracle cannot change its proof without invalidating the Ed25519 signature.

### VRF input

At `start_game`, the contract computes:

```
vrf_input = SHA-256(commitment || contract_random)
```

This is the message the oracle must sign.  Because `contract_random` is already
fixed on-chain before the oracle signs, the oracle cannot pre-compute a signature
that biases the outcome.

### VRF proof

The oracle signs `vrf_input` with its Ed25519 private key.  The resulting 64-byte
signature is the VRF proof.  The player submits this proof in the `reveal` call.

### Verification

In `reveal`, the contract:

1. Calls `verify_vrf_proof(oracle_pk, vrf_input, vrf_proof)` — this calls
   `env.crypto().ed25519_verify` which panics (aborting the transaction) if the
   signature is invalid.
2. Derives `vrf_output = SHA-256(vrf_proof)`.
3. XORs `vrf_output` into `contract_random` to produce the aggregated randomness.
4. Derives the outcome from `SHA-256(player_secret || aggregated)`.

The VRF proof is stored in `HistoryEntry.vrf_proof` so any past game can be
independently re-verified off-chain.

## Fallback: no-oracle mode

When `oracle_vrf_pk` is all-zero bytes (`[0u8; 32]`), VRF verification is
skipped entirely.  The player submits an all-zero proof and the game proceeds
using only the commit-reveal randomness.

This is the **backward-compatible fallback** for deployments without an oracle.
In production, always set a real Ed25519 public key.

```rust
// No-oracle deployment
let zero_pk = BytesN::from_array(&env, &[0u8; 32]);
client.initialize(&admin, &treasury, &token, &fee_bps, &min_wager, &max_wager, &zero_pk);

// Reveal with zero proof (no oracle required)
let zero_proof = BytesN::from_array(&env, &[0u8; 64]);
client.reveal(&player, &secret, &zero_proof);
```

## Oracle integration

### Off-chain oracle flow

1. Oracle monitors `start_game` events for `vrf_input` values.
2. Oracle signs each `vrf_input` with its Ed25519 private key.
3. Player retrieves the signature (VRF proof) from the oracle API.
4. Player submits `reveal(secret, vrf_proof)`.

### Key management

- The oracle's Ed25519 public key is stored in `ContractConfig.oracle_vrf_pk`.
- It is set at `initialize` time and can be rotated by the admin via `update_config`.
- The private key must be kept in a secure HSM; compromise allows the oracle to
  bias outcomes.

## Security properties

| Property        | Guarantee                                                        |
|-----------------|------------------------------------------------------------------|
| Unpredictability | Outcome cannot be predicted before `reveal` without knowing all three secrets |
| Verifiability   | Anyone can re-verify the oracle's contribution using the stored `vrf_proof` and the public key from `ContractConfig` |
| Fallback safety | Zero public key disables oracle without breaking the commit-reveal guarantee |
| Replay safety   | `vrf_input` binds the proof to a specific game via `commitment || contract_random` |

## Relevant code

| Symbol                  | File                        | Description                              |
|-------------------------|-----------------------------|------------------------------------------|
| `verify_vrf_proof`      | `contract/src/lib.rs:1418`  | Ed25519 verification with zero-pk bypass |
| `generate_outcome`      | `contract/src/lib.rs:1608`  | Outcome derivation with VRF XOR          |
| `ContractConfig.oracle_vrf_pk` | `contract/src/lib.rs:543` | Oracle public key storage           |
| `GameState.vrf_input`   | `contract/src/lib.rs:500`   | Per-game VRF input (committed at start)  |
| `HistoryEntry.vrf_proof`| `contract/src/lib.rs:617`   | Stored proof for off-chain verification  |
| `vrf_tests.rs`          | `contract/src/vrf_tests.rs` | Unit and integration tests               |
