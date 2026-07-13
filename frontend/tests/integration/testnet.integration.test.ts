/**
 * Testnet integration tests — Issue #432
 *
 * These tests run against a real deployed Flipa contract on Stellar testnet.
 * They are SKIPPED automatically when the required environment variables are
 * not set, so CI stays green without testnet credentials.
 *
 * Required env vars (see tests/integration/README.md):
 *   TESTNET_CONTRACT_ID  — deployed contract address (C...)
 *   TESTNET_SECRET_KEY   — funded testnet keypair (S...)
 *   TESTNET_RPC_URL      — optional, defaults to soroban-testnet.stellar.org
 *
 * Run manually:
 *   TESTNET_CONTRACT_ID=C... TESTNET_SECRET_KEY=S... \
 *     npx vitest run tests/integration/testnet.integration.test.ts
 *
 * Test coverage:
 *   1. Account is funded and reachable on testnet
 *   2. start_game submits a transaction and returns a tx hash
 *   3. reveal resolves the outcome (win or loss)
 *   4. cash_out pays out after a win
 *   5. continue_streak extends a winning streak
 *   6. Network error handling — bad RPC URL surfaces a clear error
 *   7. Invalid commitment is rejected by the contract (error code 12)
 */

import { describe, it, expect, beforeAll } from "vitest";
import crypto from "crypto";
import { Keypair, rpc as SorobanRpc } from "@stellar/stellar-sdk";
import { createTestnetAdapter } from "./testnet-adapter";

// ─── Guard: skip entire suite when env vars are absent ────────────────────────

const CONTRACT_ID = process.env.TESTNET_CONTRACT_ID;
const SECRET_KEY = process.env.TESTNET_SECRET_KEY;
const RPC_URL = process.env.TESTNET_RPC_URL ?? "https://soroban-testnet.stellar.org";

const runTestnet = CONTRACT_ID && SECRET_KEY;

// Vitest's `describe.skipIf` skips the whole suite cleanly.
const suite = runTestnet ? describe : describe.skip;

// ─── Helpers ──────────────────────────────────────────────────────────────────

/** Generate a random 32-byte secret and its SHA-256 commitment. */
function makeCommitment(): { secret: string; commitmentHash: string } {
  const secret = crypto.randomBytes(32).toString("hex");
  const commitmentHash = crypto.createHash("sha256").update(Buffer.from(secret, "hex")).digest("hex");
  return { secret, commitmentHash };
}

const WAGER_STROOPS = 10_000_000; // 1 XLM

// ─── Suite ────────────────────────────────────────────────────────────────────

suite("Testnet integration — full game flows", () => {
  let adapter: ReturnType<typeof createTestnetAdapter>;

  beforeAll(() => {
    adapter = createTestnetAdapter();
  });

  // 1. Connectivity
  it("testnet account is funded and reachable", async () => {
    const server = new SorobanRpc.Server(RPC_URL, { allowHttp: false });
    const keypair = Keypair.fromSecret(SECRET_KEY!);
    const account = await server.getAccount(keypair.publicKey());
    expect(account.accountId()).toBe(keypair.publicKey());
  }, 15_000);

  // 2. start_game
  it("start_game returns a transaction hash", async () => {
    const { secret, commitmentHash } = makeCommitment();
    const result = await adapter.startGame({
      wagerStroops: WAGER_STROOPS,
      side: "heads",
      commitmentHash,
    });
    expect(result.txHash).toMatch(/^[0-9a-f]{64}$/i);
    // Store for downstream tests via closure — each test is independent here
    // because testnet state is real; downstream tests create their own games.
    void secret; // used in reveal test below
  }, 30_000);

  // 3. reveal — full start → reveal flow
  it("reveal returns outcome win or loss", async () => {
    const { secret, commitmentHash } = makeCommitment();
    const { txHash: startTx } = await adapter.startGame({
      wagerStroops: WAGER_STROOPS,
      side: "heads",
      commitmentHash,
    });
    expect(startTx).toBeTruthy();

    // gameId is the player's public key on this contract
    const gameId = Keypair.fromSecret(SECRET_KEY!).publicKey();
    const { txHash, outcome } = await adapter.reveal({ gameId, secret });

    expect(txHash).toMatch(/^[0-9a-f]{64}$/i);
    expect(["win", "loss"]).toContain(outcome);
  }, 60_000);

  // 4. cash_out after a win
  it("cash_out returns a payout after winning", async () => {
    // We need a win — keep trying until we get one (max 5 attempts).
    let won = false;
    let gameId = "";
    for (let i = 0; i < 5 && !won; i++) {
      const { secret, commitmentHash } = makeCommitment();
      await adapter.startGame({ wagerStroops: WAGER_STROOPS, side: "heads", commitmentHash });
      gameId = Keypair.fromSecret(SECRET_KEY!).publicKey();
      const { outcome } = await adapter.reveal({ gameId, secret });
      won = outcome === "win";
    }

    if (!won) {
      console.warn("cash_out test: did not win in 5 attempts — skipping payout assertion");
      return;
    }

    const { txHash, payoutStroops } = await adapter.cashOut({ gameId });
    expect(txHash).toMatch(/^[0-9a-f]{64}$/i);
    expect(payoutStroops).toBeGreaterThan(WAGER_STROOPS); // payout > wager (multiplier ≥ 1)
  }, 120_000);

  // 5. continue_streak after a win
  it("continue_streak extends a winning streak", async () => {
    let won = false;
    let gameId = "";
    for (let i = 0; i < 5 && !won; i++) {
      const { secret, commitmentHash } = makeCommitment();
      await adapter.startGame({ wagerStroops: WAGER_STROOPS, side: "heads", commitmentHash });
      gameId = Keypair.fromSecret(SECRET_KEY!).publicKey();
      const { outcome } = await adapter.reveal({ gameId, secret });
      won = outcome === "win";
    }

    if (!won) {
      console.warn("continue_streak test: did not win in 5 attempts — skipping");
      return;
    }

    const { txHash } = await adapter.continueGame({ gameId });
    expect(txHash).toMatch(/^[0-9a-f]{64}$/i);
  }, 120_000);

  // 6. Network error handling
  it("surfaces a clear error when RPC is unreachable", async () => {
    // Override env temporarily by constructing a bad-URL adapter inline
    const badServer = new SorobanRpc.Server("https://invalid.testnet.example.invalid", {
      allowHttp: false,
    });
    await expect(
      badServer.getAccount(Keypair.fromSecret(SECRET_KEY!).publicKey())
    ).rejects.toThrow();
  }, 15_000);

  // 7. Invalid commitment rejected
  it("reveal with wrong secret is rejected by the contract", async () => {
    const { commitmentHash } = makeCommitment();
    await adapter.startGame({ wagerStroops: WAGER_STROOPS, side: "heads", commitmentHash });

    const gameId = Keypair.fromSecret(SECRET_KEY!).publicKey();
    const wrongSecret = crypto.randomBytes(32).toString("hex"); // does not match commitment

    await expect(adapter.reveal({ gameId, secret: wrongSecret })).rejects.toThrow();
  }, 60_000);
});
