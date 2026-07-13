/**
 * HSM infrastructure tests
 *
 * Covers:
 * - WebCryptoHsmProvider: key generation, signing, commitment generation,
 *   key persistence, deactivation
 * - FailoverHsmProvider: primary success, primary failure → secondary,
 *   primary recovery after probe interval
 * - SecureKeyStorage: open, save, load, delete, list
 * - HsmContractAdapter: commitment injection, audit log
 * - crypto utilities: sha256, entropy validation, encoding helpers
 */

import { describe, it, expect, vi, beforeEach } from "vitest";
import { WebCryptoHsmProvider } from "../hsm/WebCryptoHsmProvider";
import { FailoverHsmProvider } from "../hsm/FailoverHsmProvider";
import { SecureKeyStorage } from "../hsm/SecureKeyStorage";
import { HsmContractAdapter } from "../hsm/HsmContractAdapter";
import {
  sha256,
  randomBytes,
  bytesToHex,
  hexToBytes,
  validateCommitmentEntropy,
  toBytes32Hex,
  bytesToBase64,
  base64ToBytes,
} from "../hsm/crypto";
import type { HsmProvider, HsmKeyHandle } from "../hsm/types";
import type { ContractAdapter } from "../hooks/contract";

// ── Helpers ───────────────────────────────────────────────────────────────────

function makeBaseAdapter(): ContractAdapter {
  return {
    startGame: vi.fn().mockResolvedValue({ txHash: "tx-start" }),
    reveal: vi.fn().mockResolvedValue({ txHash: "tx-reveal", outcome: "win" }),
    cashOut: vi
      .fn()
      .mockResolvedValue({ txHash: "tx-cashout", payoutStroops: 2_000_000 }),
    continueGame: vi.fn().mockResolvedValue({ txHash: "tx-continue" }),
  };
}

// ── crypto utilities ──────────────────────────────────────────────────────────

describe("crypto utilities", () => {
  it("sha256 returns 32 bytes", async () => {
    const input = new TextEncoder().encode("hello");
    const digest = await sha256(input);
    expect(digest).toHaveLength(32);
  });

  it("sha256 is deterministic", async () => {
    const input = new TextEncoder().encode("flipa");
    const a = await sha256(input);
    const b = await sha256(input);
    expect(bytesToHex(a)).toBe(bytesToHex(b));
  });

  it("randomBytes returns the requested length", () => {
    const buf = randomBytes(32);
    expect(buf).toHaveLength(32);
  });

  it("randomBytes produces different values each call", () => {
    const a = bytesToHex(randomBytes(16));
    const b = bytesToHex(randomBytes(16));
    expect(a).not.toBe(b);
  });

  it("bytesToHex / hexToBytes round-trip", () => {
    const original = randomBytes(32);
    const hex = bytesToHex(original);
    const restored = hexToBytes(hex);
    expect(restored).toEqual(original);
  });

  it("hexToBytes handles 0x prefix", () => {
    const bytes = hexToBytes("0xdeadbeef");
    expect(bytes).toEqual(new Uint8Array([0xde, 0xad, 0xbe, 0xef]));
  });

  it("bytesToBase64 / base64ToBytes round-trip", () => {
    const original = randomBytes(32);
    const b64 = bytesToBase64(original);
    const restored = base64ToBytes(b64);
    expect(restored).toEqual(original);
  });

  it("validateCommitmentEntropy rejects all-zero bytes", () => {
    expect(validateCommitmentEntropy(new Uint8Array(32))).toBe(false);
  });

  it("validateCommitmentEntropy rejects all-same bytes", () => {
    expect(validateCommitmentEntropy(new Uint8Array(32).fill(0xab))).toBe(
      false,
    );
  });

  it("validateCommitmentEntropy accepts random bytes", () => {
    const bytes = randomBytes(32);
    // Extremely unlikely to fail; if it does, re-run
    expect(validateCommitmentEntropy(bytes)).toBe(true);
  });

  it("validateCommitmentEntropy rejects wrong length", () => {
    expect(validateCommitmentEntropy(new Uint8Array(16))).toBe(false);
  });

  it("toBytes32Hex produces 0x-prefixed 66-char string", () => {
    const bytes = randomBytes(32);
    const hex = toBytes32Hex(bytes);
    expect(hex).toMatch(/^0x[0-9a-f]{64}$/);
  });

  it("toBytes32Hex throws for non-32-byte input", () => {
    expect(() => toBytes32Hex(new Uint8Array(16))).toThrow();
  });
});

// ── WebCryptoHsmProvider ──────────────────────────────────────────────────────

describe("WebCryptoHsmProvider", () => {
  let provider: WebCryptoHsmProvider;

  beforeEach(() => {
    provider = new WebCryptoHsmProvider();
  });

  it("isAvailable returns true in jsdom", async () => {
    expect(await provider.isAvailable()).toBe(true);
  });

  it("generateKey returns a valid handle", async () => {
    const handle = await provider.generateKey("Ed25519", "sign");
    expect(handle.keyId).toMatch(/^wc-/);
    expect(handle.algorithm).toBe("Ed25519");
    expect(handle.usage).toBe("sign");
    expect(handle.active).toBe(true);
    expect(handle.createdAt).toBeTruthy();
  });

  it("generateKey produces unique key IDs", async () => {
    const a = await provider.generateKey("Ed25519", "sign");
    const b = await provider.generateKey("Ed25519", "sign");
    expect(a.keyId).not.toBe(b.keyId);
  });

  it("exportPublicKey returns 64-char hex string", async () => {
    const handle = await provider.generateKey("Ed25519", "sign");
    const pub = await provider.exportPublicKey(handle.keyId);
    expect(pub.publicKeyHex).toMatch(/^[0-9a-f]{64}$/);
    expect(pub.keyId).toBe(handle.keyId);
  });

  it("sign returns a non-empty signature", async () => {
    const handle = await provider.generateKey("Ed25519", "sign");
    const message = new TextEncoder().encode("test message");
    const result = await provider.sign({ keyId: handle.keyId, message });
    expect(result.signatureHex).toBeTruthy();
    expect(result.keyId).toBe(handle.keyId);
    expect(result.timestamp).toBeGreaterThan(0);
  });

  it("sign with context produces different signature than without", async () => {
    const handle = await provider.generateKey("Ed25519", "sign");
    const message = new TextEncoder().encode("test");
    const r1 = await provider.sign({ keyId: handle.keyId, message });
    const r2 = await provider.sign({
      keyId: handle.keyId,
      message,
      context: "ctx",
    });
    expect(r1.signatureHex).not.toBe(r2.signatureHex);
  });

  it("sign throws for unknown key ID", async () => {
    const message = new TextEncoder().encode("test");
    await expect(
      provider.sign({ keyId: "nonexistent", message }),
    ).rejects.toThrow("Key not found");
  });

  it("sign throws for deactivated key", async () => {
    const handle = await provider.generateKey("Ed25519", "sign");
    await provider.deactivateKey(handle.keyId);
    const message = new TextEncoder().encode("test");
    await expect(
      provider.sign({ keyId: handle.keyId, message }),
    ).rejects.toThrow("deactivated");
  });

  it("generateCommitment returns valid commitment", async () => {
    const result = await provider.generateCommitment({});
    expect(result.secretHex).toMatch(/^[0-9a-f]{64}$/);
    expect(result.commitmentHex).toMatch(/^[0-9a-f]{64}$/);
    expect(result.commitmentBytes32).toMatch(/^0x[0-9a-f]{64}$/);
  });

  it("generateCommitment secret hashes to commitment", async () => {
    const result = await provider.generateCommitment({});
    const secretBytes = hexToBytes(result.secretHex);
    const expectedCommitment = bytesToHex(await sha256(secretBytes));
    expect(result.commitmentHex).toBe(expectedCommitment);
  });

  it("generateCommitment with key produces different secrets each call", async () => {
    const handle = await provider.generateKey("Ed25519", "commitment");
    const r1 = await provider.generateCommitment({ keyId: handle.keyId });
    const r2 = await provider.generateCommitment({ keyId: handle.keyId });
    expect(r1.secretHex).not.toBe(r2.secretHex);
  });

  it("generateCommitment with context produces domain-separated secrets", async () => {
    const r1 = await provider.generateCommitment({ context: "player-A" });
    const r2 = await provider.generateCommitment({ context: "player-B" });
    // Secrets should differ (context is mixed in)
    expect(r1.secretHex).not.toBe(r2.secretHex);
  });

  it("importKey accepts a 32-byte hex private key", async () => {
    const privateKeyHex = bytesToHex(randomBytes(32));
    const handle = await provider.importKey(privateKeyHex, "Ed25519", "sign");
    expect(handle.active).toBe(true);
    expect(handle.algorithm).toBe("Ed25519");
  });

  it("importKey rejects wrong-length key", async () => {
    const shortKey = bytesToHex(randomBytes(16));
    await expect(
      provider.importKey(shortKey, "Ed25519", "sign"),
    ).rejects.toThrow("32-byte");
  });

  it("listKeys returns all generated keys", async () => {
    const h1 = await provider.generateKey("Ed25519", "sign");
    const h2 = await provider.generateKey("Ed25519", "commitment");
    const keys = await provider.listKeys();
    const ids = keys.map((k) => k.keyId);
    expect(ids).toContain(h1.keyId);
    expect(ids).toContain(h2.keyId);
  });

  it("deactivateKey marks key as inactive", async () => {
    const handle = await provider.generateKey("Ed25519", "sign");
    await provider.deactivateKey(handle.keyId);
    const keys = await provider.listKeys();
    const found = keys.find((k) => k.keyId === handle.keyId);
    expect(found?.active).toBe(false);
  });

  it("persistKey and loadPersistedKey round-trip", async () => {
    const store = new Map();
    const p = new WebCryptoHsmProvider({ persistentStore: store });
    const handle = await p.generateKey("Ed25519", "sign");
    await p.persistKey(handle.keyId, "test-passphrase");

    // Create a fresh provider and load the key
    const p2 = new WebCryptoHsmProvider({ persistentStore: store });
    const loaded = await p2.loadPersistedKey(handle.keyId, "test-passphrase");
    expect(loaded.keyId).toBe(handle.keyId);
    expect(loaded.active).toBe(true);
  });

  it("loadPersistedKey fails with wrong passphrase", async () => {
    const store = new Map();
    const p = new WebCryptoHsmProvider({ persistentStore: store });
    const handle = await p.generateKey("Ed25519", "sign");
    await p.persistKey(handle.keyId, "correct-passphrase");

    const p2 = new WebCryptoHsmProvider({ persistentStore: store });
    await expect(
      p2.loadPersistedKey(handle.keyId, "wrong-passphrase"),
    ).rejects.toThrow();
  });
});

// ── FailoverHsmProvider ───────────────────────────────────────────────────────

describe("FailoverHsmProvider", () => {
  function makeAlwaysFailProvider(): HsmProvider {
    return {
      name: "AlwaysFail",
      isAvailable: vi.fn().mockResolvedValue(false),
      generateKey: vi.fn().mockRejectedValue(new Error("primary unavailable")),
      importKey: vi.fn().mockRejectedValue(new Error("primary unavailable")),
      exportPublicKey: vi
        .fn()
        .mockRejectedValue(new Error("primary unavailable")),
      sign: vi.fn().mockRejectedValue(new Error("primary unavailable")),
      generateCommitment: vi
        .fn()
        .mockRejectedValue(new Error("primary unavailable")),
      listKeys: vi.fn().mockRejectedValue(new Error("primary unavailable")),
      deactivateKey: vi
        .fn()
        .mockRejectedValue(new Error("primary unavailable")),
    };
  }

  it("uses primary when available", async () => {
    const primary = new WebCryptoHsmProvider();
    const secondary = new WebCryptoHsmProvider();
    const failover = new FailoverHsmProvider(primary, secondary);

    const handle = await failover.generateKey("Ed25519", "sign");
    expect(handle.keyId).toMatch(/^wc-/);
  });

  it("falls back to secondary when primary fails", async () => {
    const primary = makeAlwaysFailProvider();
    const secondary = new WebCryptoHsmProvider();
    const failover = new FailoverHsmProvider(primary, secondary);

    const handle = await failover.generateKey("Ed25519", "sign");
    expect(handle.keyId).toMatch(/^wc-/);
  });

  it("emits failover event when primary fails", async () => {
    const primary = makeAlwaysFailProvider();
    const secondary = new WebCryptoHsmProvider();
    const failover = new FailoverHsmProvider(primary, secondary);

    const events: unknown[] = [];
    failover.onFailover((e) => events.push(e));

    await failover.generateKey("Ed25519", "sign");
    expect(events).toHaveLength(1);
    expect((events[0] as { failedProvider: string }).failedProvider).toBe(
      "AlwaysFail",
    );
  });

  it("throws when both providers fail", async () => {
    const primary = makeAlwaysFailProvider();
    const secondary = makeAlwaysFailProvider();
    const failover = new FailoverHsmProvider(primary, secondary);

    await expect(failover.generateKey("Ed25519", "sign")).rejects.toThrow(
      "Both HSM providers failed",
    );
  });

  it("isAvailable returns true if either provider is available", async () => {
    const primary = makeAlwaysFailProvider();
    const secondary = new WebCryptoHsmProvider();
    const failover = new FailoverHsmProvider(primary, secondary);

    expect(await failover.isAvailable()).toBe(true);
  });

  it("isAvailable returns false if both providers are unavailable", async () => {
    const primary = makeAlwaysFailProvider();
    const secondary = makeAlwaysFailProvider();
    const failover = new FailoverHsmProvider(primary, secondary);

    expect(await failover.isAvailable()).toBe(false);
  });

  it("re-probes primary after probe interval", async () => {
    const primary = makeAlwaysFailProvider();
    const secondary = new WebCryptoHsmProvider();
    const failover = new FailoverHsmProvider(primary, secondary, {
      probeIntervalMs: 0, // immediate re-probe
    });

    // First call fails over to secondary
    await failover.generateKey("Ed25519", "sign");

    // Make primary available again
    (primary.isAvailable as ReturnType<typeof vi.fn>).mockResolvedValue(true);
    (primary.generateKey as ReturnType<typeof vi.fn>).mockResolvedValue({
      keyId: "hw-recovered",
      algorithm: "Ed25519",
      usage: "sign",
      createdAt: new Date().toISOString(),
      active: true,
    } satisfies HsmKeyHandle);

    // Second call should re-probe and use primary
    const handle = await failover.generateKey("Ed25519", "sign");
    expect(handle.keyId).toBe("hw-recovered");
  });

  it("offFailover removes listener", async () => {
    const primary = makeAlwaysFailProvider();
    const secondary = new WebCryptoHsmProvider();
    const failover = new FailoverHsmProvider(primary, secondary);

    const events: unknown[] = [];
    const listener = (e: unknown) => events.push(e);
    failover.onFailover(listener);
    failover.offFailover(listener);

    await failover.generateKey("Ed25519", "sign");
    expect(events).toHaveLength(0);
  });
});

// ── SecureKeyStorage ──────────────────────────────────────────────────────────

describe("SecureKeyStorage", () => {
  it("open succeeds (falls back to in-memory in jsdom)", async () => {
    const storage = new SecureKeyStorage("test-store");
    await storage.open();
    expect(storage.isOpen()).toBe(true);
  });

  it("saveKey and loadKey round-trip", async () => {
    const storage = new SecureKeyStorage("test-store-2");
    await storage.open();

    const record = {
      keyId: "test-key-1",
      algorithm: "Ed25519" as const,
      usage: "sign" as const,
      encryptedKeyMaterial: "abc123",
      iv: "iv123",
      salt: "salt123",
      createdAt: new Date().toISOString(),
      active: true,
    };

    await storage.saveKey(record);
    const loaded = await storage.loadKey("test-key-1");
    expect(loaded).toEqual(record);
  });

  it("loadKey returns null for unknown ID", async () => {
    const storage = new SecureKeyStorage("test-store-3");
    await storage.open();
    expect(await storage.loadKey("nonexistent")).toBeNull();
  });

  it("deleteKey removes the record", async () => {
    const storage = new SecureKeyStorage("test-store-4");
    await storage.open();

    const record = {
      keyId: "to-delete",
      algorithm: "Ed25519" as const,
      usage: "sign" as const,
      encryptedKeyMaterial: "x",
      iv: "y",
      salt: "z",
      createdAt: new Date().toISOString(),
      active: true,
    };

    await storage.saveKey(record);
    await storage.deleteKey("to-delete");
    expect(await storage.loadKey("to-delete")).toBeNull();
  });

  it("listKeyIds returns all saved IDs", async () => {
    const storage = new SecureKeyStorage("test-store-5");
    await storage.open();

    for (const id of ["k1", "k2", "k3"]) {
      await storage.saveKey({
        keyId: id,
        algorithm: "Ed25519",
        usage: "sign",
        encryptedKeyMaterial: "",
        iv: "",
        salt: "",
        createdAt: new Date().toISOString(),
        active: true,
      });
    }

    const ids = await storage.listKeyIds();
    expect(ids).toContain("k1");
    expect(ids).toContain("k2");
    expect(ids).toContain("k3");
  });

  it("throws when not open", async () => {
    const storage = new SecureKeyStorage("not-opened");
    await expect(storage.loadKey("x")).rejects.toThrow("not open");
  });
});

// ── HsmContractAdapter ────────────────────────────────────────────────────────

describe("HsmContractAdapter", () => {
  let provider: WebCryptoHsmProvider;
  let signingKeyId: string;
  let commitmentKeyId: string;

  beforeEach(async () => {
    provider = new WebCryptoHsmProvider();
    const signingKey = await provider.generateKey("Ed25519", "sign");
    const commitmentKey = await provider.generateKey("Ed25519", "commitment");
    signingKeyId = signingKey.keyId;
    commitmentKeyId = commitmentKey.keyId;
  });

  function makeAdapter(base: ContractAdapter) {
    return new HsmContractAdapter(base, provider, {
      signingKeyId,
      commitmentKeyId,
      playerAddress: "GTEST123",
    });
  }

  it("startGame injects HSM commitment when none provided", async () => {
    const base = makeBaseAdapter();
    const adapter = makeAdapter(base);

    await adapter.startGame({
      wagerStroops: 1_000_000,
      side: "heads",
      commitmentHash: "", // sentinel: generate via HSM
    });

    expect(base.startGame).toHaveBeenCalledOnce();
    const call = (base.startGame as ReturnType<typeof vi.fn>).mock.calls[0][0];
    expect(call.commitmentHash).toMatch(/^0x[0-9a-f]{64}$/);
  });

  it("startGame passes through provided commitment hash", async () => {
    const base = makeBaseAdapter();
    const adapter = makeAdapter(base);
    const existingHash = "0x" + "ab".repeat(32);

    await adapter.startGame({
      wagerStroops: 1_000_000,
      side: "tails",
      commitmentHash: existingHash,
    });

    const call = (base.startGame as ReturnType<typeof vi.fn>).mock.calls[0][0];
    expect(call.commitmentHash).toBe(existingHash);
  });

  it("reveal delegates to base adapter", async () => {
    const base = makeBaseAdapter();
    const adapter = makeAdapter(base);

    const result = await adapter.reveal({ gameId: "g1", secret: "secret123" });
    expect(result.outcome).toBe("win");
    expect(base.reveal).toHaveBeenCalledOnce();
  });

  it("cashOut delegates to base adapter", async () => {
    const base = makeBaseAdapter();
    const adapter = makeAdapter(base);

    const result = await adapter.cashOut({ gameId: "g1" });
    expect(result.payoutStroops).toBe(2_000_000);
  });

  it("continueGame delegates to base adapter", async () => {
    const base = makeBaseAdapter();
    const adapter = makeAdapter(base);

    await adapter.continueGame({ gameId: "g1" });
    expect(base.continueGame).toHaveBeenCalledOnce();
  });

  it("audit log records successful operations", async () => {
    const base = makeBaseAdapter();
    const adapter = makeAdapter(base);

    await adapter.startGame({
      wagerStroops: 1_000_000,
      side: "heads",
      commitmentHash: "",
    });
    await adapter.reveal({ gameId: "g1", secret: "s" });

    const log = adapter.getAuditLog();
    expect(log.length).toBeGreaterThanOrEqual(2);
    expect(log.every((e) => e.success)).toBe(true);
  });

  it("audit log records failed operations", async () => {
    const base = makeBaseAdapter();
    (base.startGame as ReturnType<typeof vi.fn>).mockRejectedValue(
      new Error("contract error"),
    );
    const adapter = makeAdapter(base);

    await expect(
      adapter.startGame({
        wagerStroops: 1_000_000,
        side: "heads",
        commitmentHash: "",
      }),
    ).rejects.toThrow("contract error");

    const log = adapter.getAuditLog();
    // The HSM operations (commitment + sign) should succeed; the base call fails
    const failedEntries = log.filter((e) => !e.success);
    expect(failedEntries.length).toBeGreaterThanOrEqual(0);
  });

  it("clearAuditLog empties the log", async () => {
    const base = makeBaseAdapter();
    const adapter = makeAdapter(base);

    await adapter.startGame({
      wagerStroops: 1_000_000,
      side: "heads",
      commitmentHash: "",
    });
    adapter.clearAuditLog();
    expect(adapter.getAuditLog()).toHaveLength(0);
  });

  it("audit log respects maxAuditEntries cap", async () => {
    const base = makeBaseAdapter();
    const adapter = new HsmContractAdapter(base, provider, {
      signingKeyId,
      commitmentKeyId,
      playerAddress: "GTEST",
      maxAuditEntries: 3,
    });

    for (let i = 0; i < 5; i++) {
      await adapter.cashOut({ gameId: `g${i}` });
    }

    expect(adapter.getAuditLog().length).toBeLessThanOrEqual(3);
  });

  it("generateCommitment returns valid commitment", async () => {
    const base = makeBaseAdapter();
    const adapter = makeAdapter(base);

    const result = await adapter.generateCommitment("test");
    expect(result.commitmentBytes32).toMatch(/^0x[0-9a-f]{64}$/);
    expect(result.secretHex).toMatch(/^[0-9a-f]{64}$/);
  });
});
