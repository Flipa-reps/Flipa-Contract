# Requirements Document

## Introduction

This feature defines the backend API contract for the Tossd coinflip game session
orchestration layer. The API sits between the React/TypeScript frontend and the Tossd
Soroban smart contract on Stellar. It provides unsigned transaction preparation,
read-model endpoints for game state and contract configuration, and session lifecycle
tracking. The backend is a stateless, serverless relay: it never holds private keys,
never submits transactions on behalf of players, and never stores wager funds.

Closes the backend API spec work tracked in the soroban-coinflip-game spec.

---

## Glossary

- **Transaction_Builder**: The backend service that validates inputs, simulates
  Soroban invocations, and returns unsigned XDR envelopes for wallet signing.
- **Read_Model**: The backend service that reads on-chain state via the Soroban RPC,
  caches it with a 5-second TTL, and serves it as typed JSON to the frontend.
- **Session_Orchestrator**: The backend component that mirrors the on-chain
  `GamePhase` in a KV store so the frontend can poll a single endpoint for lifecycle
  state without hitting the RPC on every render.
- **Auth_Middleware**: The request authentication layer that validates
  `X-Wallet-Pubkey`, `X-Request-Sig`, `X-Request-Nonce`, and `X-Request-Timestamp`
  headers on mutating endpoints.
- **XDR**: The Stellar binary encoding format for transactions and operations.
- **Commitment**: SHA-256 hash of the player's secret random value, submitted at game
  start so the player cannot change their random input after seeing the contract's
  contribution.
- **Contract**: The Tossd Soroban coinflip smart contract; the source of truth for all
  game rules and financial state.
- **Wallet**: The player's Stellar wallet extension (Freighter / xBull); the sole
  private key custodian.
- **Soroban_RPC**: The Stellar Soroban JSON-RPC endpoint used to simulate and submit
  transactions and read ledger entries.
- **GamePhase**: The on-chain lifecycle state of a player's game:
  `Committed | Revealed | Completed`.
- **ReserveHealth**: The on-chain reserve solvency snapshot with fields
  `reserveBalance`, `maxWorstCasePayout`, `coverageRatio`, `dynamicMaxWager`, `tier`.
- **Nonce**: A UUID v4 value included in each authenticated request and stored
  server-side for 60 seconds to prevent replay attacks.

---

## Requirements

### Requirement 1: Transaction Preparation

**User Story:** As a player, I want the backend to build unsigned Soroban transactions
for me, so that my wallet only needs to sign and submit without constructing XDR.

#### Acceptance Criteria

1. THE Transaction_Builder SHALL expose `POST /api/tx/prepare` accepting a
   `BuildTxRequest` with `playerPublicKey`, `operation`, and `params`, and SHALL
   return a `BuildTxResponse` containing a base64-encoded unsigned
   `TransactionEnvelope` XDR, estimated `simulationFee`, `expiresAt` ledger, and
   any non-fatal `warnings`.
2. THE Transaction_Builder SHALL include the exact `commitment` bytes from
   `StartGameParams` in the `start_game` contract invocation XDR without
   modification, truncation, or re-encoding.
3. THE Transaction_Builder SHALL simulate the transaction against the Soroban RPC
   before returning the XDR, and SHALL include the simulated fee in `simulationFee`.
4. IF `StartGameParams.wagerStroops` is less than `ContractConfig.minWagerStroops`
   or greater than `ReserveHealth.dynamicMaxWager`, THEN THE Transaction_Builder
   SHALL return HTTP 422 with `code: 'WAGER_BELOW_MINIMUM'` or
   `code: 'WAGER_ABOVE_MAXIMUM'` before any RPC simulation is attempted.
5. IF `StartGameParams.commitment` has all 32 bytes identical (weak entropy), THEN
   THE Transaction_Builder SHALL return HTTP 422 with `code: 'WEAK_COMMITMENT'`
   before any RPC simulation is attempted.
6. THE Transaction_Builder SHALL expose `POST /api/tx/submit` that accepts a
   signed XDR envelope, forwards it unchanged to the Soroban RPC, and returns the
   RPC's `txHash` and `status` verbatim.

---

### Requirement 2: Read Models

**User Story:** As a player, I want the frontend to display my current game state,
history, and contract configuration without calling the Soroban RPC directly, so that
the UI is fast and consistent.

#### Acceptance Criteria

1. THE Read_Model SHALL expose `GET /api/game/:playerPublicKey` returning a
   `GameStateReadModel` for the player's active game, or HTTP 404 with
   `code: 'NO_ACTIVE_GAME'` if no game exists.
2. THE Read_Model SHALL cache all Soroban RPC responses with a 5-second TTL and
   SHALL serve cached data on subsequent requests within the TTL window.
3. WHEN the Soroban RPC is unreachable, THE Read_Model SHALL return the last cached
   value if available, or HTTP 503 with `code: 'RPC_UNAVAILABLE'` if no cached
   value exists.
4. THE Read_Model SHALL expose `GET /api/history/:playerPublicKey` returning a
   `HistoryEntryReadModel[]` ordered by `ledger` descending, capped at 100 entries.
5. THE Read_Model SHALL expose `GET /api/reserve-health` returning a
   `ReserveHealthReadModel` with `tier` derived as: `'healthy'` when
   `coverageRatio >= 10`, `'moderate'` when `5 <= coverageRatio < 10`, `'low'`
   when `2 <= coverageRatio < 5`, and `'critical'` when `coverageRatio < 2`.
6. ALL read endpoints (`/api/game/*`, `/api/history/*`, `/api/config`,
   `/api/reserve-health`) SHALL be publicly accessible without authentication
   headers and SHALL NOT return HTTP 401 for unauthenticated requests.

---

### Requirement 3: Request Authentication

**User Story:** As a security auditor, I want all mutating endpoints to require
wallet-signed request authentication, so that no third party can prepare or submit
transactions on behalf of a player.

#### Acceptance Criteria

1. THE Auth_Middleware SHALL require `X-Wallet-Pubkey`, `X-Request-Sig`,
   `X-Request-Nonce`, and `X-Request-Timestamp` headers on all `POST` endpoints
   and SHALL return HTTP 401 with `code: 'UNAUTHORIZED'` for missing or invalid
   headers.
2. THE Auth_Middleware SHALL reject any request whose `X-Request-Nonce` has been
   seen within the last 60 seconds, returning HTTP 401 with `code: 'NONCE_REUSED'`.
3. THE Auth_Middleware SHALL reject any request where `X-Request-Timestamp` is more
   than 30 seconds in the past or more than 5 seconds in the future, returning
   HTTP 401 with `code: 'REQUEST_EXPIRED'`.
4. THE Auth_Middleware SHALL enforce per-IP rate limits of 60 requests per minute
   and per-public-key mutation limits of 20 requests per minute, returning HTTP 429
   on excess.

---

### Requirement 4: Session Orchestration

**User Story:** As a player, I want the frontend to know my current game lifecycle
phase without polling the Soroban RPC on every render, so that the UI transitions
smoothly between commit, reveal, and cash-out steps.

#### Acceptance Criteria

1. THE Session_Orchestrator SHALL maintain a `SessionRecord` per player with fields
   `phase`, `commitment`, `startLedger`, `wagerStroops`, `side`, and `updatedAt`.
2. THE `phase` field SHALL only advance in the order
   `idle → committed → revealed → completed` and SHALL never regress to an earlier
   phase during normal operation; `reclaim_wager` resets `phase` to `'idle'`.
3. THE Session_Orchestrator SHALL expose `GET /api/session/:playerPublicKey`
   returning the current `SessionRecord`, requiring authentication.
4. THE Session_Orchestrator SHALL expose `POST /api/session/:playerPublicKey/sync`
   that reconciles the session store against on-chain state from the Soroban RPC,
   requiring authentication.

---

### Requirement 5: Contract Error Mapping

**User Story:** As a developer, I want all Soroban contract errors to be mapped to
human-readable API error codes, so that the frontend can display actionable messages
without parsing raw XDR error envelopes.

#### Acceptance Criteria

1. FOR EACH Soroban contract error code returned by simulation or submission, THE
   Transaction_Builder SHALL map it to the corresponding `ApiError.code` string
   (e.g. error code `1` → `'WAGER_BELOW_MINIMUM'`, code `12` →
   `'COMMITMENT_MISMATCH'`) and SHALL include the original `contractErrorCode`
   integer in the response body.
2. THE Transaction_Builder SHALL return HTTP 422 for all contract-level validation
   errors and HTTP 502 for unexpected RPC failures.
3. IF `ContractConfigReadModel.paused` or `ContractConfigReadModel.shutdownMode` is
   `true` when `POST /api/tx/prepare` is called with `operation: 'start_game'` or
   `operation: 'continue_streak'`, THEN THE Transaction_Builder SHALL return HTTP
   422 with `code: 'CONTRACT_PAUSED'` or `code: 'CONTRACT_SHUTDOWN'` without
   simulating the transaction.

---

### Requirement 6: API Contract and Documentation

**User Story:** As a frontend developer, I want all API request and response shapes
to be defined as TypeScript interfaces, so that I can integrate the backend without
ambiguity.

#### Acceptance Criteria

1. ALL request and response bodies SHALL conform to the TypeScript interfaces defined
   in the design document: `BuildTxRequest`, `BuildTxResponse`, `GameStateReadModel`,
   `HistoryEntryReadModel`, `ContractConfigReadModel`, `ReserveHealthReadModel`,
   `SessionRecord`, and `ApiError`.
2. ALL `i128` Soroban values (wagers, balances, fees) SHALL be serialized as decimal
   strings in JSON to avoid JavaScript `number` precision loss.
3. ALL `BytesN` Soroban values (commitments, secrets, VRF proofs) SHALL be serialized
   as lowercase hex strings in JSON.
4. THE API SHALL return `Content-Type: application/json` on all endpoints and SHALL
   include `Access-Control-Allow-Origin` headers appropriate for the frontend origin.
