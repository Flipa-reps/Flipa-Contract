# Implementation Plan: Admin Dashboard Metrics

## Overview

Implement the admin analytics dashboard for the Tossd Soroban coinflip contract (issue #511).
The work is split across five layers: backend metrics collector, dashboard REST API, WebSocket
server, alerting engine, and React dashboard UI. All backend code lives in a new `backend/`
directory; frontend additions extend the existing `frontend/` tree.

## Tasks

- [ ] 1. Bootstrap backend project structure and shared types
  - Create `backend/` directory with `package.json`, `tsconfig.json`, and `vitest.config.ts`
  - Install dependencies: `express`, `ws`, `better-sqlite3`, `jsonwebtoken`, `@stellar/stellar-sdk`, `fast-check`, `vitest`
  - Create `backend/src/types.ts` defining `MetricsSnapshot`, `AlertRule`, `Alert`, and `MetricsCollectorConfig` interfaces
  - Create `backend/src/db.ts` with SQLite schema initialisation (`metrics_snapshots`, `alert_rules`, `alerts` tables and index)
  - _Requirements: 1.2, 1.3, 6.3, 6.4_

- [ ] 2. Implement Metrics Collector
  - [ ] 2.1 Implement `MetricsCollector` class in `backend/src/metricsCollector.ts`
    - Poll Soroban RPC at configurable interval using `@stellar/stellar-sdk`
    - Derive `health_status` from `paused`, `shutdown_mode`, `reserve_balance`, `min_reserve_threshold`
    - Retain last snapshot and increment `rpc_error_count` on failure; set `health_status = Critical` after 3 consecutive failures
    - Persist each successful snapshot via `MetricsStore`; enforce retention by deleting rows older than `retentionDays`
    - Emit `'snapshot'` and `'error'` events
    - _Requirements: 1.1, 1.2, 1.3, 1.4, 1.5, 1.6_

  - [ ]* 2.2 Write property test for snapshot completeness and persistence (Property 1)
    - **Property 1: Snapshot field completeness and persistence**
    - **Validates: Requirements 1.2, 1.6**
    - File: `backend/src/__tests__/properties/metricsCollector.property.test.ts`

  - [ ]* 2.3 Write property test for health status derivation (Property 2)
    - **Property 2: Health status derivation correctness**
    - **Validates: Requirements 1.3**
    - File: `backend/src/__tests__/properties/metricsCollector.property.test.ts`

  - [ ]* 2.4 Write property test for RPC failure resilience (Property 3)
    - **Property 3: RPC failure resilience**
    - **Validates: Requirements 1.4, 1.5**
    - File: `backend/src/__tests__/properties/metricsCollector.property.test.ts`

- [ ] 3. Implement Dashboard API (Express)
  - [ ] 3.1 Implement JWT auth middleware and `/api/auth/login` + `/api/auth/refresh` endpoints in `backend/src/auth.ts`
    - Issue HS256 JWT with ≤ 8h expiry on successful login
    - Provide refresh endpoint
    - Return HTTP 401 for missing/expired/malformed tokens on protected routes
    - _Requirements: 8.1, 8.3, 8.4_

  - [ ]* 3.2 Write unit tests for auth
    - JWT issuance, expiry enforcement, 401 on missing token
    - File: `backend/src/__tests__/unit/auth.test.ts`
    - _Requirements: 8.1, 8.4_

  - [ ] 3.3 Implement metrics endpoints in `backend/src/routes/metrics.ts`
    - `GET /api/metrics/current` — return latest snapshot within 500 ms
    - `GET /api/metrics/history?start=&end=` — return snapshots in range ordered by ascending timestamp; reject ranges > 30 days with HTTP 400; return `[]` for empty ranges
    - `GET /api/config` — return current `ContractConfig` fields
    - _Requirements: 2.1, 2.2, 2.3, 2.4, 2.6_

  - [ ]* 3.4 Write property test for history query correctness (Property 4)
    - **Property 4: History query correctness**
    - **Validates: Requirements 2.2**
    - File: `backend/src/__tests__/properties/dashboardApi.property.test.ts`

  - [ ]* 3.5 Write property test for history range validation (Property 5)
    - **Property 5: History range validation**
    - **Validates: Requirements 2.4**
    - File: `backend/src/__tests__/properties/dashboardApi.property.test.ts`

  - [ ]* 3.6 Write property test for authentication enforcement (Property 6)
    - **Property 6: Authentication enforcement**
    - **Validates: Requirements 2.5, 8.3**
    - File: `backend/src/__tests__/properties/dashboardApi.property.test.ts`

  - [ ]* 3.7 Write unit tests for metrics API
    - `GET /api/metrics/current` returns latest snapshot, `GET /api/config` returns correct fields, empty range returns `[]`
    - File: `backend/src/__tests__/unit/metricsApi.test.ts`
    - _Requirements: 2.1, 2.3, 2.6_

- [ ] 4. Checkpoint — Ensure all backend API tests pass
  - Ensure all tests pass, ask the user if questions arise.

- [ ] 5. Implement WebSocket Server
  - [ ] 5.1 Implement `WebSocketServer` in `backend/src/wsServer.ts`
    - Validate JWT on handshake; reject unauthenticated connections with HTTP 401
    - Send latest snapshot immediately on connect
    - Broadcast each new snapshot to all authenticated clients within 1 s
    - Heartbeat ping every 15 s; close non-responding clients after 5 s
    - _Requirements: 3.1, 3.2, 3.3, 3.5, 3.6, 3.7_

  - [ ]* 5.2 Write property test for WebSocket authentication enforcement (Property 7)
    - **Property 7: WebSocket authentication enforcement**
    - **Validates: Requirements 3.7, 8.2**
    - File: `backend/src/__tests__/properties/websocketServer.property.test.ts`

  - [ ]* 5.3 Write property test for WebSocket broadcast completeness (Property 8)
    - **Property 8: WebSocket broadcast completeness**
    - **Validates: Requirements 3.2**
    - File: `backend/src/__tests__/properties/websocketServer.property.test.ts`

  - [ ]* 5.4 Write unit tests for WebSocket server
    - On-connect initial snapshot delivery, authenticated connection accepted
    - File: `backend/src/__tests__/unit/websocket.test.ts`
    - _Requirements: 3.1, 3.3_

- [ ] 6. Implement Alerting Engine and alert rule CRUD
  - [ ] 6.1 Implement `AlertingEngine` class in `backend/src/alertingEngine.ts`
    - Evaluate each snapshot against all active `Alert_Rule` entries
    - Create `Alert` records on rule match with correct fields
    - Track last-triggered state per rule to prevent alert storms
    - Dispatch webhook POST within 5 s; retry up to 3 times with 10 s delay; mark `delivery_failed` after exhaustion
    - _Requirements: 6.1, 6.4, 6.5, 6.6, 6.9_

  - [ ]* 6.2 Write property test for alert creation on rule match (Property 13)
    - **Property 13: Alert creation on rule match**
    - **Validates: Requirements 6.1, 6.4**
    - File: `backend/src/__tests__/properties/alertingEngine.property.test.ts`

  - [ ]* 6.3 Write property test for webhook retry exhaustion (Property 14)
    - **Property 14: Webhook retry exhaustion**
    - **Validates: Requirements 6.6**
    - File: `backend/src/__tests__/properties/alertingEngine.property.test.ts`

  - [ ]* 6.4 Write property test for active alerts ordering (Property 15)
    - **Property 15: Active alerts ordering**
    - **Validates: Requirements 6.7**
    - File: `backend/src/__tests__/properties/alertingEngine.property.test.ts`

  - [ ]* 6.5 Write property test for alert acknowledgement round-trip (Property 16)
    - **Property 16: Alert acknowledgement round-trip**
    - **Validates: Requirements 6.8**
    - File: `backend/src/__tests__/properties/alertingEngine.property.test.ts`

  - [ ]* 6.6 Write property test for alert storm prevention (Property 17)
    - **Property 17: Alert storm prevention**
    - **Validates: Requirements 6.9**
    - File: `backend/src/__tests__/properties/alertingEngine.property.test.ts`

  - [ ] 6.7 Implement alert rule CRUD and alert management endpoints in `backend/src/routes/alerts.ts`
    - `GET/POST /api/alerts/rules`, `PUT/DELETE /api/alerts/rules/:id`
    - `GET /api/alerts/active`, `POST /api/alerts/:id/acknowledge`
    - `GET /api/alerts/rules/export`, `POST /api/alerts/rules/import` with schema validation (HTTP 422 on invalid entries)
    - Upsert on import when `id` matches existing rule
    - _Requirements: 6.2, 6.3, 6.7, 6.8, 7.1, 7.2, 7.3, 7.5_

  - [ ]* 6.8 Write property test for alert rule round-trip serialization (Property 18)
    - **Property 18: Alert rule round-trip serialization**
    - **Validates: Requirements 7.4, 7.5**
    - File: `backend/src/__tests__/properties/alertRules.property.test.ts`

  - [ ]* 6.9 Write property test for import schema validation (Property 19)
    - **Property 19: Import schema validation**
    - **Validates: Requirements 7.3**
    - File: `backend/src/__tests__/properties/alertRules.property.test.ts`

  - [ ]* 6.10 Write unit tests for alerting
    - Webhook POST sent on alert creation
    - File: `backend/src/__tests__/unit/alerting.test.ts`
    - _Requirements: 6.5_

- [ ] 7. Wire backend components together in `backend/src/index.ts`
  - Instantiate `MetricsCollector`, `MetricsStore`, `AlertingEngine`, Express app, and `WebSocketServer`
  - Subscribe `AlertingEngine.evaluate` and `WebSocketServer.broadcast` to the collector's `'snapshot'` event
  - Start HTTP server and begin polling
  - _Requirements: 1.1, 3.2, 6.1_

- [ ] 8. Checkpoint — Ensure all backend tests pass
  - Ensure all tests pass, ask the user if questions arise.

- [ ] 9. Implement `useMetricsWebSocket` hook
  - Create `frontend/hooks/useMetricsWebSocket.ts`
  - Manage WebSocket lifecycle: connect, receive snapshots, update `snapshot` state and `lastUpdated`
  - Implement exponential back-off reconnection: `delay = min(2^attempt * 1000, 30_000)` ms
  - Expose `status`: `'connecting' | 'connected' | 'reconnecting' | 'disconnected'`
  - _Requirements: 3.4, 5.1, 5.4, 5.5_

  - [ ]* 9.1 Write property test for exponential back-off correctness (Property 9)
    - **Property 9: Exponential back-off correctness**
    - **Validates: Requirements 3.4**
    - File: `frontend/tests/properties/useMetricsWebSocket.property.test.ts`

  - [ ]* 9.2 Write property test for hook state update on snapshot receipt (Property 12)
    - **Property 12: Hook state update on snapshot receipt**
    - **Validates: Requirements 5.1**
    - File: `frontend/tests/properties/useMetricsWebSocket.property.test.ts`

- [ ] 10. Implement React dashboard components
  - [ ] 10.1 Create `frontend/components/admin/MetricCard.tsx` and `HealthIndicator.tsx`
    - `MetricCard`: display a single KPI tile (reserve, games, volume, fees)
    - `HealthIndicator`: colour-coded badge — green/amber/red for Healthy/Degraded/Critical
    - _Requirements: 4.1, 4.2_

  - [ ] 10.2 Create `frontend/components/admin/ReserveChart.tsx` and `GameActivityChart.tsx`
    - `ReserveChart`: 24h `reserve_balance` time-series using `recharts`
    - `GameActivityChart`: 24h games-per-hour bar chart aggregating `total_games` deltas
    - _Requirements: 4.3, 4.4_

  - [ ]* 10.3 Write property test for dashboard metric rendering completeness (Property 10)
    - **Property 10: Dashboard metric rendering completeness**
    - **Validates: Requirements 4.1, 4.2**
    - File: `frontend/tests/properties/adminDashboard.property.test.ts`

  - [ ]* 10.4 Write property test for chart data aggregation correctness (Property 11)
    - **Property 11: Chart data aggregation correctness**
    - **Validates: Requirements 4.3, 4.4**
    - File: `frontend/tests/properties/adminDashboard.property.test.ts`

  - [ ] 10.5 Create `frontend/components/admin/AlertsPanel.tsx`
    - List unacknowledged alerts ordered by `triggered_at` descending
    - Acknowledge action calls `POST /api/alerts/:id/acknowledge` and removes the alert from the panel
    - _Requirements: 6.7, 6.8_

  - [ ] 10.6 Create `frontend/components/admin/ConnectionStatus.tsx`
    - Show "Reconnecting…" banner when WebSocket status is `reconnecting` or `disconnected`
    - Show staleness indicator while reconnecting; remove it on successful reconnect
    - _Requirements: 4.5, 5.4, 5.5_

  - [ ]* 10.7 Write unit tests for `ConnectionStatus`
    - "Reconnecting…" shown when disconnected, staleness indicator shown/removed correctly
    - File: `frontend/tests/ConnectionStatus.test.tsx`
    - _Requirements: 4.5, 5.4, 5.5_

  - [ ] 10.8 Create `frontend/components/admin/AdminDashboard.tsx`
    - Top-level admin-only route component
    - Compose `MetricCard`, `HealthIndicator`, `ReserveChart`, `GameActivityChart`, `AlertsPanel`, `ConnectionStatus`
    - Display `lastUpdated` timestamp for data freshness (Req 4.6)
    - Animate metric value transitions ≤ 300 ms (Req 5.3)
    - Fetch JWT from in-memory state only; never write to `localStorage`/`sessionStorage` (Req 8.5)
    - _Requirements: 4.1, 4.6, 5.2, 5.3, 8.5_

- [ ] 11. Final checkpoint — Ensure all tests pass
  - Ensure all tests pass, ask the user if questions arise.

## Notes

- Tasks marked with `*` are optional and can be skipped for a faster MVP
- Each task references specific requirements for traceability
- Property tests use `fast-check` with a minimum of 100 iterations and must include the comment `// Feature: admin-dashboard-metrics, Property N: <property_text>`
- The backend runs as a separate Node.js process; do not bundle it into the Vite frontend build
