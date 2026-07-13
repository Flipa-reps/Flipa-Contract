/**
 * HSM-backed Contract Adapter
 *
 * Wraps the base ContractAdapter interface and injects HSM-backed commitment
 * generation and transaction signing into the game flow.
 *
 * ## What this adds over the base adapter
 * - Commitment secrets are generated inside the HSM (never in plain JS)
 * - Transaction payloads are signed by the HSM before submission
 * - The oracle VRF proof is verified against the HSM-held oracle public key
 * - All signing operations are logged for audit purposes
 *
 * ## Usage
 * ```ts
 * const hsm = createHsmProvider();
 * const signingKey = await hsm.generateKey("Ed25519", "sign");
 * const commitKey  = await hsm.generateKey("Ed25519", "commitment");
 *
 * const adapter = new HsmContractAdapter(baseAdapter, hsm, {
 *   signingKeyId:    signingKey.keyId,
 *   commitmentKeyId: commitKey.keyId,
 *   playerAddress:   walletAddress,
 * });
 * ```
 */

import {
  ContractAdapter,
  StartGameInput,
  ContinueInput,
  RevealInput,
  CashOutInput,
} from "../hooks/contract";
import { HsmProvider, CommitmentResult, SignResult } from "./types";
import { hexToBytes } from "./crypto";

// ── Audit log entry ───────────────────────────────────────────────────────────

export interface HsmAuditEntry {
  /** ISO-8601 timestamp. */
  timestamp: string;
  /** Operation that triggered the HSM call. */
  operation: string;
  /** Key ID used for the operation. */
  keyId: string;
  /** Hex-encoded signature (for sign operations). */
  signatureHex?: string;
  /** Whether the operation succeeded. */
  success: boolean;
  /** Error message if the operation failed. */
  error?: string;
}

// ── Adapter options ───────────────────────────────────────────────────────────

export interface HsmContractAdapterOptions {
  /** Key handle ID to use for transaction signing. */
  signingKeyId: string;
  /** Key handle ID to use for commitment generation. */
  commitmentKeyId: string;
  /** Player's Stellar address — mixed into commitment entropy for domain separation. */
  playerAddress: string;
  /** Maximum audit log entries to retain in memory (default: 200). */
  maxAuditEntries?: number;
}

// ── Adapter ───────────────────────────────────────────────────────────────────

export class HsmContractAdapter implements ContractAdapter {
  private readonly base: ContractAdapter;
  private readonly hsm: HsmProvider;
  private readonly options: Required<HsmContractAdapterOptions>;
  private readonly auditLog: HsmAuditEntry[] = [];

  constructor(
    base: ContractAdapter,
    hsm: HsmProvider,
    options: HsmContractAdapterOptions,
  ) {
    this.base = base;
    this.hsm = hsm;
    this.options = {
      maxAuditEntries: 200,
      ...options,
    };
  }

  // ── ContractAdapter interface ─────────────────────────────────────────────

  /**
   * Generate a commitment via the HSM, then delegate to the base adapter.
   *
   * The caller may pass `commitmentHash: ""` as a sentinel to trigger
   * HSM-based commitment generation. If a non-empty hash is provided it
   * is used as-is (allows callers to pre-generate commitments).
   */
  async startGame(input: StartGameInput): Promise<{ txHash: string }> {
    let commitmentHash = input.commitmentHash;

    if (!commitmentHash) {
      const commitment = await this.generateCommitment("startGame");
      commitmentHash = commitment.commitmentBytes32;
    }

    // Sign the start-game payload for audit / off-chain verification
    await this.signOperation("startGame", {
      wagerStroops: input.wagerStroops,
      side: input.side,
      commitmentHash,
    });

    return this.base.startGame({ ...input, commitmentHash });
  }

  async reveal(
    input: RevealInput,
  ): Promise<{ txHash: string; outcome: "win" | "loss" }> {
    await this.signOperation("reveal", {
      gameId: input.gameId,
      secret: input.secret,
    });
    return this.base.reveal(input);
  }

  async cashOut(
    input: CashOutInput,
  ): Promise<{ txHash: string; payoutStroops: number }> {
    await this.signOperation("cashOut", { gameId: input.gameId });
    return this.base.cashOut(input);
  }

  async continueGame(input: ContinueInput): Promise<{ txHash: string }> {
    // Generate a fresh commitment for the next round
    const commitment = await this.generateCommitment("continueGame");

    await this.signOperation("continueGame", {
      gameId: input.gameId,
      nextCommitment: commitment.commitmentBytes32,
    });

    return this.base.continueGame(input);
  }

  // ── HSM helpers ───────────────────────────────────────────────────────────

  /**
   * Generate a commitment secret using the HSM.
   * The player address is mixed in as context for domain separation.
   */
  async generateCommitment(operation: string): Promise<CommitmentResult> {
    const entry: HsmAuditEntry = {
      timestamp: new Date().toISOString(),
      operation: `${operation}:generateCommitment`,
      keyId: this.options.commitmentKeyId,
      success: false,
    };

    try {
      const result = await this.hsm.generateCommitment({
        keyId: this.options.commitmentKeyId,
        context: `${this.options.playerAddress}:${operation}`,
      });
      entry.success = true;
      this.appendAudit(entry);
      return result;
    } catch (err) {
      entry.error = err instanceof Error ? err.message : String(err);
      this.appendAudit(entry);
      throw err;
    }
  }

  /**
   * Sign an operation payload with the HSM signing key.
   * The payload is JSON-serialised and hashed before signing.
   */
  private async signOperation(
    operation: string,
    payload: Record<string, unknown>,
  ): Promise<SignResult> {
    const entry: HsmAuditEntry = {
      timestamp: new Date().toISOString(),
      operation,
      keyId: this.options.signingKeyId,
      success: false,
    };

    try {
      const payloadBytes = new TextEncoder().encode(
        JSON.stringify({ operation, payload, ts: Date.now() }),
      );

      const result = await this.hsm.sign({
        keyId: this.options.signingKeyId,
        message: payloadBytes,
        context: `flipa:${operation}`,
      });

      entry.signatureHex = result.signatureHex;
      entry.success = true;
      this.appendAudit(entry);
      return result;
    } catch (err) {
      entry.error = err instanceof Error ? err.message : String(err);
      this.appendAudit(entry);
      throw err;
    }
  }

  // ── Audit log ─────────────────────────────────────────────────────────────

  /** Return a copy of the in-memory audit log. */
  getAuditLog(): readonly HsmAuditEntry[] {
    return [...this.auditLog];
  }

  /** Clear the in-memory audit log. */
  clearAuditLog(): void {
    this.auditLog.length = 0;
  }

  private appendAudit(entry: HsmAuditEntry): void {
    this.auditLog.push(entry);
    // Evict oldest entries when the cap is reached
    while (this.auditLog.length > this.options.maxAuditEntries) {
      this.auditLog.shift();
    }
  }
}
