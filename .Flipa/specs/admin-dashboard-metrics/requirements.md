# Requirements Document

## Introduction

This feature adds a comprehensive real-time admin dashboard for the Tossd coinflip
contract. It introduces a metrics collection layer that reads on-chain `ContractStats`,
`ContractConfig`, and per-player data; a backend API that aggregates and exposes those
metrics; a WebSocket server that streams live updates to connected admin clients; and a
React/TypeScript dashboard UI that visualises contract health, game activity, reserve
levels, and fee revenue. An alerting subsystem notifies admins when configurable
thresholds are breached (e.g. reserve balance drops below a safety margin).

Closes #511.

---

## Glossary

- **Dashboard**: The React/TypeScript admin UI that displays real-time contract metrics.
- **Metrics_Collector**: The backend service responsible for polling the Soroban RPC
  and aggregating `ContractStats`, `ContractConfig`, and derived metrics into a
  `MetricsSnapshot`.
- **MetricsSnapshot**: A point-in-time record of all collected contract metrics,
  including reserve balance, total games, total volume, fee revenue, active game count,
  and contract health status.
- **Dashboard_API**: The HTTP REST API that serves historical `MetricsSnapshot` records
  and current contract configuration to the Dashboard.
- **WebSocket_Server**: The server-side component that maintains persistent WebSocket
  connections and pushes `MetricsSnapshot` updates to subscribed Dashboard clients.
- **WebSocket_Client**: The Dashboard-side hook that manages the WebSocket connection
  and delivers incoming `MetricsSnapshot` data to React components.
- **Alert**: A notification generated when a metric crosses a configured threshold.
- **Alert_Rule**: A named, configurable condition (threshold + metric + comparison
  operator) that the Alerting_Engine evaluates against each new `MetricsSnapshot`.
- **Alerting_Engine**: The backend component that evaluates `Alert_Rule` entries against
  incoming `MetricsSnapshot` data and emits `Alert` records.
- **Admin**: The privileged address stored in `ContractConfig.admin`; the only user
  permitted to access the Dashboard and manage `Alert_Rule` entries.
- **Contract**: The Tossd Soroban coinflip smart contract.
- **ContractStats**: The on-chain aggregate statistics struct (`total_games`,
  `total_volume`, `total_fees`, `reserve_balance`, `pool_size`, `mix_count`).
- **ContractConfig**: The on-chain contract configuration struct (`paused`,
  `shutdown_mode`, `fee_bps`, `min_wager`, `max_wager`, `min_reserve_threshold`).
- **Health_Status**: A derived enum (`Healthy`, `Degraded`, `Critical`) computed from
  reserve balance relative to `min_reserve_threshold` and pause/shutdown flags.
- **Soroban_RPC**: The Stellar Soroban JSON-RPC endpoint used to read on-chain state.

---

## Requirements

### Requirement 1: Metrics Collection

**User Story:** As an Admin, I want the system to continuously collect contract metrics
from the Soroban RPC, so that the dashboard always reflects current on-chain state.

#### Acceptance Criteria

1. THE Metrics_Collector SHALL poll the Soroban_RPC at a configurable interval
   (default: 5 seconds) to read `ContractStats` and `ContractConfig`.
2. WHEN a poll succeeds, THE Metrics_Collector SHALL produce a `MetricsSnapshot`
   containing: `timestamp`, `reserve_balance`, `total_games`, `total_volume`,
   `total_fees`, `fee_bps`, `paused`, `shutdown_mode`, `min_reserve_threshold`,
   and `health_status`.
3. THE Metrics_Collector SHALL derive `health_status` as follows:
   - `Critical` when `paused` is `true` or `shutdown_mode` is `true`
   - `Degraded` when `reserve_balance` is less than or equal to
     `2 × min_reserve_threshold` and `min_reserve_threshold` is greater than 0
   - `Healthy` otherwise
4. IF the Soroban_RPC call fails, THEN THE Metrics_Collector SHALL retain the
   most recent `MetricsSnapshot` and SHALL increment an internal `rpc_error_count`
   counter without crashing.
5. WHEN `rpc_error_count` reaches 3 consecutive failures, THE Metrics_Collector
   SHALL set `health_status` to `Critical` in the retained snapshot and SHALL
   emit an internal error event for the Alerting_Engine to process.
6. THE Metrics_Collector SHALL persist each successful `MetricsSnapshot` to a
   time-series store with a configurable retention period (default: 7 days).

---

### Requirement 2: Dashboard API

**User Story:** As an Admin, I want a REST API to query historical metrics and current
contract configuration, so that the dashboard can render charts and configuration panels.

#### Acceptance Criteria

1. THE Dashboard_API SHALL expose a `GET /api/metrics/current` endpoint that returns
   the most recent `MetricsSnapshot` as JSON within 500 ms.
2. THE Dashboard_API SHALL expose a `GET /api/metrics/history` endpoint that accepts
   `start` and `end` ISO-8601 timestamp query parameters and returns an array of
   `MetricsSnapshot` records within that range, ordered by ascending timestamp.
3. THE Dashboard_API SHALL expose a `GET /api/config` endpoint that returns the
   current `ContractConfig` fields relevant to the dashboard
   (`fee_bps`, `min_wager`, `max_wager`, `paused`, `shutdown_mode`,
   `min_reserve_threshold`) as JSON.
4. IF a request to `GET /api/metrics/history` specifies a range exceeding 30 days,
   THEN THE Dashboard_API SHALL return HTTP 400 with a descriptive error message.
5. THE Dashboard_API SHALL require a valid admin authentication token on all endpoints
   and SHALL return HTTP 401 for unauthenticated requests.
6. WHEN no `MetricsSnapshot` records exist for the requested history range, THE
   Dashboard_API SHALL return an empty array with HTTP 200.

---

### Requirement 3: WebSocket Metric Streaming

**User Story:** As an Admin, I want the dashboard to receive metric updates in real time
via WebSocket, so that I can monitor contract health without manually refreshing the page.

#### Acceptance Criteria

1. THE WebSocket_Server SHALL accept WebSocket connections from authenticated Dashboard
   clients on a dedicated endpoint (e.g. `ws://host/ws/metrics`).
2. WHEN a new `MetricsSnapshot` is produced by the Metrics_Collector, THE
   WebSocket_Server SHALL broadcast it to all connected WebSocket_Client instances
   within 1 second of snapshot creation.
3. WHEN a WebSocket_Client connects, THE WebSocket_Server SHALL immediately send the
   most recent `MetricsSnapshot` as the first message so the client renders current
   data without waiting for the next poll cycle.
4. IF a WebSocket connection drops, THEN THE WebSocket_Client SHALL attempt to
   reconnect with exponential back-off starting at 1 second, up to a maximum of
   30 seconds between attempts.
5. THE WebSocket_Server SHALL send a heartbeat ping to each connected client every
   15 seconds and SHALL close connections that do not respond within 5 seconds.
6. THE WebSocket_Server SHALL support at least 10 concurrent authenticated connections
   without degrading broadcast latency beyond 1 second.
7. IF an unauthenticated client attempts a WebSocket connection, THEN THE
   WebSocket_Server SHALL reject the handshake with HTTP 401 and SHALL NOT add the
   client to the broadcast list.

---

### Requirement 4: Dashboard UI — Metrics Display

**User Story:** As an Admin, I want a visual dashboard showing key contract metrics and
health status, so that I can assess contract state at a glance.

#### Acceptance Criteria

1. THE Dashboard SHALL display the following metrics from the most recent
   `MetricsSnapshot`: `reserve_balance` (in XLM), `total_games`, `total_volume`
   (in XLM), `total_fees` (in XLM), `fee_bps`, `paused`, and `health_status`.
2. THE Dashboard SHALL render `health_status` as a colour-coded indicator:
   green for `Healthy`, amber for `Degraded`, and red for `Critical`.
3. THE Dashboard SHALL display a time-series chart of `reserve_balance` over the
   last 24 hours, updated in real time as new `MetricsSnapshot` data arrives via
   the WebSocket_Client.
4. THE Dashboard SHALL display a time-series chart of `total_games` (game count per
   hour) over the last 24 hours.
5. WHEN the WebSocket_Client is disconnected, THE Dashboard SHALL display a
   visible "Reconnecting…" status indicator and SHALL NOT show stale data as current.
6. THE Dashboard SHALL show the timestamp of the most recent `MetricsSnapshot` so
   the Admin can verify data freshness.
7. THE Dashboard SHALL be accessible to keyboard-only navigation and SHALL meet
   WCAG 2.1 AA colour-contrast requirements for all metric indicators.

---

### Requirement 5: Dashboard UI — Real-Time Updates

**User Story:** As an Admin, I want the dashboard metrics to update automatically as
new data arrives, so that I do not need to reload the page to see the latest state.

#### Acceptance Criteria

1. THE WebSocket_Client SHALL maintain a persistent connection to the
   WebSocket_Server and SHALL update the Dashboard state on every received
   `MetricsSnapshot` message.
2. WHEN a new `MetricsSnapshot` arrives, THE Dashboard SHALL re-render affected
   metric panels within 100 ms of the message being received by the
   WebSocket_Client.
3. THE Dashboard SHALL animate transitions between metric values (e.g. counter
   increments) over no more than 300 ms to provide visual continuity.
4. WHILE the WebSocket_Client is in a reconnecting state, THE Dashboard SHALL
   continue to display the last known `MetricsSnapshot` values with a visual
   staleness indicator.
5. WHEN the WebSocket_Client successfully reconnects, THE Dashboard SHALL
   immediately request and display the current `MetricsSnapshot` and SHALL
   remove the staleness indicator.

---

### Requirement 6: Alerting System

**User Story:** As an Admin, I want to configure threshold-based alerts on contract
metrics, so that I am notified automatically when the contract enters a potentially
dangerous state.

#### Acceptance Criteria

1. THE Alerting_Engine SHALL evaluate each new `MetricsSnapshot` against all
   configured `Alert_Rule` entries.
2. THE Dashboard_API SHALL expose `POST /api/alerts/rules`, `GET /api/alerts/rules`,
   `PUT /api/alerts/rules/{id}`, and `DELETE /api/alerts/rules/{id}` endpoints for
   managing `Alert_Rule` entries; all endpoints SHALL require admin authentication.
3. AN `Alert_Rule` SHALL specify: `metric_name` (one of `reserve_balance`,
   `total_games`, `rpc_error_count`), `operator` (`lt`, `gt`, `eq`), `threshold`
   (numeric), `severity` (`warning` or `critical`), and `notification_channel`
   (`in_app` or `webhook`).
4. WHEN an `Alert_Rule` condition is met, THE Alerting_Engine SHALL create an
   `Alert` record containing: `rule_id`, `metric_name`, `observed_value`,
   `threshold`, `severity`, and `triggered_at` timestamp.
5. WHEN an `Alert` with `notification_channel = webhook` is created, THE
   Alerting_Engine SHALL send an HTTP POST to the configured webhook URL with the
   `Alert` record as JSON within 5 seconds of the `Alert` being created.
6. IF the webhook POST fails, THEN THE Alerting_Engine SHALL retry up to 3 times
   with a 10-second delay between attempts and SHALL mark the `Alert` as
   `delivery_failed` after all retries are exhausted.
7. THE Dashboard SHALL display active (unacknowledged) `Alert` records in a
   dedicated panel, ordered by `triggered_at` descending.
8. WHEN an Admin acknowledges an `Alert` in the Dashboard, THE Dashboard_API SHALL
   mark the `Alert` as acknowledged and SHALL remove it from the active alerts panel.
9. THE Alerting_Engine SHALL not re-trigger an `Alert_Rule` for the same condition
   until the metric has recovered (crossed back through the threshold) and then
   breached it again, preventing alert storms.

---

### Requirement 7: Alert Rule Serialization

**User Story:** As an Admin, I want to export and import alert rule configurations,
so that I can replicate alert setups across environments.

#### Acceptance Criteria

1. THE Dashboard_API SHALL expose a `GET /api/alerts/rules/export` endpoint that
   returns all `Alert_Rule` entries serialized as a JSON array.
2. THE Dashboard_API SHALL expose a `POST /api/alerts/rules/import` endpoint that
   accepts a JSON array of `Alert_Rule` entries and creates or updates rules
   accordingly.
3. THE Dashboard_API SHALL validate each imported `Alert_Rule` entry against the
   schema defined in Requirement 6, Criterion 3, and SHALL return HTTP 422 with
   a per-rule error list for any invalid entries.
4. FOR ALL valid `Alert_Rule` arrays, exporting then importing then exporting SHALL
   produce an equivalent JSON array (round-trip property).
5. IF an imported `Alert_Rule` has an `id` that matches an existing rule, THEN THE
   Dashboard_API SHALL update the existing rule rather than creating a duplicate.

---

### Requirement 8: Authentication and Access Control

**User Story:** As a security auditor, I want all dashboard endpoints and WebSocket
connections to require admin authentication, so that contract metrics and alert
configurations are not exposed to unauthorized parties.

#### Acceptance Criteria

1. THE Dashboard_API SHALL issue a signed JWT to the Admin upon successful
   authentication and SHALL require that JWT as a Bearer token on all subsequent
   requests.
2. THE WebSocket_Server SHALL validate the JWT provided in the WebSocket handshake
   before accepting the connection.
3. IF a JWT is expired or invalid, THEN THE Dashboard_API SHALL return HTTP 401
   and THE WebSocket_Server SHALL reject the connection with HTTP 401.
4. THE Dashboard_API SHALL enforce a JWT expiry of no more than 8 hours and SHALL
   provide a token-refresh endpoint.
5. THE Dashboard SHALL store the JWT in memory only and SHALL NOT persist it to
   `localStorage` or `sessionStorage`.
