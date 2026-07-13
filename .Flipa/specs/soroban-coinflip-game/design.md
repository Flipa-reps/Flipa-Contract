# Design Document: Soroban Coinflip Game — Backend API Spec

## Overview

This feature defines the backend (serverless) API contract for the Flipa coinflip game
session orchestration layer. It sits between the browser wallet and the Soroban smart
contract, providing three responsibilities:

1. **Transaction Preparation** — validates inputs, builds and simulates unsigned
   Soroban transactions, and returns XDR envelopes for the wallet to sign and submit.
2. **Read Models** — serves game state, player history, contract config, and reserve
   health without requiring the frontend to call the Soroban RPC directly.
3. **Session Orchestration** — tracks the commit-reveal lifecycle, enforces trust
   boundaries, and surfaces actionable errors before they reach the contract.

The API is intentionally minimal: it never holds private keys, never submits
transactions on behalf of players, and never stores wager funds. All financial
operations are executed by the player's wallet against the Soroban contract. The
backend is an untrusted relay; the contract is the source of truth; the wallet holds
the keys.

---

## Architecture

```mermaid
graph TD
    subgraph Browser
        W[Stellar Wallet\nFreighter / xBull]
        FE[React Frontend\nVite / TypeScript]
    end

    subgraph Backend API (Serverless / Edge)
        GW[API Gateway\nHTTP + Auth]
        TX[Transaction Builder\nService]
        RM[Read Model\nService]
        SO[Session Orchestrator]
        SV[Request Authenticator\nEd25519 / HMAC]
    end

    subgraph Stellar Network
        RPC[Soroban RPC]
        SC[Flipa Contract]
    end

    subgraph Persistence
        DB[(Session Store\nKV / SQLite)]
        CACHE[(Read Cache\nTTL 5 s)]
    end

    FE -->|REST + X-Wallet-Pubkey header| GW
    GW -->|verify request signature| SV
    GW --> TX
    GW --> RM
    GW --> SO
    TX -->|simulateTransaction XDR| RPC
    RM -->|getLedgerEntries| RPC
    RM -->|cache hit| CACHE
    SO -->|persist session phase| DB
    W -->|sign XDR envelope| FE
    FE -->|submitTransaction| SC
    SC -->|events / state| RPC
```

**Key design decisions:**

- **Stateless transaction preparation**: The API builds unsigned XDR and returns it to
  the wallet. The wallet signs and submits. The API never touches private keys.
- **Wallet authentication**: Each mutating request carries the player's Stellar public
  key (`X-Wallet-Pubkey`) and a short-lived HMAC-SHA256 signature over
  `(method + path + nonce + timestamp)`. No JWT issuance; the wallet is the identity
  provider. Read endpoints are unauthenticated (public key is a path parameter).
- **Read-through cache**: Contract state reads are cached with a 5-second TTL to avoid
  hammering the Soroban RPC on every page render.
- **Serverless-first**: All handlers are stateless functions. Session state is stored
  in a KV store (Cloudflare KV, Upstash Redis, or SQLite for local dev).
- **Minimal surface**: Only endpoints required by the game flow are exposed. Admin
  mutation endpoints belong to the admin-dashboard-metrics API.
- **Trust boundary**: The backend validates inputs and simulates transactions but
  cannot forge outcomes. The contract enforces all game rules on-chain.

---

## Components and Interfaces

### 2.1 Transaction Builder Service

```typescript
interface BuildTxRequest {
  playerPublicKey: string;   // Stellar G-address (StrKey encoded)
  operation: ContractOperation;
  params: OperationParams;
}

interface BuildTxResponse {
  xdr: string;               // base64-encoded unsigned TransactionEnvelope XDR
  simulationFee: string;     // stroops, as string (bigint-safe)
  expiresAt: string;         // ISO-8601; transaction valid until this ledger
  warnings: string[];        // non-fatal issues (e.g. low reserve)
}

type ContractOperation =
  | 'start_game'
  | 'reveal'
  | 'cash_out'
  | 'continue_streak'
  | 'reclaim_wager';

type OperationParams =
  | StartGameParams
  | RevealParams
  | CashOutParams
  | ContinueStreakParams
  | ReclaimWagerParams;
```

### 2.2 Session Orchestrator

```typescript
interface SessionRecord {
  playerPublicKey: string;
  phase: 'idle' | 'committed' | 'revealed' | 'completed';
  commitment: string | null;   // hex-encoded SHA-256
  startLedger: number | null;
  wagerStroops: string | null; // bigint as string
  side: 'heads' | 'tails' | null;
  updatedAt: string;           // ISO-8601
}
```

The orchestrator mirrors the on-chain `GamePhase` in the session store so the frontend
can poll a single endpoint for lifecycle state without hitting the RPC on every render.
It is updated optimistically on transaction submission and reconciled against on-chain
state on the next read-model fetch.

### 2.3 Read Model Service

```typescript
interface ReadModelService {
  getGameState(playerPublicKey: string): Promise<GameStateReadModel | null>;
  getPlayerHistory(playerPublicKey: string, limit?: number): Promise<HistoryEntryReadModel[]>;
  getContractConfig(): Promise<ContractConfigReadModel>;
  getReserveHealth(): Promise<ReserveHealthReadModel>;
}
```

All reads go through the 5-second TTL cache. Cache misses call `getLedgerEntries` on
the Soroban RPC and populate the cache before returning.

### 2.4 API Gateway (HTTP Routes)

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| `POST` | `/api/tx/prepare` | Required | Build unsigned XDR for any contract operation |
| `POST` | `/api/tx/submit` | Required | Relay a signed XDR to the Soroban RPC |
| `GET` | `/api/game/:playerPublicKey` | None | Current game state read model |
| `GET` | `/api/history/:playerPublicKey` | None | Player game history |
| `GET` | `/api/config` | None | Contract config read model |
| `GET` | `/api/reserve-health` | None | Reserve health snapshot |
| `GET` | `/api/session/:playerPublicKey` | Required | Session orchestration state |
| `POST` | `/api/session/:playerPublicKey/sync` | Required | Force reconcile session with on-chain state |

Auth = `X-Wallet-Pubkey` + `X-Request-Sig` + `X-Request-Nonce` + `X-Request-Timestamp` headers.

---

## Data Models

### StartGameParams

```typescript
interface StartGameParams {
  wagerStroops: string;        // i128 as string
  side: 'heads' | 'tails';
  commitment: string;          // hex-encoded SHA-256(secret), 64 chars
  sideBet?: SideBetParams;
}

interface SideBetParams {
  type: 'exact_streak' | 'sequence';
  target: number;              // u32
  amountStroops: string;       // i128 as string
}
```

### RevealParams

```typescript
interface RevealParams {
  secret: string;              // hex-encoded raw secret bytes
  vrfProof: string;            // hex-encoded 64-byte Ed25519 signature
}
```

### CashOutParams / ContinueStreakParams / ReclaimWagerParams

```typescript
interface CashOutParams {}     // no additional params; player identified by pubkey

interface ContinueStreakParams {
  newCommitment: string;       // hex-encoded SHA-256(new_secret), 64 chars
  sideBet?: SideBetParams;
}

interface ReclaimWagerParams {}
```

### GameStateReadModel

```typescript
interface GameStateReadModel {
  playerPublicKey: string;
  wagerStroops: string;        // i128 as string
  side: 'heads' | 'tails';
  streak: number;              // u32
  phase: 'committed' | 'revealed' | 'completed';
  startLedger: number;
  feeBps: number;
  sideBet: SideBetReadModel | null;
  commitment: string;          // hex
  contractRandom: string;      // hex
  token: string;               // Stellar address
}

interface SideBetReadModel {
  type: 'exact_streak' | 'sequence';
  target: number;
  amountStroops: string;
}
```

### HistoryEntryReadModel

```typescript
interface HistoryEntryReadModel {
  wagerStroops: string;
  side: 'heads' | 'tails';
  outcome: 'heads' | 'tails';
  won: boolean;
  streak: number;
  commitment: string;          // hex
  secret: string;              // hex
  contractRandom: string;      // hex
  payoutStroops: string;
  ledger: number;
  vrfProof: string;            // hex, 64 bytes
}
```

### ContractConfigReadModel

```typescript
interface ContractConfigReadModel {
  feeBps: number;
  minWagerStroops: string;
  maxWagerStroops: string;
  paused: boolean;
  shutdownMode: boolean;
  minReserveThreshold: string; // i128 as string
  multipliers: MultiplierConfigReadModel;
  token: string;               // Stellar address
}

interface MultiplierConfigReadModel {
  streak1: number;             // basis points
  streak2: number;
  streak3: number;
  streak4Plus: number;
}
```

### ReserveHealthReadModel

```typescript
interface ReserveHealthReadModel {
  reserveBalance: string;          // i128 as string
  maxWorstCasePayout: string;      // i128 as string
  coverageRatio: string;           // integer, floored
  dynamicMaxWager: string;         // i128 as string
  tier: 'healthy' | 'moderate' | 'low' | 'critical';
}
```

### API Error Shape

```typescript
interface ApiError {
  code: string;                // machine-readable, e.g. 'WAGER_BELOW_MINIMUM'
  message: string;             // human-readable
  contractErrorCode?: number;  // Soroban error u32 if applicable
  details?: Record<string, unknown>;
}
```

### Request Authentication Headers

```typescript
interface AuthHeaders {
  'X-Wallet-Pubkey': string;   // Stellar G-address
  'X-Request-Sig': string;     // hex HMAC-SHA256(secret, canonical_string)
  'X-Request-Nonce': string;   // UUID v4, single-use
  'X-Request-Timestamp': string; // Unix seconds as string; rejected if > 30 s old
}
```

---

## Correctness Properties

*A property is a characteristic or behavior that should hold true across all valid
executions of a system — essentially, a formal statement about what the system should do.
Properties serve as the bridge between human-readable specifications and
machine-verifiable correctness guarantees.*

### Property 1: Commitment integrity

*For any* valid `StartGameParams` where `commitment` is a 64-character lowercase hex
string, the Transaction Builder must include that exact commitment bytes in the
`start_game` contract invocation XDR without modification, truncation, or re-encoding.

**Validates: Requirements 1.1, 1.2**

---

### Property 2: XDR round-trip fidelity

*For any* `BuildTxResponse.xdr` returned by `POST /api/tx/prepare`, decoding the
base64 XDR must produce a valid `TransactionEnvelope` whose source account matches
`playerPublicKey` and whose single operation invokes the correct contract function
with the correct arguments.

**Validates: Requirements 1.1, 1.3**

---

### Property 3: Wager bounds enforcement

*For any* `StartGameParams.wagerStroops` value that is less than
`ContractConfig.minWagerStroops` or greater than `ContractConfig.dynamicMaxWager`,
`POST /api/tx/prepare` must return HTTP 422 with `code: 'WAGER_BELOW_MINIMUM'` or
`code: 'WAGER_ABOVE_MAXIMUM'` respectively, before any RPC simulation is attempted.

**Validates: Requirements 1.4, 2.1**

---

### Property 4: Commitment entropy validation

*For any* `commitment` string where all 32 bytes are identical (e.g. all zeros, all
`0xff`), `POST /api/tx/prepare` with `operation: 'start_game'` must return HTTP 422
with `code: 'WEAK_COMMITMENT'` without forwarding the request to the RPC.

**Validates: Requirements 1.5**

---

### Property 5: Nonce uniqueness enforcement

*For any* two requests that share the same `X-Request-Nonce` value, the second request
must be rejected with HTTP 401 and `code: 'NONCE_REUSED'`, regardless of whether the
first request succeeded or failed.

**Validates: Requirements 3.1, 3.2**

---

### Property 6: Timestamp window enforcement

*For any* request where `X-Request-Timestamp` is more than 30 seconds in the past or
more than 5 seconds in the future (relative to server time), the API must return HTTP
401 with `code: 'REQUEST_EXPIRED'`.

**Validates: Requirements 3.1, 3.3**

---

### Property 7: Read model cache consistency

*For any* sequence of `GET /api/game/:playerPublicKey` calls within a 5-second window,
all responses must return the same `GameStateReadModel` snapshot (cache hit). After the
TTL expires, the next call must fetch fresh state from the RPC.

**Validates: Requirements 2.2, 2.3**

---

### Property 8: Phase transition monotonicity

*For any* `SessionRecord`, the `phase` field must only advance in the order
`idle → committed → revealed → completed` and must never regress to an earlier phase
during normal operation (reclaim resets to `idle`).

**Validates: Requirements 4.1, 4.2**

---

### Property 9: History ordering

*For any* `GET /api/history/:playerPublicKey` response, the returned
`HistoryEntryReadModel[]` must be ordered by `ledger` descending (most recent first)
and must contain at most 100 entries.

**Validates: Requirements 2.4**

---

### Property 10: Reserve health tier correctness

*For any* `ReserveHealthReadModel`, the `tier` field must satisfy:
- `'healthy'` when `coverageRatio >= 10`
- `'moderate'` when `5 <= coverageRatio < 10`
- `'low'` when `2 <= coverageRatio < 5`
- `'critical'` when `coverageRatio < 2`

**Validates: Requirements 2.5**

---

### Property 11: Submit relay fidelity

*For any* signed XDR submitted to `POST /api/tx/submit`, the API must forward the XDR
to the Soroban RPC unchanged and return the RPC's `txHash` and `status` verbatim. The
API must not modify, re-sign, or re-encode the envelope.

**Validates: Requirements 1.6**

---

### Property 12: Contract error passthrough

*For any* Soroban RPC simulation or submission that returns a contract error (non-zero
`u32` error code), the API must map it to the corresponding `ApiError.code` string and
include the original `contractErrorCode` in the response body, returning HTTP 422.

**Validates: Requirements 5.1, 5.2**

---

### Property 13: Paused contract rejection

*For any* `POST /api/tx/prepare` with `operation: 'start_game'` or
`operation: 'continue_streak'` when `ContractConfigReadModel.paused` or
`ContractConfigReadModel.shutdownMode` is `true`, the API must return HTTP 422 with
`code: 'CONTRACT_PAUSED'` or `code: 'CONTRACT_SHUTDOWN'` without simulating the
transaction.

**Validates: Requirements 1.4, 5.3**

---

### Property 14: Unauthenticated read access

*For any* `GET` request to `/api/game/:pk`, `/api/history/:pk`, `/api/config`, or
`/api/reserve-health` that omits auth headers, the API must return a valid response
(HTTP 200 or 404) and must not return HTTP 401.

**Validates: Requirements 2.6**

---

### Property 15: Authenticated mutation requirement

*For any* `POST` request to `/api/tx/prepare`, `/api/tx/submit`, or
`/api/session/:pk/sync` that omits or provides invalid auth headers, the API must
return HTTP 401 with `code: 'UNAUTHORIZED'` and must not execute the operation.

**Validates: Requirements 3.4**

---

## Error Handling

| Scenario | Component | HTTP Status | `code` | Behaviour |
|----------|-----------|-------------|--------|-----------|
| Wager below minimum | Transaction Builder | 422 | `WAGER_BELOW_MINIMUM` | Reject before RPC simulation |
| Wager above maximum | Transaction Builder | 422 | `WAGER_ABOVE_MAXIMUM` | Reject before RPC simulation |
| Weak commitment (all-same-byte) | Transaction Builder | 422 | `WEAK_COMMITMENT` | Reject before RPC simulation |
| Active game exists | Transaction Builder | 422 | `ACTIVE_GAME_EXISTS` | Reject after on-chain check |
| Insufficient reserves | Transaction Builder | 422 | `INSUFFICIENT_RESERVES` | Reject after reserve health check |
| Contract paused | Transaction Builder | 422 | `CONTRACT_PAUSED` | Reject before RPC simulation |
| Contract shutdown | Transaction Builder | 422 | `CONTRACT_SHUTDOWN` | Reject before RPC simulation |
| Commitment mismatch | Transaction Builder | 422 | `COMMITMENT_MISMATCH` | Returned from RPC simulation |
| Reveal too early | Transaction Builder | 422 | `REVEAL_TOO_EARLY` | Returned from RPC simulation |
| Reveal timeout | Transaction Builder | 422 | `REVEAL_TIMEOUT` | Returned from RPC simulation |
| No active game | Transaction Builder | 404 | `NO_ACTIVE_GAME` | Returned from on-chain check |
| Invalid phase | Transaction Builder | 422 | `INVALID_PHASE` | Returned from RPC simulation |
| Nonce reused | Auth Middleware | 401 | `NONCE_REUSED` | Reject immediately |
| Request expired | Auth Middleware | 401 | `REQUEST_EXPIRED` | Reject immediately |
| Invalid signature | Auth Middleware | 401 | `INVALID_SIGNATURE` | Reject immediately |
| Soroban RPC unreachable | Read Model / TX Builder | 503 | `RPC_UNAVAILABLE` | Return last cached value or 503 |
| RPC simulation failure | Transaction Builder | 502 | `SIMULATION_FAILED` | Return RPC error details |
| Player not found | Read Model | 404 | `PLAYER_NOT_FOUND` | Return null game state |
| Invalid public key format | Validation | 400 | `INVALID_PUBLIC_KEY` | Reject before any processing |
| Secret length invalid | Transaction Builder | 422 | `INVALID_SECRET_LENGTH` | Reject before RPC simulation |

---

## Testing Strategy

### Dual Testing Approach

Both unit tests and property-based tests are required. Unit tests cover specific
examples, integration points, and error conditions. Property-based tests verify
universal correctness across all valid inputs.

### Property-Based Testing

**Library:** [`fast-check`](https://github.com/dubzzz/fast-check) for TypeScript
(backend and frontend). Minimum **100 iterations** per property test.

Each property test must include a comment referencing the design property:
```
// Feature: soroban-coinflip-game, Property N: <property_text>
```

Each correctness property (1–15) must be implemented by a single property-based test.

**Backend property tests** (`backend/src/__tests__/properties/`):
- `txBuilder.property.test.ts` — Properties 1, 2, 3, 4, 11, 12, 13
- `auth.property.test.ts` — Properties 5, 6, 14, 15
- `readModel.property.test.ts` — Properties 7, 9, 10
- `session.property.test.ts` — Property 8

### Unit Tests

**Backend unit tests** (`backend/src/__tests__/unit/`):
- `txBuilder.test.ts` — XDR construction for each operation, simulation error mapping
- `auth.test.ts` — nonce store, timestamp window, signature verification
- `readModel.test.ts` — cache TTL, RPC fallback, 404 on missing player
- `session.test.ts` — phase transitions, sync reconciliation

### Test Configuration

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

```typescript
// fast-check property test example
import fc from 'fast-check';

test('Property 3: wager bounds enforcement', () => {
  // Feature: soroban-coinflip-game, Property 3: wager bounds enforcement
  fc.assert(
    fc.property(
      fc.bigInt({ min: 1n }),   // minWager
      fc.bigInt({ min: 1n }),   // maxWager (will be added to minWager)
      fc.bigInt({ min: 0n }),   // wager
      (minWager, spread, wager) => {
        const maxWager = minWager + spread;
        const isBelowMin = wager < minWager;
        const isAboveMax = wager > maxWager;
        if (isBelowMin) {
          const result = validateWager(wager, minWager, maxWager);
          return result.code === 'WAGER_BELOW_MINIMUM';
        }
        if (isAboveMax) {
          const result = validateWager(wager, minWager, maxWager);
          return result.code === 'WAGER_ABOVE_MAXIMUM';
        }
        return true;
      }
    ),
    { numRuns: 100 }
  );
});
```

---

## Security Assumptions

1. **Backend is untrusted relay**: The API cannot forge valid Soroban transactions
   without the player's private key. All financial logic is enforced by the contract.
2. **Commitment secrecy**: The player generates the secret client-side and sends only
   the SHA-256 commitment to the API. The secret is never transmitted to the backend
   until the reveal step.
3. **No key custody**: The backend never receives, stores, or derives private keys.
   The wallet extension (Freighter / xBull) is the sole key custodian.
4. **Nonce replay protection**: Each authenticated request carries a UUID v4 nonce
   stored server-side for 60 seconds. Replayed requests are rejected with HTTP 401.
5. **Timestamp binding**: Requests older than 30 seconds are rejected to limit the
   replay window even if the nonce store is unavailable.
6. **Read model trust**: Read model data is sourced from the Soroban RPC and is
   authoritative. The backend does not modify or filter on-chain state.
7. **Simulation is advisory**: XDR simulation results are used for fee estimation and
   pre-flight validation only. The contract enforces all rules at submission time.
8. **Rate limiting**: The API enforces per-IP and per-public-key rate limits to prevent
   enumeration and DoS. Limits: 60 requests/minute per IP, 20 mutations/minute per key.

---

## Dependencies

| Dependency | Purpose |
|------------|---------|
| `@stellar/stellar-sdk` | XDR construction, simulation, RPC calls |
| `fast-check` | Property-based testing |
| `vitest` | Test runner |
| `hono` or `express` | HTTP routing (serverless-compatible) |
| `better-sqlite3` | Local dev session/nonce store |
| Cloudflare KV / Upstash Redis | Production session/nonce store |
