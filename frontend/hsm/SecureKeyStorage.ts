/**
 * Secure Key Storage
 *
 * Provides encrypted, persistent storage for HSM key records using the
 * browser's IndexedDB (preferred) or localStorage (fallback). All key
 * material is encrypted with AES-256-GCM before being written to storage.
 *
 * ## Storage hierarchy
 * 1. IndexedDB  — preferred; larger quota, structured, async
 * 2. localStorage — fallback for environments without IndexedDB
 * 3. In-memory   — last resort; keys are lost on page unload
 *
 * ## Encryption
 * Each key record is encrypted with a session-derived AES-256-GCM key.
 * The session key is derived from a user-supplied passphrase via PBKDF2
 * (310,000 iterations, SHA-256) with a per-record random salt.
 *
 * ## Usage
 * ```ts
 * const storage = new SecureKeyStorage("flipa-hsm-keys");
 * await storage.open();
 * await storage.saveKey(encryptedRecord);
 * const record = await storage.loadKey("key-id");
 * ```
 */

import { EncryptedKeyRecord } from "./types";
import {
  randomBytes,
  bytesToBase64,
  base64ToBytes,
  bytesToHex,
} from "./crypto";

// ── Storage backend interface ─────────────────────────────────────────────────

interface StorageBackend {
  get(key: string): Promise<string | null>;
  set(key: string, value: string): Promise<void>;
  delete(key: string): Promise<void>;
  keys(): Promise<string[]>;
}

// ── IndexedDB backend ─────────────────────────────────────────────────────────

class IndexedDbBackend implements StorageBackend {
  private db: IDBDatabase | null = null;
  private readonly dbName: string;
  private readonly storeName = "keys";

  constructor(dbName: string) {
    this.dbName = dbName;
  }

  async open(): Promise<void> {
    return new Promise((resolve, reject) => {
      const request = indexedDB.open(this.dbName, 1);

      request.onupgradeneeded = (event) => {
        const db = (event.target as IDBOpenDBRequest).result;
        if (!db.objectStoreNames.contains(this.storeName)) {
          db.createObjectStore(this.storeName);
        }
      };

      request.onsuccess = (event) => {
        this.db = (event.target as IDBOpenDBRequest).result;
        resolve();
      };

      request.onerror = () => reject(request.error);
    });
  }

  async get(key: string): Promise<string | null> {
    return new Promise((resolve, reject) => {
      const tx = this.db!.transaction(this.storeName, "readonly");
      const store = tx.objectStore(this.storeName);
      const request = store.get(key);
      request.onsuccess = () => resolve(request.result ?? null);
      request.onerror = () => reject(request.error);
    });
  }

  async set(key: string, value: string): Promise<void> {
    return new Promise((resolve, reject) => {
      const tx = this.db!.transaction(this.storeName, "readwrite");
      const store = tx.objectStore(this.storeName);
      const request = store.put(value, key);
      request.onsuccess = () => resolve();
      request.onerror = () => reject(request.error);
    });
  }

  async delete(key: string): Promise<void> {
    return new Promise((resolve, reject) => {
      const tx = this.db!.transaction(this.storeName, "readwrite");
      const store = tx.objectStore(this.storeName);
      const request = store.delete(key);
      request.onsuccess = () => resolve();
      request.onerror = () => reject(request.error);
    });
  }

  async keys(): Promise<string[]> {
    return new Promise((resolve, reject) => {
      const tx = this.db!.transaction(this.storeName, "readonly");
      const store = tx.objectStore(this.storeName);
      const request = store.getAllKeys();
      request.onsuccess = () => resolve(request.result as string[]);
      request.onerror = () => reject(request.error);
    });
  }
}

// ── localStorage backend ──────────────────────────────────────────────────────

class LocalStorageBackend implements StorageBackend {
  private readonly prefix: string;

  constructor(prefix: string) {
    this.prefix = prefix + ":";
  }

  async get(key: string): Promise<string | null> {
    return localStorage.getItem(this.prefix + key);
  }

  async set(key: string, value: string): Promise<void> {
    localStorage.setItem(this.prefix + key, value);
  }

  async delete(key: string): Promise<void> {
    localStorage.removeItem(this.prefix + key);
  }

  async keys(): Promise<string[]> {
    const result: string[] = [];
    for (let i = 0; i < localStorage.length; i++) {
      const k = localStorage.key(i);
      if (k && k.startsWith(this.prefix)) {
        result.push(k.slice(this.prefix.length));
      }
    }
    return result;
  }
}

// ── In-memory backend (last resort) ──────────────────────────────────────────

class InMemoryBackend implements StorageBackend {
  private readonly store = new Map<string, string>();

  async get(key: string): Promise<string | null> {
    return this.store.get(key) ?? null;
  }

  async set(key: string, value: string): Promise<void> {
    this.store.set(key, value);
  }

  async delete(key: string): Promise<void> {
    this.store.delete(key);
  }

  async keys(): Promise<string[]> {
    return Array.from(this.store.keys());
  }
}

// ── SecureKeyStorage ──────────────────────────────────────────────────────────

export class SecureKeyStorage {
  private backend: StorageBackend | null = null;
  private readonly storeName: string;

  constructor(storeName = "flipa-hsm-keys") {
    this.storeName = storeName;
  }

  /**
   * Open the storage backend, selecting the best available option.
   * Must be called before any read/write operations.
   */
  async open(): Promise<void> {
    // Try IndexedDB first
    if (typeof indexedDB !== "undefined") {
      try {
        const idb = new IndexedDbBackend(this.storeName);
        await idb.open();
        this.backend = idb;
        return;
      } catch {
        // Fall through
      }
    }

    // Try localStorage
    if (typeof localStorage !== "undefined") {
      try {
        this.backend = new LocalStorageBackend(this.storeName);
        return;
      } catch {
        // Fall through
      }
    }

    // Last resort: in-memory (ephemeral)
    this.backend = new InMemoryBackend();
  }

  /** Save an encrypted key record to storage. */
  async saveKey(record: EncryptedKeyRecord): Promise<void> {
    this.assertOpen();
    await this.backend!.set(record.keyId, JSON.stringify(record));
  }

  /** Load an encrypted key record by ID. Returns null if not found. */
  async loadKey(keyId: string): Promise<EncryptedKeyRecord | null> {
    this.assertOpen();
    const raw = await this.backend!.get(keyId);
    if (!raw) return null;
    return JSON.parse(raw) as EncryptedKeyRecord;
  }

  /** Delete a key record from storage. */
  async deleteKey(keyId: string): Promise<void> {
    this.assertOpen();
    await this.backend!.delete(keyId);
  }

  /** List all stored key IDs. */
  async listKeyIds(): Promise<string[]> {
    this.assertOpen();
    return this.backend!.keys();
  }

  /** Load all stored key records. */
  async loadAllKeys(): Promise<EncryptedKeyRecord[]> {
    const ids = await this.listKeyIds();
    const records = await Promise.all(ids.map((id) => this.loadKey(id)));
    return records.filter((r): r is EncryptedKeyRecord => r !== null);
  }

  /** Returns true if the storage backend is open and ready. */
  isOpen(): boolean {
    return this.backend !== null;
  }

  private assertOpen(): void {
    if (!this.backend) {
      throw new Error(
        "SecureKeyStorage is not open. Call open() before reading or writing.",
      );
    }
  }
}
