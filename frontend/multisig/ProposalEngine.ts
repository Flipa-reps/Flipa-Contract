/**
 * Proposal Engine
 *
 * Core state machine for the multi-party authorization + timelock system.
 * Manages the full proposal lifecycle: create → approve/reject → queue →
 * timelock → execute / cancel / expire.
 *
 * ## Invariants
 * - A signer may only approve or reject once per proposal.
 * - Execution is blocked until both quorum is met AND the timelock has elapsed.
 * - Emergency proposals use a higher quorum threshold but a reduced timelock
 *   (50% of the normal delay, minimum 1 minute).
 * - Only the proposer or a SuperAdmin may cancel a pending/queued proposal.
 * - Proposals that reach their voting deadline without quorum transition to
 *   `expired` automatically on the next status check.
 *
 * ## Persistence
 * Proposals are serialised to localStorage (keyed by `storageKey`) so they
 * survive page refreshes. The in-memory map is the authoritative source.
 *
 * ## Audit integration
 * Every state transition emits a security event via the injected emitter.
 */

import {
  Proposal,
  ProposalAction,
  ProposalStatus,
  ApprovalRecord,
  RejectionRecord,
  MultisigConfig,
  TimelockStatus,
  ProposalExecutionResult,
  DEFAULT_MULTISIG_CONFIG,
  DEFAULT_TIMELOCK_MS,
  PERMISSION_RISK,
  ACTION_PERMISSION,
} from "./types";
import { randomBytes, bytesToHex } from "../hsm/crypto";
import type { SecurityEventEmitter } from "../security/SecurityEventEmitter";
import type { RoleRegistry } from "../rbac/RoleRegistry";

// ── Engine ────────────────────────────────────────────────────────────────────

export class ProposalEngine {
  private readonly proposals = new Map<string, Proposal>();
  private config: MultisigConfig;
  private readonly storageKey: string;
  private emitter: SecurityEventEmitter | null;
  private registry: RoleRegistry | null;

  constructor(
    options: {
      config?: Partial<MultisigConfig>;
      storageKey?: string;
      emitter?: SecurityEventEmitter | null;
      registry?: RoleRegistry | null;
    } = {},
  ) {
    this.config = { ...DEFAULT_MULTISIG_CONFIG, ...options.config };
    this.storageKey = options.storageKey ?? "flipa-multisig-proposals";
    this.emitter = options.emitter ?? null;
    this.registry = options.registry ?? null;
    this.loadFromStorage();
  }

  // ── Configuration ─────────────────────────────────────────────────────────

  getConfig(): Readonly<MultisigConfig> {
    return { ...this.config };
  }

  updateConfig(patch: Partial<MultisigConfig>): void {
    this.config = { ...this.config, ...patch };
  }

  addSigner(address: string): void {
    if (!this.config.signers.includes(address)) {
      this.config.signers = [...this.config.signers, address];
    }
  }

  removeSigner(address: string): void {
    this.config.signers = this.config.signers.filter((s) => s !== address);
  }

  // ── Proposal creation ─────────────────────────────────────────────────────

  /**
   * Create a new proposal.
   *
   * @param proposer    - Address creating the proposal (must be a signer or SuperAdmin)
   * @param action      - The operation to be executed
   * @param description - Human-readable description
   * @param emergency   - If true, uses higher quorum + reduced timelock
   */
  createProposal(
    proposer: string,
    action: ProposalAction,
    description: string,
    emergency = false,
  ): Proposal {
    this.assertIsSigner(proposer);

    const requiredPermission = ACTION_PERMISSION[action.type];
    const risk = PERMISSION_RISK[requiredPermission];
    const baseDelay =
      this.config.timelockOverrides?.[risk] ?? DEFAULT_TIMELOCK_MS[risk];
    const timelockMs = emergency
      ? Math.max(baseDelay * 0.5, 60_000) // 50% of normal, min 1 min
      : baseDelay;

    const quorumThreshold = emergency
      ? this.config.emergencyQuorumThreshold
      : this.config.quorumThreshold;

    const now = new Date();
    const votingDeadline = new Date(now.getTime() + this.config.votingWindowMs);

    const proposal: Proposal = {
      id: generateProposalId(),
      proposer,
      action,
      requiredPermission,
      risk,
      status: "pending",
      createdAt: now.toISOString(),
      votingDeadline: votingDeadline.toISOString(),
      queuedAt: null,
      executeAfter: null,
      executedAt: null,
      executedBy: null,
      timelockMs,
      quorumThreshold,
      approvals: [],
      rejections: [],
      description,
      emergency,
    };

    this.proposals.set(proposal.id, proposal);
    this.persist();

    this.emitter
      ?.emit("proposal.created", "system", "info", proposer, {
        proposalId: proposal.id,
        action: action.type,
        risk,
        timelockMs,
        emergency,
        description,
      })
      .catch(() => {});

    return proposal;
  }

  // ── Approval / rejection ──────────────────────────────────────────────────

  /**
   * Approve a proposal. Automatically transitions to `queued` if quorum is met.
   */
  approve(
    signerAddress: string,
    proposalId: string,
    comment?: string,
  ): Proposal {
    this.assertIsSigner(signerAddress);
    const raw = this.requireProposalRaw(proposalId);

    // Terminal states are never votable
    if (
      raw.status === "executed" ||
      raw.status === "cancelled" ||
      raw.status === "expired"
    ) {
      throw new MultisigError(
        `Proposal ${raw.id} is not open for voting (status: ${raw.status})`,
        raw.id,
        raw.status,
      );
    }

    // For duplicate-vote check, treat queued/ready as pending (votes were cast
    // while the proposal was pending; status was updated by a prior approve call)
    const viewAsPending: Proposal = { ...raw, status: "pending" };
    this.assertVotable(viewAsPending, signerAddress);

    const record: ApprovalRecord = {
      address: signerAddress,
      approvedAt: new Date().toISOString(),
      comment,
    };

    // Build the updated proposal with the new approval, then recompute status
    const withApproval: Proposal = {
      ...raw,
      approvals: [...raw.approvals, record],
    };

    const withStatus = this.recomputeStatus(withApproval);
    this.proposals.set(proposalId, withStatus);
    this.persist();

    this.emitter
      ?.emit("proposal.approved", "system", "info", signerAddress, {
        proposalId,
        approvalCount: withStatus.approvals.length,
        status: withStatus.status,
      })
      .catch(() => {});

    return withStatus;
  }

  /**
   * Reject a proposal. Does not cancel it — other signers may still approve.
   */
  reject(signerAddress: string, proposalId: string, reason?: string): Proposal {
    this.assertIsSigner(signerAddress);
    const raw = this.requireProposalRaw(proposalId);

    if (
      raw.status === "executed" ||
      raw.status === "cancelled" ||
      raw.status === "expired"
    ) {
      throw new MultisigError(
        `Proposal ${raw.id} is not open for voting (status: ${raw.status})`,
        raw.id,
        raw.status,
      );
    }

    const viewAsPending: Proposal = { ...raw, status: "pending" };
    this.assertVotable(viewAsPending, signerAddress);

    const record: RejectionRecord = {
      address: signerAddress,
      rejectedAt: new Date().toISOString(),
      reason,
    };

    const updated: Proposal = {
      ...raw,
      rejections: [...raw.rejections, record],
    };

    const withStatus = this.recomputeStatus(updated);
    this.proposals.set(proposalId, withStatus);
    this.persist();

    this.emitter
      ?.emit("proposal.rejected", "system", "warning", signerAddress, {
        proposalId,
        rejectionCount: withStatus.rejections.length,
        reason,
      })
      .catch(() => {});

    return withStatus;
  }

  // ── Execution ─────────────────────────────────────────────────────────────

  /**
   * Execute a proposal that is in `ready` status.
   * Returns the execution result; the caller is responsible for applying the
   * action to the contract.
   *
   * @throws if the proposal is not ready (wrong status or timelock not elapsed).
   */
  execute(
    executorAddress: string,
    proposalId: string,
  ): ProposalExecutionResult {
    this.assertIsSigner(executorAddress);
    const proposal = this.requireProposal(proposalId);

    // Refresh status in case timelock just elapsed
    const refreshed = this.recomputeStatus(proposal);
    this.proposals.set(proposalId, refreshed);

    if (refreshed.status !== "ready") {
      throw new MultisigError(
        `Proposal ${proposalId} is not ready for execution (status: ${refreshed.status})`,
        proposalId,
        refreshed.status,
      );
    }

    const now = new Date().toISOString();
    const executed: Proposal = {
      ...refreshed,
      status: "executed",
      executedAt: now,
      executedBy: executorAddress,
    };

    this.proposals.set(proposalId, executed);
    this.persist();

    const result: ProposalExecutionResult = {
      proposalId,
      executedAt: now,
      executedBy: executorAddress,
      action: executed.action,
    };

    this.emitter
      ?.emit("proposal.executed", "system", "info", executorAddress, {
        proposalId,
        action: executed.action.type,
        timelockMs: executed.timelockMs,
      })
      .catch(() => {});

    return result;
  }

  // ── Cancellation ──────────────────────────────────────────────────────────

  /**
   * Cancel a proposal. Only the proposer or a SuperAdmin may cancel.
   */
  cancel(callerAddress: string, proposalId: string, reason?: string): Proposal {
    const proposal = this.requireProposal(proposalId);

    const isSuperAdmin =
      this.registry?.hasAtLeastRole(callerAddress, "SuperAdmin") ?? false;
    const isProposer = proposal.proposer === callerAddress;

    if (!isSuperAdmin && !isProposer) {
      throw new MultisigError(
        `Only the proposer or a SuperAdmin may cancel proposal ${proposalId}`,
        proposalId,
        proposal.status,
      );
    }

    if (proposal.status === "executed" || proposal.status === "cancelled") {
      throw new MultisigError(
        `Cannot cancel a proposal in status: ${proposal.status}`,
        proposalId,
        proposal.status,
      );
    }

    const cancelled: Proposal = { ...proposal, status: "cancelled" };
    this.proposals.set(proposalId, cancelled);
    this.persist();

    this.emitter
      ?.emit("proposal.cancelled", "system", "warning", callerAddress, {
        proposalId,
        reason,
        previousStatus: proposal.status,
      })
      .catch(() => {});

    return cancelled;
  }

  // ── Emergency override ────────────────────────────────────────────────────

  /**
   * Emergency override: immediately queue a proposal, bypassing the normal
   * voting window. Requires the caller to be a SuperAdmin AND the proposal
   * to have met the emergency quorum threshold.
   *
   * This is a break-glass mechanism for critical security incidents.
   */
  emergencyOverride(callerAddress: string, proposalId: string): Proposal {
    const isSuperAdmin =
      this.registry?.hasAtLeastRole(callerAddress, "SuperAdmin") ?? false;
    if (!isSuperAdmin) {
      throw new MultisigError(
        `Emergency override requires SuperAdmin role`,
        proposalId,
        "pending",
      );
    }

    const proposal = this.requireProposal(proposalId);

    if (proposal.status !== "pending" && proposal.status !== "queued") {
      throw new MultisigError(
        `Emergency override only applies to pending or queued proposals`,
        proposalId,
        proposal.status,
      );
    }

    // Check emergency quorum
    const approvalFraction = this.approvalFraction(proposal);
    if (approvalFraction < this.config.emergencyQuorumThreshold) {
      throw new MultisigError(
        `Emergency override requires ${Math.round(this.config.emergencyQuorumThreshold * 100)}% approval ` +
          `(current: ${Math.round(approvalFraction * 100)}%)`,
        proposalId,
        proposal.status,
      );
    }

    const now = new Date();
    // Reduced timelock: 1 minute minimum
    const emergencyDelay = Math.max(proposal.timelockMs * 0.1, 60_000);
    const executeAfter = new Date(now.getTime() + emergencyDelay);

    const overridden: Proposal = {
      ...proposal,
      status: "queued",
      queuedAt: now.toISOString(),
      executeAfter: executeAfter.toISOString(),
      timelockMs: emergencyDelay,
      emergency: true,
    };

    this.proposals.set(proposalId, overridden);
    this.persist();

    this.emitter
      ?.emit(
        "proposal.emergency_override",
        "system",
        "critical",
        callerAddress,
        {
          proposalId,
          originalTimelockMs: proposal.timelockMs,
          reducedTimelockMs: emergencyDelay,
          approvalFraction,
        },
      )
      .catch(() => {});

    return overridden;
  }

  // ── Queries ───────────────────────────────────────────────────────────────

  getProposal(id: string): Proposal | null {
    const p = this.proposals.get(id);
    if (!p) return null;
    // Refresh status on read
    const refreshed = this.recomputeStatus(p);
    if (refreshed.status !== p.status) {
      this.proposals.set(id, refreshed);
      this.persist();
    }
    return refreshed;
  }

  listProposals(filter?: { status?: ProposalStatus }): Proposal[] {
    const all = Array.from(this.proposals.values())
      .map((p) => this.recomputeStatus(p))
      .sort((a, b) => {
        const timeDiff = b.createdAt.localeCompare(a.createdAt);
        return timeDiff !== 0 ? timeDiff : b.id.localeCompare(a.id);
      });

    if (filter?.status) {
      return all.filter((p) => p.status === filter.status);
    }
    return all;
  }

  getTimelockStatus(proposalId: string): TimelockStatus | null {
    const proposal = this.getProposal(proposalId);
    if (!proposal) return null;

    const now = Date.now();
    const executeAfterMs = proposal.executeAfter
      ? new Date(proposal.executeAfter).getTime()
      : null;

    const remainingMs = executeAfterMs
      ? Math.max(0, executeAfterMs - now)
      : proposal.timelockMs;

    return {
      proposalId,
      timelockMs: proposal.timelockMs,
      queuedAt: proposal.queuedAt,
      executeAfter: proposal.executeAfter,
      remainingMs,
      elapsed: executeAfterMs !== null && now >= executeAfterMs,
    };
  }

  // ── Status recomputation ──────────────────────────────────────────────────

  /**
   * Recompute the proposal status based on current time and approval counts.
   * Pure function — does not mutate the proposal map.
   */
  recomputeStatus(proposal: Proposal): Proposal {
    // Terminal states are immutable
    if (proposal.status === "executed" || proposal.status === "cancelled") {
      return proposal;
    }

    const now = Date.now();
    const votingDeadlineMs = new Date(proposal.votingDeadline).getTime();
    const fraction = this.approvalFraction(proposal);
    const quorumMet = fraction >= proposal.quorumThreshold;

    // Check expiry first
    if (!quorumMet && now > votingDeadlineMs && proposal.status === "pending") {
      return { ...proposal, status: "expired" };
    }

    // Transition pending → queued when quorum is met
    if (proposal.status === "pending" && quorumMet) {
      const queuedAt = new Date().toISOString();
      const executeAfter = new Date(now + proposal.timelockMs).toISOString();
      return {
        ...proposal,
        status: proposal.timelockMs === 0 ? "ready" : "queued",
        queuedAt,
        executeAfter,
      };
    }

    // Transition queued → ready when timelock elapses
    if (proposal.status === "queued" && proposal.executeAfter) {
      const executeAfterMs = new Date(proposal.executeAfter).getTime();
      if (now >= executeAfterMs) {
        return { ...proposal, status: "ready" };
      }
    }

    return proposal;
  }

  // ── Private helpers ───────────────────────────────────────────────────────

  private approvalFraction(proposal: Proposal): number {
    const total = this.config.signers.length;
    if (total === 0) return 0;
    return proposal.approvals.length / total;
  }

  private assertIsSigner(address: string): void {
    const isSigner = this.config.signers.includes(address);
    const isSuperAdmin =
      this.registry?.hasAtLeastRole(address, "SuperAdmin") ?? false;
    if (!isSigner && !isSuperAdmin) {
      throw new MultisigError(
        `${address} is not an authorised signer`,
        "",
        "pending",
      );
    }
  }

  private assertVotable(proposal: Proposal, signerAddress: string): void {
    // Use the proposal's own status field (passed in as the raw stored value).
    // We do NOT re-read from the map here to avoid seeing a status that was
    // written by a previous approve() call in the same sequence.
    if (proposal.status !== "pending") {
      throw new MultisigError(
        `Proposal ${proposal.id} is not open for voting (status: ${proposal.status})`,
        proposal.id,
        proposal.status,
      );
    }

    const alreadyApproved = proposal.approvals.some(
      (a) => a.address === signerAddress,
    );
    const alreadyRejected = proposal.rejections.some(
      (r) => r.address === signerAddress,
    );

    if (alreadyApproved || alreadyRejected) {
      throw new MultisigError(
        `${signerAddress} has already voted on proposal ${proposal.id}`,
        proposal.id,
        proposal.status,
      );
    }

    const now = Date.now();
    const deadline = new Date(proposal.votingDeadline).getTime();
    if (now > deadline) {
      throw new MultisigError(
        `Voting deadline for proposal ${proposal.id} has passed`,
        proposal.id,
        proposal.status,
      );
    }
  }

  private requireProposal(id: string): Proposal {
    const p = this.proposals.get(id);
    if (!p) {
      throw new MultisigError(`Proposal not found: ${id}`, id, "pending");
    }
    return p;
  }

  /** Return the raw stored proposal without recomputing status. */
  private requireProposalRaw(id: string): Proposal {
    const p = this.proposals.get(id);
    if (!p) {
      throw new MultisigError(`Proposal not found: ${id}`, id, "pending");
    }
    return p;
  }

  private persist(): void {
    try {
      if (typeof localStorage !== "undefined") {
        localStorage.setItem(
          this.storageKey,
          JSON.stringify(Array.from(this.proposals.entries())),
        );
      }
    } catch {
      // Non-fatal
    }
  }

  private loadFromStorage(): void {
    try {
      if (typeof localStorage !== "undefined") {
        const raw = localStorage.getItem(this.storageKey);
        if (!raw) return;
        const entries = JSON.parse(raw) as [string, Proposal][];
        for (const [id, proposal] of entries) {
          this.proposals.set(id, proposal);
        }
      }
    } catch {
      // Corrupt storage — start fresh
    }
  }
}

// ── Error ─────────────────────────────────────────────────────────────────────

export class MultisigError extends Error {
  constructor(
    message: string,
    public readonly proposalId: string,
    public readonly status: ProposalStatus,
  ) {
    super(message);
    this.name = "MultisigError";
  }
}

// ── ID generator ──────────────────────────────────────────────────────────────

function generateProposalId(): string {
  const bytes = randomBytes(16);
  bytes[6] = (bytes[6] & 0x0f) | 0x40;
  bytes[8] = (bytes[8] & 0x3f) | 0x80;
  const hex = bytesToHex(bytes);
  return [
    hex.slice(0, 8),
    hex.slice(8, 12),
    hex.slice(12, 16),
    hex.slice(16, 20),
    hex.slice(20, 32),
  ].join("-");
}
