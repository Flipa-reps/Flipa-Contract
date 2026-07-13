/**
 * Immutable Audit Log
 *
 * Append-only, tamper-evident log of security events. Each entry is linked
 * to the previous via a SHA-256 hash chain — any modification to a past
 * entry invalidates every subsequent hash, making tampering detectable.
 *
 * ## Tamper-evidence mechanism
 *
 * Each event carries two hashes:
 *   - `previousHash` — SHA-256 of the previous event's canonical JSON
 *   - `eventHash`    — SHA-256 of this event's canonical JSON (including
 *                      `previousHash`), forming a linked chain
 *
 * The genesis event uses `previousHash = "0".repeat(64)` (all-zero sentinel).
 *
 * Verification walks the chain and recomputes every hash; any mismatch
 * indicates tampering or corruption.
 *
 * ## Persistence
 *
 * Events are flushed to IndexedDB (preferred) or localStorage (fallback)
 * after every append. The in-memory chain is the authoritative source
 * during a session; persistence is for forensic recovery.
 *
 * ## Capacity
 *
 * The in-memory ring buffer retains the most recent `maxEntries` events
 * (default 10,000). Older events are evicted from memory but remain in
 * persistent storage.
 */

import { SecurityEvent, SecurityEventBase } from "./types";
import { sha256, bytesToHex } from "../hsm/crypto";

// ── Constants ─────────────────────────────────────────────────────────────────

/** Sentinel hash used as `previousHash` for the first event in the chain. */
export const GENESIS_HASH = "0".repeat(64);

/** Default maximum in-memory entries before ring-buffer eviction. */
const DEFAULT_MAX_ENTRIES = 10_000;

// ── Verification result ───────────────────────────────────────────────────────

export interface ChainVerificationResult {
  valid: boolean;
  /** Total events checked. */
  checkedCount: number;
  /** Index of the first tampered event (-1 if none). */
  firstTamperedIndex: number;
  /** Details of each broken link. */
  violations: Array<{
    index: number;
    eventId: string;
    expected: string;
    actual: string;
  }>;
}

// ── Storage backend (same pattern as SecureKeyStorage) ────────────────────────

interface LogStorageBackend {
  append(event: SecurityEvent): Promise<void>;
  loadAll(): Promise<SecurityEvent[]>;
  clear(): Promise<void>;
}

class IndexedDbLogBackend implements LogStorageBackend {
  private db: IDBDatabase | null = null;
  private readonly dbName: string;
  private readonly storeName = "security_events";

  constructor(dbName: string) {
    this.dbName = dbName;
  }

  async open(): Promise<void> {
    return new Promise((resolve, reject) => {
      const req = indexedDB.open(this.dbName, 1);
      req.onupgradeneeded = (e) => {
        const db = (e.target as IDBOpenDBRequest).result;
        if (!db.objectStoreNames.contains(this.storeName)) {
          const store = db.createObjectStore(this.storeName, {
            keyPath: "id",
          });
          store.createIndex("timestamp", "timestamp");
          store.createIndex("category", "category");
          store.createIndex("severity", "severity");
          store.createIndex("actor", "actor");
        }
      };
      req.onsuccess = (e) => {
        this.db = (e.target as IDBOpenDBRequest).result;
        resolve();
      };
      req.onerror = () => reject(req.error);
    });
  }

  async append(event: SecurityEvent): Promise<void> {
    return new Promise((resolve, reject) => {
      const tx = this.db!.transaction(this.storeName, "readwrite");
      const store = tx.objectStore(this.storeName);
      const req = store.put(event);
      req.onsuccess = () => resolve();
      req.onerror = () => reject(req.error);
    });
  }

  async loadAll(): Promise<SecurityEvent[]> {
    return new Promise((resolve, reject) => {
      const tx = this.db!.transaction(this.storeName, "readonly");
      const store = tx.objectStore(this.storeName);
      const idx = store.index("timestamp");
      const req = idx.getAll();
      req.onsuccess = () => resolve(req.result as SecurityEvent[]);
      req.onerror = () => reject(req.error);
    });
  }

  async clear(): Promise<void> {
    return new Promise((resolve, reject) => {
      const tx = this.db!.transaction(this.storeName, "readwrite");
      const store = tx.objectStore(this.storeName);
      const req = store.clear();
      req.onsuccess = () => resolve();
      req.onerror = () => reject(req.error);
    });
  }
}

class LocalStorageLogBackend implements LogStorageBackend {
  private readonly key: string;

  constructor(key: string) {
    this.key = key;
  }

  async append(event: SecurityEvent): Promise<void> {
    const existing = await this.loadAll();
    existing.push(event);
    localStorage.setItem(this.key, JSON.stringify(existing));
  }

  async loadAll(): Promise<SecurityEvent[]> {
    const raw = localStorage.getItem(this.key);
    if (!raw) return [];
    try {
      return JSON.parse(raw) as SecurityEvent[];
    } catch {
      return [];
    }
  }

  async clear(): Promise<void> {
    localStorage.removeItem(this.key);
  }
}

class InMemoryLogBackend implements LogStorageBackend {
  private readonly events: SecurityEvent[] = [];

  async append(event: SecurityEvent): Promise<void> {
    this.events.push(event);
  }

  async loadAll(): Promise<SecurityEvent[]> {
    return [...this.events];
  }

  async clear(): Promise<void> {
    this.events.length = 0;
  }
}

// ── ImmutableAuditLog ─────────────────────────────────────────────────────────

export class ImmutableAuditLog {
  private readonly chain: SecurityEvent[] = [];
  private backend: LogStorageBackend | null = null;
  private readonly maxEntries: number;
  private readonly storeName: string;

  constructor(
    options: {
      storeName?: string;
      maxEntries?: number;
    } = {},
  ) {
    this.storeName = options.storeName ?? "flipa-security-log";
    this.maxEntries = options.maxEntries ?? DEFAULT_MAX_ENTRIES;
  }

  // ── Lifecycle ─────────────────────────────────────────────────────────────

  /**
   * Open the persistent backend and replay stored events into the in-memory
   * chain. Must be called before `append`.
   */
  async open(): Promise<void> {
    this.backend = await this.openBackend();
    const stored = await this.backend.loadAll();
    // Sort by timestamp to restore chain order
    stored.sort((a, b) => a.timestamp.localeCompare(b.timestamp));
    for (const event of stored) {
      this.chain.push(event);
    }
    // Trim to maxEntries (keep newest)
    while (this.chain.length > this.maxEntries) {
      this.chain.shift();
    }
  }

  // ── Append ────────────────────────────────────────────────────────────────

  /**
   * Append a pre-built security event to the chain.
   * The event must already have `previousHash` and `eventHash` set correctly.
   * Use `SecurityEventEmitter` to build events — it handles hashing.
   */
  async append(event: SecurityEvent): Promise<void> {
    this.chain.push(event);
    // Evict oldest from memory when cap is reached
    while (this.chain.length > this.maxEntries) {
      this.chain.shift();
    }
    // Persist asynchronously; don't block the caller
    if (this.backend) {
      await this.backend.append(event).catch(() => {
        // Persistence failure must not break the in-memory chain
      });
    }
  }

  // ── Query ─────────────────────────────────────────────────────────────────

  /** Return all in-memory events (newest last). */
  getAll(): readonly SecurityEvent[] {
    return [...this.chain];
  }

  /** Return the most recent `n` events. */
  getRecent(n: number): readonly SecurityEvent[] {
    return this.chain.slice(-n);
  }

  /** Return the current tail hash (hash of the last event, or GENESIS_HASH). */
  getTailHash(): string {
    if (this.chain.length === 0) return GENESIS_HASH;
    return this.chain[this.chain.length - 1].eventHash;
  }

  /** Total number of events in the in-memory chain. */
  get size(): number {
    return this.chain.length;
  }

  // ── Verification ──────────────────────────────────────────────────────────

  /**
   * Walk the entire chain and verify every hash link.
   * Returns a detailed report of any violations found.
   *
   * Time complexity: O(n) where n = chain length.
   */
  async verify(): Promise<ChainVerificationResult> {
    const violations: ChainVerificationResult["violations"] = [];
    let expectedPrevious = GENESIS_HASH;

    for (let i = 0; i < this.chain.length; i++) {
      const event = this.chain[i];

      // 1. Check previousHash link
      if (event.previousHash !== expectedPrevious) {
        violations.push({
          index: i,
          eventId: event.id,
          expected: expectedPrevious,
          actual: event.previousHash,
        });
      }

      // 2. Recompute eventHash and verify
      const recomputed = await computeEventHash(event);
      if (recomputed !== event.eventHash) {
        violations.push({
          index: i,
          eventId: event.id,
          expected: recomputed,
          actual: event.eventHash,
        });
      }

      expectedPrevious = event.eventHash;
    }

    return {
      valid: violations.length === 0,
      checkedCount: this.chain.length,
      firstTamperedIndex: violations.length > 0 ? violations[0].index : -1,
      violations,
    };
  }

  // ── Export ────────────────────────────────────────────────────────────────

  /**
   * Export the full chain as a JSON string for forensic archival.
   * Includes a top-level `chainHash` covering all events for quick integrity
   * checks without replaying the full chain.
   */
  async export(): Promise<string> {
    const chainHash = await computeChainHash(this.chain);
    return JSON.stringify(
      {
        exportedAt: new Date().toISOString(),
        eventCount: this.chain.length,
        tailHash: this.getTailHash(),
        chainHash,
        events: this.chain,
      },
      null,
      2,
    );
  }

  /**
   * Clear the in-memory chain and persistent storage.
   * Irreversible — use only for testing or explicit log rotation.
   */
  async clear(): Promise<void> {
    this.chain.length = 0;
    if (this.backend) {
      await this.backend.clear();
    }
  }

  // ── Private ───────────────────────────────────────────────────────────────

  private async openBackend(): Promise<LogStorageBackend> {
    if (typeof indexedDB !== "undefined") {
      try {
        const idb = new IndexedDbLogBackend(this.storeName);
        await idb.open();
        return idb;
      } catch {
        // fall through
      }
    }
    if (typeof localStorage !== "undefined") {
      try {
        return new LocalStorageLogBackend(this.storeName);
      } catch {
        // fall through
      }
    }
    return new InMemoryLogBackend();
  }
}

// ── Hash helpers (exported for use by SecurityEventEmitter) ───────────────────

/**
 * Compute the canonical SHA-256 hash of a security event.
 *
 * The canonical form is deterministic JSON with keys sorted alphabetically,
 * excluding the `eventHash` field itself (which is what we're computing).
 */
export async function computeEventHash(
  event: Omit<SecurityEventBase, "eventHash"> & { eventHash?: string },
): Promise<string> {
  // Build a copy without eventHash so we don't hash the hash
  const { eventHash: _ignored, ...rest } = event as SecurityEventBase;
  const canonical = canonicalJson(rest);
  const bytes = new TextEncoder().encode(canonical);
  const digest = await sha256(bytes);
  return bytesToHex(digest);
}

/**
 * Compute a single hash covering the entire chain (Merkle-style root).
 * Hashes the canonical JSON of every event so that any field mutation —
 * including fields not reflected in eventHash on a shallow copy — is detected.
 */
export async function computeChainHash(
  events: readonly SecurityEvent[],
): Promise<string> {
  if (events.length === 0) return GENESIS_HASH;
  const allCanonical = events.map((e) => canonicalJson(e)).join("|");
  const bytes = new TextEncoder().encode(allCanonical);
  const digest = await sha256(bytes);
  return bytesToHex(digest);
}

/**
 * Produce deterministic JSON with keys sorted alphabetically.
 * Ensures the same event always produces the same hash regardless of
 * property insertion order.
 */
export function canonicalJson(value: unknown): string {
  if (value === null || typeof value !== "object") {
    return JSON.stringify(value);
  }
  if (Array.isArray(value)) {
    return "[" + value.map(canonicalJson).join(",") + "]";
  }
  const sorted = Object.keys(value as object)
    .sort()
    .map(
      (k) =>
        JSON.stringify(k) +
        ":" +
        canonicalJson((value as Record<string, unknown>)[k]),
    )
    .join(",");
  return "{" + sorted + "}";
}
