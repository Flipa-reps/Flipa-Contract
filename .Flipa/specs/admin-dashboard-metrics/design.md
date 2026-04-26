# Design Document: Admin Dashboard Metrics

## Overview

This feature adds a real-time admin analytics dashboard for the Tossd Soroban coinflip
contract (issue #511). It consists of five integrated layers:

1. **Metrics Collector** — a Node.js/TypeScript backend service that polls the Soroban
   RPC on a configurable interval, derives `MetricsSnapshot` records, and persists them
   to a time-series store.
2. **Dashboard API** — an Express REST API that serves current and historical
   `MetricsSnapshot` data and manages `Alert_Rule` CRUD, protected by JWT auth.
3. **WebSocket Server** — a `ws`-based server that broadcasts each new
   `MetricsSnapshot` to all authenticated connected clients within 1 second.
4. **React Dashboard UI** — a new admin-only route in the existing Vite/React/TypeScript
   frontend that renders metric panels, time-series charts, and an alerts panel, fed by
   a `useMetricsWebSocket` hook.
5. **Alerting Engine** — a backend component that evaluates `Alert_Rule` entries against
   each incoming `MetricsSnapshot` and delivers `Alert` records via in-app or webhook
   channels.

The backend runs as a separate Node.js process alongside the existing frontend dev/build
pipeline. The frontend communicates with it over HTTP (REST) and WebSocket.

---

## Architecture

```mermaid
graph TD
    subgraph Stellar Network
        RPC[Soroban RPC]
    end

    subgraph Backend Process (Node.js / TypeScript)
        MC[Metrics Collector]
        TS[(Time-Series Store\nSQLite / better-sqlite3)]
        API[Dashboard API\nExpress]
        WSS[WebSocket Server\nws]
        AE[Alerting Engine]
        AR[(Alert Rules Store\nSQLite)]
    end

    subgraph Frontend (React / TypeScript / Vite)
        HOOK[useMetricsWebSocket hook]
        DASH[AdminDashboard component]
        ALERTS[AlertsPanel component]
    end

    RPC -->|poll every N s| MC
    MC -->|persist snapshot| TS
    MC -->|emit snapshot event| WSS
    MC -->|emit snapshot event| AE
    AE -->|read rules| AR
    AE -->|write alert| AR
    AE -->|webhook POST| ExternalWebhook[External Webhook URL]
    API -->|query| TS
    API -->|CRUD| AR
    WSS -->|broadcast JSON| HOOK
    HOOK -->|state update| DASH
    HOOK -->|state update| ALERTS
    DASH -->|REST calls| API
```

**Key design decisions:**

- **SQLite via `better-sqlite3`** for the time-series and alert stores. The dashboard is
  an internal admin tool; SQLite avoids an external database dependency while providing
  sufficient query performance for 7-day retention at 5-second poll intervals (~120k rows
  max). Swapping to PostgreSQL later requires only changing the repository layer.
- **`ws` library** for the WebSocket server. It is the minimal, well-maintained choice
  that integrates cleanly with an Express HTTP server via `server.on('upgrade', ...)`.
- **`@stellar/stellar-sdk`** (already a frontend dependency) is reused in the backend
  to call the Soroban RPC — no new Stellar SDK dependency needed.
- **JWT (HS256)** for authentication. The admin signs in once; the token is stored
  in-memory in the React app (never `localStorage`/`sessionStorage`).
- **Separate backend process** keeps the Soroban polling and WebSocket logic out of the
  Vite dev server and avoids bundling Node-only modules into the browser build.

---

## Components and Interfaces

### 2.1 Metrics Collector

```typescript
interface MetricsCollectorConfig {
  rpcUrl: string;
  contractId: string;
  pollIntervalMs: number;   // default 5000
  retentionDays: number;    // default 7
}

class MetricsCollector extends EventEmitter {
  constructor(config: MetricsCollectorConfig, store: MetricsStore);
  start(): void;
  stop(): void;
  getLatestSnapshot(): MetricsSnapshot | null;
  // emits: 'snapshot' (MetricsSnapshot), 'error' (Error)
}
```

The collector maintains an internal `rpcErrorCount`. On each successful poll it resets
the counter and emits `'snapshot'`. On failure it increments the counter; when it reaches
3 it sets `health_status = 'Critical'` on the retained snapshot and emits `'error'`.

### 2.2 Dashboard API (Express)

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/api/auth/login` | Issue JWT for admin |
| `POST` | `/api/auth/refresh` | Refresh JWT |
| `GET` | `/api/metrics/current` | Latest `MetricsSnapshot` |
| `GET` | `/api/metrics/history?start=&end=` | Historical snapshots |
| `GET` | `/api/config` | Current `ContractConfig` fields |
| `GET` | `/api/alerts/rules` | List alert rules |
| `POST` | `/api/alerts/rules` | Create alert rule |
| `PUT` | `/api/alerts/rules/:id` | Update alert rule |
| `DELETE` | `/api/alerts/rules/:id` | Delete alert rule |
| `GET` | `/api/alerts/rules/export` | Export rules as JSON array |
| `POST` | `/api/alerts/rules/import` | Import rules from JSON array |
| `GET` | `/api/alerts/active` | List unacknowledged alerts |
| `POST` | `/api/alerts/:id/acknowledge` | Acknowledge an alert |

All endpoints except `/api/auth/*` require `Authorization: Bearer <jwt>`.

### 2.3 WebSocket Server

```typescript
interface WsMessage {
  type: 'snapshot' | 'heartbeat';
  payload?: MetricsSnapshot;
}
```

- Handshake: client sends JWT in `Authorization` header or `?token=` query param.
- On connect: server immediately sends the latest `MetricsSnapshot`.
- Heartbeat: server pings every 15 s; closes connections that don't pong within 5 s.
- Broadcast: on each `'snapshot'` event from the collector, server serialises and sends
  to all authenticated connections.

### 2.4 WebSocket Client Hook

```typescript
function useMetricsWebSocket(url: string, token: string): {
  snapshot: MetricsSnapshot | null;
  status: 'connecting' | 'connected' | 'reconnecting' | 'disconnected';
  lastUpdated: Date | null;
}
```

Reconnection uses exponential back-off: `delay = min(2^attempt * 1000, 30_000)` ms.

### 2.5 Alerting Engine

```typescript
class AlertingEngine {
  constructor(store: AlertStore);
  evaluate(snapshot: MetricsSnapshot): Promise<void>;
  // Reads all active Alert_Rule entries, checks conditions,
  // creates Alert records, dispatches webhooks with retry.
}
```

Alert storm prevention: the engine tracks the last-triggered state per rule. A rule
re-fires only after the metric has recovered past the threshold and then breached it again.

### 2.6 React Dashboard Components

```
frontend/components/admin/
  AdminDashboard.tsx        — top-level route component
  MetricCard.tsx            — single KPI tile (reserve, games, volume, fees)
  HealthIndicator.tsx       — colour-coded health badge
  ReserveChart.tsx          — 24h reserve_balance time-series (recharts)
  GameActivityChart.tsx     — 24h games-per-hour bar chart (recharts)
  AlertsPanel.tsx           — active alerts list with acknowledge action
  ConnectionStatus.tsx      — "Reconnecting…" / staleness banner
```

`recharts` is the chosen charting library — lightweight, React-native, no canvas
dependency, tree-shakeable.

---

## Data Models

### MetricsSnapshot

```typescript
interface MetricsSnapshot {
  id: string;                    // UUID, assigned by collector
  timestamp: string;             // ISO-8601
  reserve_balance: bigint;       // stroops
  total_games: bigint;
  total_volume: bigint;          // stroops
  total_fees: bigint;            // stroops
  fee_bps: number;
  paused: boolean;
  shutdown_mode: boolean;
  min_reserve_threshold: bigint; // stroops
  health_status: 'Healthy' | 'Degraded' | 'Critical';
}
```

Health derivation (mirrors Requirement 1.3):
```
if paused || shutdown_mode  → Critical
else if reserve_balance <= 2 * min_reserve_threshold && min_reserve_threshold > 0 → Degraded
else → Healthy
```

### AlertRule

```typescript
interface AlertRule {
  id: string;                    // UUID
  metric_name: 'reserve_balance' | 'total_games' | 'rpc_error_count';
  operator: 'lt' | 'gt' | 'eq';
  threshold: number;
  severity: 'warning' | 'critical';
  notification_channel: 'in_app' | 'webhook';
  webhook_url?: string;          // required when notification_channel = 'webhook'
  created_at: string;            // ISO-8601
  updated_at: string;            // ISO-8601
}
```

### Alert

```typescript
interface Alert {
  id: string;                    // UUID
  rule_id: string;
  metric_name: string;
  observed_value: number;
  threshold: number;
  severity: 'warning' | 'critical';
  triggered_at: string;          // ISO-8601
  acknowledged: boolean;
  acknowledged_at?: string;
  delivery_status: 'pending' | 'delivered' | 'delivery_failed';
  retry_count: number;
}
```

### SQLite Schema

```sql
CREATE TABLE metrics_snapshots (
  id TEXT PRIMARY KEY,
  timestamp TEXT NOT NULL,
  reserve_balance TEXT NOT NULL,   -- stored as string (bigint)
  total_games TEXT NOT NULL,
  total_volume TEXT NOT NULL,
  total_fees TEXT NOT NULL,
  fee_bps INTEGER NOT NULL,
  paused INTEGER NOT NULL,         -- 0/1
  shutdown_mode INTEGER NOT NULL,
  min_reserve_threshold TEXT NOT NULL,
  health_status TEXT NOT NULL
);

CREATE INDEX idx_snapshots_timestamp ON metrics_snapshots(timestamp);

CREATE TABLE alert_rules (
  id TEXT PRIMARY KEY,
  metric_name TEXT NOT NULL,
  operator TEXT NOT NULL,
  threshold REAL NOT NULL,
  severity TEXT NOT NULL,
  notification_channel TEXT NOT NULL,
  webhook_url TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE alerts (
  id TEXT PRIMARY KEY,
  rule_id TEXT NOT NULL,
  metric_name TEXT NOT NULL,
  observed_value REAL NOT NULL,
  threshold REAL NOT NULL,
  severity TEXT NOT NULL,
  triggered_at TEXT NOT NULL,
  acknowledged INTEGER NOT NULL DEFAULT 0,
  acknowledged_at TEXT,
  delivery_status TEXT NOT NULL DEFAULT 'pending',
  retry_count INTEGER NOT NULL DEFAULT 0,
  FOREIGN KEY (rule_id) REFERENCES alert_rules(id)
);
```

---

## Correctness Properties

*A property is a characteristic or behavior that should hold true across all valid
executions of a system — essentially, a formal statement about what the system should do.
Properties serve as the bridge between human-readable specifications and
machine-verifiable correctness guarantees.*

### Property 1: Snapshot field completeness and persistence

*For any* valid `ContractStats` + `ContractConfig` response from the Soroban RPC, the
`MetricsSnapshot` produced by the Metrics Collector must contain all required fields
(`timestamp`, `reserve_balance`, `total_games`, `total_volume`, `total_fees`, `fee_bps`,
`paused`, `shutdown_mode`, `min_reserve_threshold`, `health_status`) with values that
match the RPC response, and that snapshot must be retrievable from the time-series store
by its `id`.

**Validates: Requirements 1.2, 1.6**

---

### Property 2: Health status derivation correctness

*For any* combination of `paused`, `shutdown_mode`, `reserve_balance`, and
`min_reserve_threshold` values, the derived `health_status` must satisfy:
- `Critical` when `paused` is `true` or `shutdown_mode` is `true`
- `Degraded` when `reserve_balance <= 2 * min_reserve_threshold` and
  `min_reserve_threshold > 0` (and neither pause flag is set)
- `Healthy` otherwise

**Validates: Requirements 1.3**

---

### Property 3: RPC failure resilience

*For any* sequence of RPC failures, the Metrics Collector must retain the most recent
successful `MetricsSnapshot` unchanged and increment `rpc_error_count` by exactly 1 per
failure. After 3 consecutive failures, the retained snapshot's `health_status` must be
`Critical`.

**Validates: Requirements 1.4, 1.5**

---

### Property 4: History query correctness

*For any* `start` and `end` ISO-8601 timestamps (where the range is ≤ 30 days) and any
set of stored `MetricsSnapshot` records, the `GET /api/metrics/history` response must
contain exactly the snapshots whose `timestamp` falls within `[start, end]`, ordered by
ascending `timestamp`.

**Validates: Requirements 2.2**

---

### Property 5: History range validation

*For any* `start` and `end` values where `end - start > 30 days`, the
`GET /api/metrics/history` endpoint must return HTTP 400 with a descriptive error
message.

**Validates: Requirements 2.4**

---

### Property 6: Authentication enforcement

*For any* request to any Dashboard API endpoint (except `/api/auth/*`) that lacks a
valid Bearer JWT, the response must be HTTP 401. This includes requests with missing,
expired, or malformed tokens.

**Validates: Requirements 2.5, 8.3**

---

### Property 7: WebSocket authentication enforcement

*For any* WebSocket connection attempt that lacks a valid JWT (missing, expired, or
malformed), the server must reject the handshake with HTTP 401 and must not add the
client to the broadcast list.

**Validates: Requirements 3.7, 8.2**

---

### Property 8: WebSocket broadcast completeness

*For any* set of authenticated connected WebSocket clients and any new `MetricsSnapshot`
emitted by the Metrics Collector, all clients in the set must receive the snapshot as a
JSON message.

**Validates: Requirements 3.2**

---

### Property 9: Exponential back-off correctness

*For any* reconnection attempt number `n` (starting at 0), the back-off delay computed
by the `useMetricsWebSocket` hook must equal `min(2^n * 1000, 30_000)` milliseconds.

**Validates: Requirements 3.4**

---

### Property 10: Dashboard metric rendering completeness

*For any* `MetricsSnapshot`, the rendered `AdminDashboard` component must display all
required fields: `reserve_balance` (in XLM), `total_games`, `total_volume` (in XLM),
`total_fees` (in XLM), `fee_bps`, `paused`, and `health_status` with the correct
colour-coded indicator (green / amber / red).

**Validates: Requirements 4.1, 4.2**

---

### Property 11: Chart data aggregation correctness

*For any* sequence of `MetricsSnapshot` records spanning up to 24 hours, the
`ReserveChart` must render data points matching the snapshot values, and the
`GameActivityChart` must render per-hour game counts that correctly aggregate the
`total_games` deltas within each hour bucket.

**Validates: Requirements 4.3, 4.4**

---

### Property 12: Hook state update on snapshot receipt

*For any* `MetricsSnapshot` message received by the `useMetricsWebSocket` hook, the
hook's `snapshot` state must be updated to reflect the received snapshot and
`lastUpdated` must be set to the current time.

**Validates: Requirements 5.1**

---

### Property 13: Alert creation on rule match

*For any* `MetricsSnapshot` and any `Alert_Rule` whose condition is satisfied by that
snapshot (i.e. `metric_value operator threshold` evaluates to `true`), the Alerting
Engine must create an `Alert` record containing the correct `rule_id`, `metric_name`,
`observed_value`, `threshold`, `severity`, and `triggered_at`.

**Validates: Requirements 6.1, 6.4**

---

### Property 14: Webhook retry exhaustion

*For any* `Alert` with `notification_channel = 'webhook'` where all delivery attempts
fail, after exactly 3 retry attempts the `Alert` record's `delivery_status` must be
`'delivery_failed'` and `retry_count` must equal 3.

**Validates: Requirements 6.6**

---

### Property 15: Active alerts ordering

*For any* set of unacknowledged `Alert` records, the alerts panel must render them
ordered by `triggered_at` descending (most recent first).

**Validates: Requirements 6.7**

---

### Property 16: Alert acknowledgement round-trip

*For any* `Alert` that is acknowledged via `POST /api/alerts/:id/acknowledge`, a
subsequent `GET /api/alerts/active` must not include that alert in the response.

**Validates: Requirements 6.8**

---

### Property 17: Alert storm prevention

*For any* `Alert_Rule` that has already fired for a given metric value, the Alerting
Engine must not create a new `Alert` for that rule until the metric has first recovered
past the threshold and then breached it again.

**Validates: Requirements 6.9**

---

### Property 18: Alert rule round-trip serialization

*For any* valid set of `Alert_Rule` entries stored in the system, exporting via
`GET /api/alerts/rules/export`, then importing via `POST /api/alerts/rules/import`,
then exporting again must produce a JSON array equivalent to the original export
(same rules, same field values, no duplicates created for matching `id`s).

**Validates: Requirements 7.4, 7.5**

---

### Property 19: Import schema validation

*For any* JSON array submitted to `POST /api/alerts/rules/import` that contains one or
more entries violating the `Alert_Rule` schema (invalid `metric_name`, `operator`,
`severity`, or `notification_channel`), the endpoint must return HTTP 422 with a
per-rule error list identifying each invalid entry.

**Validates: Requirements 7.3**

---

## Error Handling

| Scenario | Component | Behaviour |
|----------|-----------|-----------|
| Soroban RPC unreachable | Metrics Collector | Retain last snapshot; increment `rpc_error_count`; after 3 consecutive failures set `health_status = Critical` and emit error event |
| RPC returns malformed data | Metrics Collector | Log parse error; treat as RPC failure (increment counter) |
| SQLite write failure | Metrics Store | Log error; continue in-memory; alert on repeated failures |
| WebSocket client disconnects | WebSocket Server | Remove from broadcast list; no action needed |
| WebSocket server unreachable | WebSocket Client | Exponential back-off reconnect; show staleness indicator |
| JWT expired | Dashboard API / WS Server | Return HTTP 401; client redirects to login |
| History range > 30 days | Dashboard API | Return HTTP 400 with descriptive message |
| Webhook delivery failure | Alerting Engine | Retry up to 3 times with 10s delay; mark `delivery_failed` |
| Invalid alert rule import | Dashboard API | Return HTTP 422 with per-rule validation errors |
| No snapshots in history range | Dashboard API | Return `[]` with HTTP 200 |

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
// Feature: admin-dashboard-metrics, Property N: <property_text>
```

Each correctness property (1–19) must be implemented by a single property-based test.

**Backend property tests** (`backend/src/__tests__/properties/`):
- `metricsCollector.property.test.ts` — Properties 1, 2, 3
- `dashboardApi.property.test.ts` — Properties 4, 5, 6
- `websocketServer.property.test.ts` — Properties 7, 8
- `alertingEngine.property.test.ts` — Properties 13, 14, 15, 16, 17
- `alertRules.property.test.ts` — Properties 18, 19

**Frontend property tests** (`frontend/tests/properties/`):
- `useMetricsWebSocket.property.test.ts` — Property 9, 12
- `adminDashboard.property.test.ts` — Properties 10, 11

### Unit Tests

**Backend unit tests** (`backend/src/__tests__/unit/`):
- `auth.test.ts` — JWT issuance (Req 8.1), expiry ≤ 8h (Req 8.4), 401 on missing token
- `metricsApi.test.ts` — `GET /api/metrics/current` returns latest snapshot (Req 2.1),
  `GET /api/config` returns correct fields (Req 2.3), empty range returns `[]` (Req 2.6)
- `websocket.test.ts` — on-connect initial snapshot delivery (Req 3.3),
  authenticated connection accepted (Req 3.1)
- `alerting.test.ts` — webhook POST sent on alert creation (Req 6.5)

**Frontend unit tests** (`frontend/tests/`):
- `ConnectionStatus.test.tsx` — "Reconnecting…" shown when disconnected (Req 4.5),
  staleness indicator shown during reconnect (Req 5.4),
  staleness indicator removed on reconnect (Req 5.5)

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

test('Property 2: health status derivation', () => {
  // Feature: admin-dashboard-metrics, Property 2: health status derivation correctness
  fc.assert(
    fc.property(
      fc.boolean(),           // paused
      fc.boolean(),           // shutdown_mode
      fc.bigInt({ min: 0n }), // reserve_balance
      fc.bigInt({ min: 0n }), // min_reserve_threshold
      (paused, shutdown_mode, reserve_balance, min_reserve_threshold) => {
        const status = deriveHealthStatus({ paused, shutdown_mode, reserve_balance, min_reserve_threshold });
        if (paused || shutdown_mode) return status === 'Critical';
        if (min_reserve_threshold > 0n && reserve_balance <= 2n * min_reserve_threshold) return status === 'Degraded';
        return status === 'Healthy';
      }
    ),
    { numRuns: 100 }
  );
});
```
