/**
 * Role Registry
 *
 * In-memory store of role assignments with full CRUD, permission checking,
 * and audit-log integration. This is the single source of truth for who
 * holds which role in the current session.
 *
 * ## Persistence
 * Assignments are serialised to sessionStorage so they survive page refreshes
 * within the same browser session but are cleared on tab close. For production
 * use, assignments should be loaded from the on-chain contract state on mount.
 *
 * ## Audit integration
 * Every mutating operation (grant, revoke) optionally emits a security event
 * via an injected `SecurityEventEmitter`. Pass `null` to disable.
 */

import {
  Role,
  Permission,
  RoleAssignment,
  PermissionCheckResult,
  ALL_ROLES,
} from "./types";
import {
  roleHasPermission,
  roleAtLeast,
  getEffectivePermissions,
} from "./RoleHierarchy";
import type { SecurityEventEmitter } from "../security/SecurityEventEmitter";

// ── Registry ──────────────────────────────────────────────────────────────────

export class RoleRegistry {
  /** address → assignment */
  private readonly assignments = new Map<string, RoleAssignment>();
  private readonly storageKey: string;
  private emitter: SecurityEventEmitter | null;

  constructor(
    options: {
      storageKey?: string;
      emitter?: SecurityEventEmitter | null;
    } = {},
  ) {
    this.storageKey = options.storageKey ?? "flipa-rbac-assignments";
    this.emitter = options.emitter ?? null;
    this.loadFromStorage();
  }

  // ── Role assignment ───────────────────────────────────────────────────────

  /**
   * Grant `role` to `address`. If the address already has a role it is
   * replaced. Only a SuperAdmin may call this — the caller's role is checked
   * before the assignment is written.
   *
   * @throws if `callerAddress` does not hold SuperAdmin.
   */
  grantRole(
    callerAddress: string,
    targetAddress: string,
    role: Role,
    label?: string,
  ): RoleAssignment {
    this.requireRole(callerAddress, "SuperAdmin");

    const assignment: RoleAssignment = {
      address: targetAddress,
      role,
      grantedAt: new Date().toISOString(),
      grantedBy: callerAddress,
      label,
    };

    this.assignments.set(targetAddress, assignment);
    this.persistToStorage();

    this.emitter
      ?.emit("role.granted", "authorization", "info", callerAddress, {
        targetAddress,
        role,
        label,
      })
      .catch(() => {});

    return assignment;
  }

  /**
   * Revoke the role of `targetAddress`. Only a SuperAdmin may call this.
   * No-op if the address has no role.
   *
   * @throws if `callerAddress` does not hold SuperAdmin.
   */
  revokeRole(callerAddress: string, targetAddress: string): void {
    this.requireRole(callerAddress, "SuperAdmin");

    const existing = this.assignments.get(targetAddress);
    if (!existing) return;

    this.assignments.delete(targetAddress);
    this.persistToStorage();

    this.emitter
      ?.emit("role.revoked", "authorization", "info", callerAddress, {
        targetAddress,
        previousRole: existing.role,
      })
      .catch(() => {});
  }

  /**
   * Bootstrap: set the SuperAdmin without a caller check.
   * Should only be called once during initialisation with the on-chain admin
   * address. Subsequent calls are no-ops if the address already has SuperAdmin.
   */
  bootstrapSuperAdmin(address: string): void {
    if (this.assignments.get(address)?.role === "SuperAdmin") return;
    this.assignments.set(address, {
      address,
      role: "SuperAdmin",
      grantedAt: new Date().toISOString(),
      grantedBy: "system",
      label: "Contract admin (bootstrapped)",
    });
    this.persistToStorage();
  }

  // ── Role queries ──────────────────────────────────────────────────────────

  /** Return the role of `address`, or null if they have no role. */
  getRoleOf(address: string): Role | null {
    return this.assignments.get(address)?.role ?? null;
  }

  /** Return the full assignment record for `address`, or null. */
  getAssignment(address: string): RoleAssignment | null {
    return this.assignments.get(address) ?? null;
  }

  /** Return all current assignments. */
  listAssignments(): RoleAssignment[] {
    return Array.from(this.assignments.values());
  }

  /** Return all addresses holding `role` (exact match, not hierarchy). */
  getAddressesWithRole(role: Role): RoleAssignment[] {
    return this.listAssignments().filter((a) => a.role === role);
  }

  // ── Permission checks ─────────────────────────────────────────────────────

  /**
   * Check whether `address` has `permission`.
   * Returns a detailed result object for logging and UI feedback.
   */
  checkPermission(
    address: string,
    permission: Permission,
  ): PermissionCheckResult {
    const role = this.getRoleOf(address);

    if (!role) {
      return {
        granted: false,
        permission,
        address,
        reason: "No role assigned",
      };
    }

    const granted = roleHasPermission(role, permission);
    return {
      granted,
      grantingRole: granted ? role : undefined,
      permission,
      address,
      reason: granted
        ? undefined
        : `Role ${role} does not have permission ${permission}`,
    };
  }

  /**
   * Returns true if `address` has `permission`.
   * Convenience wrapper around `checkPermission`.
   */
  hasPermission(address: string, permission: Permission): boolean {
    return this.checkPermission(address, permission).granted;
  }

  /**
   * Assert that `address` has `permission`.
   * Emits an `access.denied` event and throws if not.
   */
  assertPermission(address: string, permission: Permission): void {
    const result = this.checkPermission(address, permission);
    if (!result.granted) {
      this.emitter
        ?.emitAccessDenied(
          address,
          permission.split(":")[0],
          permission.split(":")[1],
          result.reason ?? "Insufficient permissions",
        )
        .catch(() => {});
      throw new RbacError(
        `Permission denied: ${address} lacks ${permission}. ${result.reason ?? ""}`,
        address,
        permission,
        result.reason,
      );
    }

    this.emitter
      ?.emit("access.granted", "authorization", "info", address, {
        resource: permission.split(":")[0],
        action: permission.split(":")[1],
        walletAddress: address,
      })
      .catch(() => {});
  }

  /**
   * Returns all permissions held by `address` (empty array if no role).
   */
  getPermissionsOf(address: string): Permission[] {
    const role = this.getRoleOf(address);
    if (!role) return [];
    return getEffectivePermissions(role);
  }

  // ── Role hierarchy checks ─────────────────────────────────────────────────

  /**
   * Returns true if `address` holds at least `minimumRole`.
   */
  hasAtLeastRole(address: string, minimumRole: Role): boolean {
    const role = this.getRoleOf(address);
    if (!role) return false;
    return roleAtLeast(role, minimumRole);
  }

  // ── Emitter injection ─────────────────────────────────────────────────────

  setEmitter(emitter: SecurityEventEmitter | null): void {
    this.emitter = emitter;
  }

  // ── Private ───────────────────────────────────────────────────────────────

  private requireRole(address: string, minimumRole: Role): void {
    if (!this.hasAtLeastRole(address, minimumRole)) {
      throw new RbacError(
        `${address} requires at least ${minimumRole} to perform this operation`,
        address,
        undefined,
        `Requires ${minimumRole}`,
      );
    }
  }

  private persistToStorage(): void {
    try {
      if (typeof sessionStorage !== "undefined") {
        sessionStorage.setItem(
          this.storageKey,
          JSON.stringify(Array.from(this.assignments.entries())),
        );
      }
    } catch {
      // Storage quota or security errors are non-fatal
    }
  }

  private loadFromStorage(): void {
    try {
      if (typeof sessionStorage !== "undefined") {
        const raw = sessionStorage.getItem(this.storageKey);
        if (!raw) return;
        const entries = JSON.parse(raw) as [string, RoleAssignment][];
        for (const [addr, assignment] of entries) {
          this.assignments.set(addr, assignment);
        }
      }
    } catch {
      // Corrupt storage — start fresh
    }
  }
}

// ── Error type ────────────────────────────────────────────────────────────────

export class RbacError extends Error {
  constructor(
    message: string,
    public readonly address: string,
    public readonly permission: Permission | undefined,
    public readonly reason: string | undefined,
  ) {
    super(message);
    this.name = "RbacError";
  }
}
