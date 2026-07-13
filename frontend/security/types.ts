/**
 * Security event logging types
 *
 * Defines the comprehensive security event taxonomy for the Flipa application.
 * All security-relevant operations emit typed events that are aggregated into
 * an immutable, tamper-evident audit log.
 *
 * ## Event categories
 * - Authentication: wallet connect/disconnect, session lifecycle
 * - Authorization: access control decisions, role checks
 * - Cryptographic: key generation, signing, commitment generation
 * - Transaction: game operations, fund movements
 * - System: HSM failover, configuration changes, errors
 * - Anomaly: rate limit violations, suspicious patterns
 */

// ── Event severity ────────────────────────────────────────────────────────────

export type SecurityEventSeverity =
  | "info" // Normal operation
  | "warning" // Unusual but not necessarily malicious
  | "error" // Operation failed
  | "critical"; // Security violation or system compromise

// ── Event categories ──────────────────────────────────────────────────────────

export type SecurityEventCategory =
  | "authentication"
  | "authorization"
  | "cryptographic"
  | "transaction"
  | "system"
  | "anomaly";

// ── Base event ────────────────────────────────────────────────────────────────

/**
 * Base security event structure.
 * All concrete event types extend this interface.
 */
export interface SecurityEventBase {
  /** Unique event ID (UUID v4). */
  readonly id: string;
  /** ISO-8601 timestamp with millisecond precision. */
  readonly timestamp: string;
  /** Event category for filtering and aggregation. */
  readonly category: SecurityEventCategory;
  /** Severity level for alerting and prioritization. */
  readonly severity: SecurityEventSeverity;
  /** Human-readable event type discriminator. */
  readonly type: string;
  /** Actor who triggered the event (wallet address, system, etc.). */
  readonly actor: string;
  /** IP address or client identifier (optional, for network-level tracking). */
  readonly clientId?: string;
  /** Session ID for correlation across events. */
  readonly sessionId?: string;
  /** Free-form metadata for event-specific context. */
  readonly metadata: Record<string, unknown>;
  /** SHA-256 hash of the previous event in the chain (tamper detection). */
  readonly previousHash: string;
  /** SHA-256 hash of this event's canonical representation. */
  readonly eventHash: string;
}

// ── Authentication events ─────────────────────────────────────────────────────

export interface WalletConnectEvent extends SecurityEventBase {
  category: "authentication";
  type: "wallet.connect";
  metadata: {
    walletType: "freighter" | "albedo" | "xbull" | "rabet";
    walletAddress: string;
    publicKey: string;
  };
}

export interface WalletDisconnectEvent extends SecurityEventBase {
  category: "authentication";
  type: "wallet.disconnect";
  metadata: {
    walletAddress: string;
    reason: "user" | "timeout" | "error";
  };
}

export interface SessionStartEvent extends SecurityEventBase {
  category: "authentication";
  type: "session.start";
  metadata: {
    walletAddress: string;
    expiresAt: string;
  };
}

export interface SessionEndEvent extends SecurityEventBase {
  category: "authentication";
  type: "session.end";
  metadata: {
    walletAddress: string;
    reason: "logout" | "timeout" | "revoked";
    durationMs: number;
  };
}

// ── Authorization events ──────────────────────────────────────────────────────

export interface AccessGrantedEvent extends SecurityEventBase {
  category: "authorization";
  type: "access.granted";
  metadata: {
    resource: string;
    action: string;
    walletAddress: string;
  };
}

export interface AccessDeniedEvent extends SecurityEventBase {
  category: "authorization";
  type: "access.denied";
  severity: "warning";
  metadata: {
    resource: string;
    action: string;
    walletAddress: string;
    reason: string;
  };
}

// ── Cryptographic events ──────────────────────────────────────────────────────

export interface KeyGeneratedEvent extends SecurityEventBase {
  category: "cryptographic";
  type: "key.generated";
  metadata: {
    keyId: string;
    algorithm: string;
    usage: string;
    provider: string;
  };
}

export interface KeyDeactivatedEvent extends SecurityEventBase {
  category: "cryptographic";
  type: "key.deactivated";
  metadata: {
    keyId: string;
    reason: string;
  };
}

export interface SigningOperationEvent extends SecurityEventBase {
  category: "cryptographic";
  type: "signing.operation";
  metadata: {
    keyId: string;
    operation: string;
    signatureHex: string;
    payloadHash: string;
  };
}

export interface CommitmentGeneratedEvent extends SecurityEventBase {
  category: "cryptographic";
  type: "commitment.generated";
  metadata: {
    keyId?: string;
    commitmentHash: string;
    context: string;
  };
}

// ── Transaction events ────────────────────────────────────────────────────────

export interface GameStartedEvent extends SecurityEventBase {
  category: "transaction";
  type: "game.started";
  metadata: {
    gameId: string;
    walletAddress: string;
    wagerStroops: number;
    side: "heads" | "tails";
    commitmentHash: string;
    txHash: string;
  };
}

export interface GameRevealedEvent extends SecurityEventBase {
  category: "transaction";
  type: "game.revealed";
  metadata: {
    gameId: string;
    walletAddress: string;
    outcome: "win" | "loss";
    txHash: string;
  };
}

export interface GameCashedOutEvent extends SecurityEventBase {
  category: "transaction";
  type: "game.cashedout";
  metadata: {
    gameId: string;
    walletAddress: string;
    payoutStroops: number;
    txHash: string;
  };
}

export interface GameContinuedEvent extends SecurityEventBase {
  category: "transaction";
  type: "game.continued";
  metadata: {
    gameId: string;
    walletAddress: string;
    newCommitmentHash: string;
    txHash: string;
  };
}

// ── System events ─────────────────────────────────────────────────────────────

export interface HsmFailoverEvent extends SecurityEventBase {
  category: "system";
  type: "hsm.failover";
  severity: "warning";
  metadata: {
    failedProvider: string;
    activeProvider: string;
    operation: string;
    reason: string;
  };
}

export interface HsmRecoveryEvent extends SecurityEventBase {
  category: "system";
  type: "hsm.recovery";
  metadata: {
    provider: string;
    downDurationMs: number;
  };
}

export interface ConfigurationChangedEvent extends SecurityEventBase {
  category: "system";
  type: "config.changed";
  metadata: {
    changedBy: string;
    field: string;
    oldValue: unknown;
    newValue: unknown;
  };
}

export interface ErrorEvent extends SecurityEventBase {
  category: "system";
  severity: "error";
  type: "error";
  metadata: {
    errorType: string;
    errorMessage: string;
    stackTrace?: string;
    context: Record<string, unknown>;
  };
}

// ── Anomaly events ────────────────────────────────────────────────────────────

export interface RateLimitExceededEvent extends SecurityEventBase {
  category: "anomaly";
  severity: "warning";
  type: "ratelimit.exceeded";
  metadata: {
    walletAddress: string;
    operation: string;
    limit: number;
    actual: number;
    windowMs: number;
  };
}

export interface SuspiciousPatternEvent extends SecurityEventBase {
  category: "anomaly";
  severity: "warning";
  type: "pattern.suspicious";
  metadata: {
    walletAddress: string;
    pattern: string;
    description: string;
    confidence: number; // 0.0 - 1.0
  };
}

export interface CommitmentReplayAttemptEvent extends SecurityEventBase {
  category: "anomaly";
  severity: "critical";
  type: "commitment.replay";
  metadata: {
    walletAddress: string;
    commitmentHash: string;
    originalGameId: string;
    attemptedGameId: string;
  };
}

// ── Union type ────────────────────────────────────────────────────────────────

export type SecurityEvent =
  | WalletConnectEvent
  | WalletDisconnectEvent
  | SessionStartEvent
  | SessionEndEvent
  | AccessGrantedEvent
  | AccessDeniedEvent
  | KeyGeneratedEvent
  | KeyDeactivatedEvent
  | SigningOperationEvent
  | CommitmentGeneratedEvent
  | GameStartedEvent
  | GameRevealedEvent
  | GameCashedOutEvent
  | GameContinuedEvent
  | HsmFailoverEvent
  | HsmRecoveryEvent
  | ConfigurationChangedEvent
  | ErrorEvent
  | RateLimitExceededEvent
  | SuspiciousPatternEvent
  | CommitmentReplayAttemptEvent;

// ── Event filter ──────────────────────────────────────────────────────────────

export interface SecurityEventFilter {
  /** Filter by category (OR logic if multiple). */
  categories?: SecurityEventCategory[];
  /** Filter by severity (OR logic if multiple). */
  severities?: SecurityEventSeverity[];
  /** Filter by actor (exact match). */
  actor?: string;
  /** Filter by session ID (exact match). */
  sessionId?: string;
  /** Filter by time range (inclusive). */
  timeRange?: {
    start: string; // ISO-8601
    end: string; // ISO-8601
  };
  /** Filter by event type (OR logic if multiple). */
  types?: string[];
}

// ── Aggregation result ────────────────────────────────────────────────────────

export interface SecurityEventAggregation {
  /** Total event count matching the filter. */
  totalCount: number;
  /** Breakdown by category. */
  byCategory: Record<SecurityEventCategory, number>;
  /** Breakdown by severity. */
  bySeverity: Record<SecurityEventSeverity, number>;
  /** Breakdown by actor (top 10). */
  byActor: Array<{ actor: string; count: number }>;
  /** Time range covered. */
  timeRange: {
    earliest: string;
    latest: string;
  };
}
