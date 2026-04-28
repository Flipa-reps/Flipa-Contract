# Implementation Plan: Soroban Coinflip Game — Backend API

## Overview

Implement the backend API for Tossd game session orchestration (soroban-coinflip-game
spec). The work is split across four layers: shared types and project bootstrap,
transaction builder service, read model service, and session orchestrator. All backend
code lives in a new `backend/` directory. The frontend integration layer extends the
existing `frontend/hooks/contract.ts` adapter.

## Tasks

- [ ] 1. Bootstrap backend project structure and shared types
  - Create `backend/` directory with `package.json`, `tsconfig.json`, and `vitest.config.ts`
  - Install dependencies: `hono` (or `express`), `better-sqlite3`, `@stellar/stellar-sdk`, `fast-check`, `vitest`
  - Create `backend/src/types.ts` defining all interfaces: `BuildTxRequest`, `BuildTxResponse`, `StartGameParams`, `RevealParams`, `CashOutParams`, `ContinueStreakParams`, `ReclaimWagerParams`, `SideBetParams`, `GameStateReadModel`, `HistoryEntryReadModel`, `ContractConfigReadModel`, `ReserveHealthReadModel`, `SessionRecord`, `ApiError`, `AuthHeaders`
  - Create `backend/src/db.ts` with SQLite schema for `sessions` and `nonces` tables
  - _Requirements: 1.1, 4.1, 6.1, 6.2, 6.3_

- [ ] 2. Implement Auth Middleware
  - [ ] 2.1 Implement `backend/src/auth.ts` with nonce store, timestamp window check, and header validation
    - Store nonces in SQLite with 60-second TTL; reject reused nonces with HTTP 401 `NONCE_REUSED`
    - Reject requests with `X-Request-Timestamp` older than 30 s or newer than 5 s with HTTP 401 `REQUEST_EXPIRED`
    - Validate `X-Wallet-Pubkey` is a valid Stellar G-address
    - Enforce per-IP 60 req/min and per-key 20 mutations/min rate limits; return HTTP 429 on excess
    - _Requirements: 3.1, 3.2, 3.3, 3.4_

  - [ ]* 2.2 Write property test for nonce uniqueness enforcement (Property 5)
    - **Property 5: Nonce uniqueness enforcement**
    - **Validates: Requirements 3.1, 3.2**
    - File: `backend/src/__tests__/properties/auth.property.test.ts`

  - [ ]* 2.3 Write property test for timestamp window enforcement (Property 6)
    - **Property 6: Timestamp window enforcement**
    - **Validates: Requirements 3.1, 3.3**
    - File: `backend/src/__tests__/properties/auth.property.test.ts`

  - [ ]* 2.4 Write property test for unauthenticated read access (Property 14)
    - **Property 14: Unauthenticated read access**
    - **Validates: Requirements 2.6**
    - File: `backend/src/__tests__/properties/auth.property.test.ts`

  - [ ]* 2.5 Write property test for authenticated mutation requirement (Property 15)
    - **Property 15: Authenticated mutation requirement**
    - **Validates: Requirements 3.4**
    - File: `backend/src/__tests__/properties/auth.property.test.ts`

  - [ ]* 2.6 Write unit tests for auth middleware
    - Nonce reuse rejected, expired timestamp rejected, valid headers accepted, rate limit enforced
    - File: `backend/src/__tests__/unit/auth.test.ts`
    - _Requirements: 3.1, 3.2, 3.3, 3.4_

- [ ] 3. Implement Transaction Builder Service
  - [ ] 3.1 Implement `backend/src/txBuilder.ts`
    - Build unsigned XDR for `start_game`, `reveal`, `cash_out`, `continue_streak`, `reclaim_wager`
    - Validate wager bounds against `ContractConfig.minWagerStroops` and `ReserveHealth.dynamicMaxWager` before simulation; return HTTP 422 on violation
    - Validate commitment entropy (reject all-same-byte commitments) before simulation; return HTTP 422 `WEAK_COMMITMENT`
    - Simulate via `@stellar/stellar-sdk` `simulateTransaction`; include fee in response
    - Map Soroban contract error codes to `ApiError.code` strings per the error table in the design
    - Reject `start_game` / `continue_streak` immediately when `paused` or `shutdownMode` is true
    - _Requirements: 1.1, 1.2, 1.3, 1.4, 1.5, 5.1, 5.2, 5.3_

  - [ ]* 3.2 Write property test for commitment integrity (Property 1)
    - **Property 1: Commitment integrity**
    - **Validates: Requirements 1.1, 1.2**
    - File: `backend/src/__tests__/properties/txBuilder.property.test.ts`

  - [ ]* 3.3 Write property test for XDR round-trip fidelity (Property 2)
    - **Property 2: XDR round-trip fidelity**
    - **Validates: Requirements 1.1, 1.3**
    - File: `backend/src/__tests__/properties/txBuilder.property.test.ts`

  - [ ]* 3.4 Write property test for wager bounds enforcement (Property 3)
    - **Property 3: Wager bounds enforcement**
    - **Validates: Requirements 1.4, 2.1**
    - File: `backend/src/__tests__/properties/txBuilder.property.test.ts`

  - [ ]* 3.5 Write property test for commitment entropy validation (Property 4)
    - **Property 4: Commitment entropy validation**
    - **Validates: Requirements 1.5**
    - File: `backend/src/__tests__/properties/txBuilder.property.test.ts`

  - [ ]* 3.6 Write property test for submit relay fidelity (Property 11)
    - **Property 11: Submit relay fidelity**
    - **Validates: Requirements 1.6**
    - File: `backend/src/__tests__/properties/txBuilder.property.test.ts`

  - [ ]* 3.7 Write property test for contract error passthrough (Property 12)
    - **Property 12: Contract error passthrough**
    - **Validates: Requirements 5.1, 5.2**
    - File: `backend/src/__tests__/properties/txBuilder.property.test.ts`

  - [ ]* 3.8 Write property test for paused contract rejection (Property 13)
    - **Property 13: Paused contract rejection**
    - **Validates: Requirements 1.4, 5.3**
    - File: `backend/src/__tests__/properties/txBuilder.property.test.ts`

  - [ ]* 3.9 Write unit tests for transaction builder
    - XDR construction for each operation, simulation error mapping, wager validation
    - File: `backend/src/__tests__/unit/txBuilder.test.ts`
    - _Requirements: 1.1, 1.2, 1.3, 1.4, 1.5, 1.6_

- [ ] 4. Checkpoint — Ensure all transaction builder tests pass
  - Ensure all tests pass, ask the user if questions arise.

- [ ] 5. Implement Read Model Service
  - [ ] 5.1 Implement `backend/src/readModel.ts`
    - `GET /api/game/:playerPublicKey` — fetch `GameState` ledger entry; return `GameStateReadModel` or 404
    - `GET /api/history/:playerPublicKey` — fetch history ring-buffer; return `HistoryEntryReadModel[]` ordered by `ledger` desc, capped at 100
    - `GET /api/config` — fetch `ContractConfig` ledger entry; return `ContractConfigReadModel`
    - `GET /api/reserve-health` — fetch `ReserveHealth` ledger entry; return `ReserveHealthReadModel` with derived `tier`
    - Implement 5-second TTL in-memory cache; serve cached data on hits; fall back to last cached value on RPC failure
    - Serialize all `i128` values as decimal strings; serialize all `BytesN` values as lowercase hex
    - _Requirements: 2.1, 2.2, 2.3, 2.4, 2.5, 2.6, 6.2, 6.3_

  - [ ]* 5.2 Write property test for read model cache consistency (Property 7)
    - **Property 7: Read model cache consistency**
    - **Validates: Requirements 2.2, 2.3**
    - File: `backend/src/__tests__/properties/readModel.property.test.ts`

  - [ ]* 5.3 Write property test for history ordering (Property 9)
    - **Property 9: History ordering**
    - **Validates: Requirements 2.4**
    - File: `backend/src/__tests__/properties/readModel.property.test.ts`

  - [ ]* 5.4 Write property test for reserve health tier correctness (Property 10)
    - **Property 10: Reserve health tier correctness**
    - **Validates: Requirements 2.5**
    - File: `backend/src/__tests__/properties/readModel.property.test.ts`

  - [ ]* 5.5 Write unit tests for read model service
    - Cache TTL, RPC fallback, 404 on missing player, tier derivation
    - File: `backend/src/__tests__/unit/readModel.test.ts`
    - _Requirements: 2.1, 2.2, 2.3, 2.4, 2.5, 2.6_

- [ ] 6. Implement Session Orchestrator
  - [ ] 6.1 Implement `backend/src/session.ts`
    - `GET /api/session/:playerPublicKey` — return `SessionRecord` from KV/SQLite store
    - `POST /api/session/:playerPublicKey/sync` — reconcile session phase against on-chain `GameState`; advance phase forward only
    - Enforce phase monotonicity: `idle → committed → revealed → completed`; `reclaim_wager` resets to `idle`
    - Update `SessionRecord` optimistically when `POST /api/tx/prepare` is called for phase-advancing operations
    - _Requirements: 4.1, 4.2, 4.3, 4.4_

  - [ ]* 6.2 Write property test for phase transition monotonicity (Property 8)
    - **Property 8: Phase transition monotonicity**
    - **Validates: Requirements 4.1, 4.2**
    - File: `backend/src/__tests__/properties/session.property.test.ts`

  - [ ]* 6.3 Write unit tests for session orchestrator
    - Phase transitions, sync reconciliation, reclaim reset
    - File: `backend/src/__tests__/unit/session.test.ts`
    - _Requirements: 4.1, 4.2, 4.3, 4.4_

- [ ] 7. Wire backend components together in `backend/src/index.ts`
  - Instantiate Read Model Service, Transaction Builder, Session Orchestrator, Auth Middleware
  - Register all routes with the HTTP framework
  - Apply CORS headers for the frontend origin
  - Start HTTP server
  - _Requirements: 1.1, 2.1, 3.1, 4.1, 6.4_

- [ ] 8. Final checkpoint — Ensure all tests pass
  - Ensure all tests pass, ask the user if questions arise.

## Notes

- Tasks marked with `*` are optional and can be skipped for a faster MVP
- Each task references specific requirements for traceability
- Property tests use `fast-check` with a minimum of 100 iterations and must include the comment `// Feature: soroban-coinflip-game, Property N: <property_text>`
- The backend runs as a separate Node.js process; do not bundle it into the Vite frontend build
- All `i128` values must be serialized as decimal strings in JSON; all `BytesN` values as lowercase hex strings
