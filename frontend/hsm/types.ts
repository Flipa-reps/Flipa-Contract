/**
 * HSM (Hardware Security Module) type definitions.
 *
 * Defines the core interfaces for HSM-backed cryptographic operations used
 * throughout the Flipa application. All signing and key management operations
 * should flow through these abstractions rather than touching raw key material.
 */

// ── Key types ─────────────────────────────────────────────────────────────────

/** Supported asymmetric key algorithms. */
export type KeyAlgorithm = "Ed25519" | "secp256k1";

/** Key usage scopes — a key may only be used for its declared purpose. */
export type KeyUsage = "sign" | "commitment" | "vrf";

/** Opaque key handle returned by the HSM; never contains raw key bytes. */
export interface HsmKeyHandle {
  /** Stable identifier for this key within the HSM slot. */
  readonly keyId: string;
  /** Algorithm the key was generated with. */
  readonly algorithm: KeyAlgorithm;
  /** Declared usage scope. */
  readonly usage: KeyUsage;
  /** ISO-8601 creation timestamp. */
  readonly createdAt: string;
  /** Whether the key is currently active and usable. */
  readonly active: boolean;
}

/** Raw public key bytes exported from the HSM (never private key bytes). */
export interface HsmPublicKey {
  readonly keyId: string;
  readonly algorithm: KeyAlgorithm;
  /** Hex-encoded public key bytes. */
  readonly publicKeyHex: string;
}

// ── Signing ───────────────────────────────────────────────────────────────────

/** Input to a signing operation. */
export interface SignRequest {
  /** Key handle to sign with. */
  keyId: string;
  /** Raw bytes to sign (will be hashed internally per algorithm). */
  message: Uint8Array;
  /** Optional context string mixed into the hash (domain separation). */
  context?: string;
}

/** Result of a signing operation. */
export interface SignResult {
  /** Hex-encoded signature bytes. */
  signatureHex: string;
  /** Key that produced the signature. */
  keyId: string;
  /** Algorithm used. */
  algorithm: KeyAlgorithm;
  /** Unix timestamp (ms) when the signature was produced. */
  timestamp: number;
}

// ── Commitment generation ─────────────────────────────────────────────────────

/** Request to generate a cryptographically strong commitment secret. */
export interface CommitmentRequest {
  /** Key handle to use for HMAC-based entropy mixing (optional). */
  keyId?: string;
  /** Additional entropy to mix in (e.g. player address). */
  context?: string;
}

/** A commitment secret + its SHA-256 hash, ready for the commit-reveal flow. */
export interface CommitmentResult {
  /** 32-byte secret as hex — keep this private until reveal time. */
  secretHex: string;
  /** SHA-256(secret) as hex — submit this on-chain as the commitment. */
  commitmentHex: string;
  /** Bytes32 representation for Soroban SDK (0x-prefixed). */
  commitmentBytes32: string;
}

// ── Key storage ───────────────────────────────────────────────────────────────

/** Encrypted key record persisted in secure storage. */
export interface EncryptedKeyRecord {
  keyId: string;
  algorithm: KeyAlgorithm;
  usage: KeyUsage;
  /** AES-GCM encrypted private key material (base64). */
  encryptedKeyMaterial: string;
  /** AES-GCM IV (base64). */
  iv: string;
  /** Salt used for key derivation (base64). */
  salt: string;
  createdAt: string;
  active: boolean;
}

// ── HSM provider interface ────────────────────────────────────────────────────

/**
 * Core HSM provider interface.
 *
 * Implementations include:
 * - `WebCryptoHsmProvider`  — browser WebCrypto API (software, dev/test)
 * - `HardwareHsmProvider`   — PKCS#11 / cloud HSM bridge (production)
 * - `FailoverHsmProvider`   — wraps primary + fallback with automatic retry
 */
export interface HsmProvider {
  /** Human-readable name for logging and diagnostics. */
  readonly name: string;

  /** Returns true if the provider is reachable and operational. */
  isAvailable(): Promise<boolean>;

  /** Generate a new key pair inside the HSM; returns an opaque handle. */
  generateKey(algorithm: KeyAlgorithm, usage: KeyUsage): Promise<HsmKeyHandle>;

  /** Import an existing key into the HSM (e.g. during migration). */
  importKey(
    privateKeyHex: string,
    algorithm: KeyAlgorithm,
    usage: KeyUsage,
  ): Promise<HsmKeyHandle>;

  /** Export the public key for a given key handle. */
  exportPublicKey(keyId: string): Promise<HsmPublicKey>;

  /** Sign a message with the specified key. */
  sign(request: SignRequest): Promise<SignResult>;

  /** Generate a commitment secret using HSM-grade entropy. */
  generateCommitment(request: CommitmentRequest): Promise<CommitmentResult>;

  /** List all key handles managed by this provider. */
  listKeys(): Promise<HsmKeyHandle[]>;

  /** Deactivate a key (soft-delete; key material is not destroyed). */
  deactivateKey(keyId: string): Promise<void>;
}

// ── Health / diagnostics ──────────────────────────────────────────────────────

export interface HsmHealthStatus {
  available: boolean;
  providerName: string;
  activeKeyCount: number;
  lastCheckedAt: string;
  /** Non-empty when the provider is degraded or unavailable. */
  errorMessage?: string;
}
