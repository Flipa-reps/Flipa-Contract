/**
 * Integration tests for contract hooks: useStartGame, useReveal, useCashOut, useContinue.
 *
 * Mocking strategy:
 *   - ContractAdapter is a plain object with vi.fn() methods — no Soroban SDK import needed.
 *   - Each hook is exercised via a minimal React harness rendered with @testing-library/react.
 *   - Async state transitions are observed with waitFor().
 *
 * Patterns documented here:
 *   - createContract()  — factory for a fresh mock adapter per test
 *   - HookHarness       — renders all four hooks; exposes loading/error/result via data-testid
 *   - SingleHookHarness — renders one hook in isolation for focused state tests
 */
import React from "react";
import { render, screen, fireEvent, waitFor, act } from "@testing-library/react";
import { describe, expect, it, vi, beforeEach } from "vitest";
import { ContractAdapter } from "../hooks/contract";
import { useCashOut } from "../hooks/useCashOut";
import { useContinue } from "../hooks/useContinue";
import { useReveal } from "../hooks/useReveal";
import { useStartGame } from "../hooks/useStartGame";

function createContract(): ContractAdapter {
  return {
    startGame: vi.fn(),
    reveal: vi.fn(),
    cashOut: vi.fn(),
    continueGame: vi.fn(),
  };
}

function HookHarness({ contract }: { contract: ContractAdapter }) {
  const start = useStartGame(contract);
  const reveal = useReveal(contract);
  const cashOut = useCashOut(contract);
  const cont = useContinue(contract);

  return (
    <div>
      <button
        onClick={() => {
          void start
            .startGame({ wagerStroops: 10000000, side: "heads", commitmentHash: "0xabc" })
            .catch(() => {});
        }}
      >
        Start
      </button>
      <button
        onClick={() => {
          void reveal.reveal({ gameId: "g1", secret: "s1" }).catch(() => {});
        }}
      >
        Reveal
      </button>
      <button
        onClick={() => {
          void cashOut.cashOut({ gameId: "g1" }).catch(() => {});
        }}
      >
        CashOut
      </button>
      <button
        onClick={() => {
          void cont.continueGame({ gameId: "g1" }).catch(() => {});
        }}
      >
        Continue
      </button>

      <span data-testid="start-loading">{String(start.loading)}</span>
      <span data-testid="reveal-loading">{String(reveal.loading)}</span>
      <span data-testid="cash-loading">{String(cashOut.loading)}</span>
      <span data-testid="cont-loading">{String(cont.loading)}</span>

      <span data-testid="start-error">{start.error ?? ""}</span>
      <span data-testid="reveal-error">{reveal.error ?? ""}</span>
      <span data-testid="cash-error">{cashOut.error ?? ""}</span>
      <span data-testid="cont-error">{cont.error ?? ""}</span>
    </div>
  );
}

describe("contract hooks integration", () => {
  it("calls all contract endpoints with mock responses", async () => {
    const contract = createContract();
    (contract.startGame as any).mockResolvedValue({ txHash: "0xstart" });
    (contract.reveal as any).mockResolvedValue({ txHash: "0xreveal", outcome: "win" });
    (contract.cashOut as any).mockResolvedValue({ txHash: "0xcash", payoutStroops: 19000000 });
    (contract.continueGame as any).mockResolvedValue({ txHash: "0xcont" });

    render(<HookHarness contract={contract} />);

    fireEvent.click(screen.getByText("Start"));
    fireEvent.click(screen.getByText("Reveal"));
    fireEvent.click(screen.getByText("CashOut"));
    fireEvent.click(screen.getByText("Continue"));

    await waitFor(() => {
      expect(contract.startGame).toHaveBeenCalledTimes(1);
      expect(contract.reveal).toHaveBeenCalledTimes(1);
      expect(contract.cashOut).toHaveBeenCalledTimes(1);
      expect(contract.continueGame).toHaveBeenCalledTimes(1);
    });
  });

  it("exposes loading states while request is pending", async () => {
    const contract = createContract();
    (contract.startGame as any).mockImplementation(
      () => new Promise((resolve) => setTimeout(() => resolve({ txHash: "0xstart" }), 20))
    );
    (contract.reveal as any).mockResolvedValue({ txHash: "0xreveal", outcome: "win" });
    (contract.cashOut as any).mockResolvedValue({ txHash: "0xcash", payoutStroops: 10000000 });
    (contract.continueGame as any).mockResolvedValue({ txHash: "0xcont" });

    render(<HookHarness contract={contract} />);
    fireEvent.click(screen.getByText("Start"));

    expect(screen.getByTestId("start-loading").textContent).toBe("true");

    await waitFor(() => {
      expect(screen.getByTestId("start-loading").textContent).toBe("false");
    });
  });

  it("surfaces errors for failed contract calls", async () => {
    const contract = createContract();
    (contract.startGame as any).mockRejectedValue(new Error("start failed"));
    (contract.reveal as any).mockRejectedValue(new Error("reveal failed"));
    (contract.cashOut as any).mockRejectedValue(new Error("cash failed"));
    (contract.continueGame as any).mockRejectedValue(new Error("continue failed"));

    render(<HookHarness contract={contract} />);

    fireEvent.click(screen.getByText("Start"));
    fireEvent.click(screen.getByText("Reveal"));
    fireEvent.click(screen.getByText("CashOut"));
    fireEvent.click(screen.getByText("Continue"));

    await waitFor(() => {
      expect(screen.getByTestId("start-error").textContent).toContain("start failed");
      expect(screen.getByTestId("reveal-error").textContent).toContain("reveal failed");
      expect(screen.getByTestId("cash-error").textContent).toContain("cash failed");
      expect(screen.getByTestId("cont-error").textContent).toContain("continue failed");
    });
  });
});

// ─── Per-hook isolated harnesses ─────────────────────────────────────────────

function StartHarness({ contract }: { contract: ContractAdapter }) {
  const { startGame, loading, error } = useStartGame(contract);
  const [txHash, setTxHash] = React.useState("");
  return (
    <div>
      <button onClick={() => void startGame({ wagerStroops: 10_000_000, side: "heads", commitmentHash: "0xabc" }).then((r) => setTxHash(r.txHash)).catch(() => {})}>start</button>
      <button onClick={() => void startGame({ wagerStroops: 5_000_000, side: "tails", commitmentHash: "0xdef" }).catch(() => {})}>start-tails</button>
      <span data-testid="loading">{String(loading)}</span>
      <span data-testid="error">{error ?? ""}</span>
      <span data-testid="txhash">{txHash}</span>
    </div>
  );
}

function RevealHarness({ contract }: { contract: ContractAdapter }) {
  const { reveal, loading, error } = useReveal(contract);
  const [outcome, setOutcome] = React.useState("");
  return (
    <div>
      <button onClick={() => void reveal({ gameId: "g1", secret: "s1" }).then((r) => setOutcome(r.outcome)).catch(() => {})}>reveal</button>
      <span data-testid="loading">{String(loading)}</span>
      <span data-testid="error">{error ?? ""}</span>
      <span data-testid="outcome">{outcome}</span>
    </div>
  );
}

function CashOutHarness({ contract }: { contract: ContractAdapter }) {
  const { cashOut, loading, error } = useCashOut(contract);
  const [payout, setPayout] = React.useState(0);
  return (
    <div>
      <button onClick={() => void cashOut({ gameId: "g1" }).then((r) => setPayout(r.payoutStroops)).catch(() => {})}>cashout</button>
      <span data-testid="loading">{String(loading)}</span>
      <span data-testid="error">{error ?? ""}</span>
      <span data-testid="payout">{payout}</span>
    </div>
  );
}

function ContinueHarness({ contract }: { contract: ContractAdapter }) {
  const { continueGame, loading, error } = useContinue(contract);
  const [txHash, setTxHash] = React.useState("");
  return (
    <div>
      <button onClick={() => void continueGame({ gameId: "g1" }).then((r) => setTxHash(r.txHash)).catch(() => {})}>continue</button>
      <span data-testid="loading">{String(loading)}</span>
      <span data-testid="error">{error ?? ""}</span>
      <span data-testid="txhash">{txHash}</span>
    </div>
  );
}

// ─── useStartGame ─────────────────────────────────────────────────────────────

describe("useStartGame", () => {
  let contract: ContractAdapter;
  beforeEach(() => { contract = createContract(); });

  it("starts idle: loading=false, error=''", () => {
    render(<StartHarness contract={contract} />);
    expect(screen.getByTestId("loading").textContent).toBe("false");
    expect(screen.getByTestId("error").textContent).toBe("");
  });

  it("sets loading=true while pending, then false on success", async () => {
    let resolve!: (v: { txHash: string }) => void;
    (contract.startGame as any).mockReturnValue(new Promise((r) => { resolve = r; }));
    render(<StartHarness contract={contract} />);
    fireEvent.click(screen.getByText("start"));
    expect(screen.getByTestId("loading").textContent).toBe("true");
    await act(async () => { resolve({ txHash: "0xstart" }); });
    expect(screen.getByTestId("loading").textContent).toBe("false");
  });

  it("returns txHash on success", async () => {
    (contract.startGame as any).mockResolvedValue({ txHash: "0xabc123" });
    render(<StartHarness contract={contract} />);
    fireEvent.click(screen.getByText("start"));
    await waitFor(() => expect(screen.getByTestId("txhash").textContent).toBe("0xabc123"));
  });

  it("forwards wagerStroops, side, commitmentHash to contract", async () => {
    (contract.startGame as any).mockResolvedValue({ txHash: "x" });
    render(<StartHarness contract={contract} />);
    fireEvent.click(screen.getByText("start-tails"));
    await waitFor(() => expect(contract.startGame).toHaveBeenCalledWith({
      wagerStroops: 5_000_000, side: "tails", commitmentHash: "0xdef",
    }));
  });

  it("sets error message on failure", async () => {
    (contract.startGame as any).mockRejectedValue(new Error("insufficient balance"));
    render(<StartHarness contract={contract} />);
    fireEvent.click(screen.getByText("start"));
    await waitFor(() => expect(screen.getByTestId("error").textContent).toBe("insufficient balance"));
  });

  it("normalises non-Error rejections to fallback message", async () => {
    (contract.startGame as any).mockRejectedValue("oops");
    render(<StartHarness contract={contract} />);
    fireEvent.click(screen.getByText("start"));
    await waitFor(() => expect(screen.getByTestId("error").textContent).toBe("Failed to start game"));
  });

  it("clears previous error on retry", async () => {
    (contract.startGame as any).mockRejectedValueOnce(new Error("first error"));
    (contract.startGame as any).mockResolvedValue({ txHash: "ok" });
    render(<StartHarness contract={contract} />);
    fireEvent.click(screen.getByText("start"));
    await waitFor(() => expect(screen.getByTestId("error").textContent).toBe("first error"));
    fireEvent.click(screen.getByText("start"));
    await waitFor(() => expect(screen.getByTestId("error").textContent).toBe(""));
  });

  it("loading returns to false after error", async () => {
    (contract.startGame as any).mockRejectedValue(new Error("fail"));
    render(<StartHarness contract={contract} />);
    fireEvent.click(screen.getByText("start"));
    await waitFor(() => expect(screen.getByTestId("loading").textContent).toBe("false"));
  });

  it("simulates tx signing: contract called once per click", async () => {
    (contract.startGame as any).mockResolvedValue({ txHash: "x" });
    render(<StartHarness contract={contract} />);
    fireEvent.click(screen.getByText("start"));
    await waitFor(() => expect(contract.startGame).toHaveBeenCalledTimes(1));
  });
});

// ─── useReveal ────────────────────────────────────────────────────────────────

describe("useReveal", () => {
  let contract: ContractAdapter;
  beforeEach(() => { contract = createContract(); });

  it("starts idle", () => {
    render(<RevealHarness contract={contract} />);
    expect(screen.getByTestId("loading").textContent).toBe("false");
    expect(screen.getByTestId("error").textContent).toBe("");
  });

  it("sets loading=true while pending", async () => {
    let resolve!: (v: { txHash: string; outcome: "win" | "loss" }) => void;
    (contract.reveal as any).mockReturnValue(new Promise((r) => { resolve = r; }));
    render(<RevealHarness contract={contract} />);
    fireEvent.click(screen.getByText("reveal"));
    expect(screen.getByTestId("loading").textContent).toBe("true");
    await act(async () => { resolve({ txHash: "x", outcome: "win" }); });
    expect(screen.getByTestId("loading").textContent).toBe("false");
  });

  it("exposes win outcome on success", async () => {
    (contract.reveal as any).mockResolvedValue({ txHash: "0xrev", outcome: "win" });
    render(<RevealHarness contract={contract} />);
    fireEvent.click(screen.getByText("reveal"));
    await waitFor(() => expect(screen.getByTestId("outcome").textContent).toBe("win"));
  });

  it("exposes loss outcome on success", async () => {
    (contract.reveal as any).mockResolvedValue({ txHash: "0xrev", outcome: "loss" });
    render(<RevealHarness contract={contract} />);
    fireEvent.click(screen.getByText("reveal"));
    await waitFor(() => expect(screen.getByTestId("outcome").textContent).toBe("loss"));
  });

  it("forwards gameId and secret to contract", async () => {
    (contract.reveal as any).mockResolvedValue({ txHash: "x", outcome: "win" });
    render(<RevealHarness contract={contract} />);
    fireEvent.click(screen.getByText("reveal"));
    await waitFor(() => expect(contract.reveal).toHaveBeenCalledWith({ gameId: "g1", secret: "s1" }));
  });

  it("sets error on failure", async () => {
    (contract.reveal as any).mockRejectedValue(new Error("commitment mismatch"));
    render(<RevealHarness contract={contract} />);
    fireEvent.click(screen.getByText("reveal"));
    await waitFor(() => expect(screen.getByTestId("error").textContent).toBe("commitment mismatch"));
  });

  it("normalises non-Error rejections", async () => {
    (contract.reveal as any).mockRejectedValue(42);
    render(<RevealHarness contract={contract} />);
    fireEvent.click(screen.getByText("reveal"));
    await waitFor(() => expect(screen.getByTestId("error").textContent).toBe("Failed to reveal result"));
  });

  it("loading returns to false after error", async () => {
    (contract.reveal as any).mockRejectedValue(new Error("fail"));
    render(<RevealHarness contract={contract} />);
    fireEvent.click(screen.getByText("reveal"));
    await waitFor(() => expect(screen.getByTestId("loading").textContent).toBe("false"));
  });
});

// ─── useCashOut ───────────────────────────────────────────────────────────────

describe("useCashOut", () => {
  let contract: ContractAdapter;
  beforeEach(() => { contract = createContract(); });

  it("starts idle", () => {
    render(<CashOutHarness contract={contract} />);
    expect(screen.getByTestId("loading").textContent).toBe("false");
    expect(screen.getByTestId("error").textContent).toBe("");
  });

  it("sets loading=true while pending", async () => {
    let resolve!: (v: { txHash: string; payoutStroops: number }) => void;
    (contract.cashOut as any).mockReturnValue(new Promise((r) => { resolve = r; }));
    render(<CashOutHarness contract={contract} />);
    fireEvent.click(screen.getByText("cashout"));
    expect(screen.getByTestId("loading").textContent).toBe("true");
    await act(async () => { resolve({ txHash: "x", payoutStroops: 19_000_000 }); });
    expect(screen.getByTestId("loading").textContent).toBe("false");
  });

  it("exposes payoutStroops on success", async () => {
    (contract.cashOut as any).mockResolvedValue({ txHash: "0xcash", payoutStroops: 19_000_000 });
    render(<CashOutHarness contract={contract} />);
    fireEvent.click(screen.getByText("cashout"));
    await waitFor(() => expect(screen.getByTestId("payout").textContent).toBe("19000000"));
  });

  it("forwards gameId to contract", async () => {
    (contract.cashOut as any).mockResolvedValue({ txHash: "x", payoutStroops: 0 });
    render(<CashOutHarness contract={contract} />);
    fireEvent.click(screen.getByText("cashout"));
    await waitFor(() => expect(contract.cashOut).toHaveBeenCalledWith({ gameId: "g1" }));
  });

  it("sets error on failure", async () => {
    (contract.cashOut as any).mockRejectedValue(new Error("game already settled"));
    render(<CashOutHarness contract={contract} />);
    fireEvent.click(screen.getByText("cashout"));
    await waitFor(() => expect(screen.getByTestId("error").textContent).toBe("game already settled"));
  });

  it("normalises non-Error rejections", async () => {
    (contract.cashOut as any).mockRejectedValue(null);
    render(<CashOutHarness contract={contract} />);
    fireEvent.click(screen.getByText("cashout"));
    await waitFor(() => expect(screen.getByTestId("error").textContent).toBe("Failed to cash out"));
  });

  it("loading returns to false after error", async () => {
    (contract.cashOut as any).mockRejectedValue(new Error("fail"));
    render(<CashOutHarness contract={contract} />);
    fireEvent.click(screen.getByText("cashout"));
    await waitFor(() => expect(screen.getByTestId("loading").textContent).toBe("false"));
  });

  it("clears previous error on retry", async () => {
    (contract.cashOut as any).mockRejectedValueOnce(new Error("first"));
    (contract.cashOut as any).mockResolvedValue({ txHash: "ok", payoutStroops: 1 });
    render(<CashOutHarness contract={contract} />);
    fireEvent.click(screen.getByText("cashout"));
    await waitFor(() => expect(screen.getByTestId("error").textContent).toBe("first"));
    fireEvent.click(screen.getByText("cashout"));
    await waitFor(() => expect(screen.getByTestId("error").textContent).toBe(""));
  });
});

// ─── useContinue ──────────────────────────────────────────────────────────────

describe("useContinue", () => {
  let contract: ContractAdapter;
  beforeEach(() => { contract = createContract(); });

  it("starts idle", () => {
    render(<ContinueHarness contract={contract} />);
    expect(screen.getByTestId("loading").textContent).toBe("false");
    expect(screen.getByTestId("error").textContent).toBe("");
  });

  it("sets loading=true while pending", async () => {
    let resolve!: (v: { txHash: string }) => void;
    (contract.continueGame as any).mockReturnValue(new Promise((r) => { resolve = r; }));
    render(<ContinueHarness contract={contract} />);
    fireEvent.click(screen.getByText("continue"));
    expect(screen.getByTestId("loading").textContent).toBe("true");
    await act(async () => { resolve({ txHash: "x" }); });
    expect(screen.getByTestId("loading").textContent).toBe("false");
  });

  it("returns txHash on success", async () => {
    (contract.continueGame as any).mockResolvedValue({ txHash: "0xcont" });
    render(<ContinueHarness contract={contract} />);
    fireEvent.click(screen.getByText("continue"));
    await waitFor(() => expect(screen.getByTestId("txhash").textContent).toBe("0xcont"));
  });

  it("forwards gameId to contract", async () => {
    (contract.continueGame as any).mockResolvedValue({ txHash: "x" });
    render(<ContinueHarness contract={contract} />);
    fireEvent.click(screen.getByText("continue"));
    await waitFor(() => expect(contract.continueGame).toHaveBeenCalledWith({ gameId: "g1" }));
  });

  it("sets error on failure", async () => {
    (contract.continueGame as any).mockRejectedValue(new Error("streak expired"));
    render(<ContinueHarness contract={contract} />);
    fireEvent.click(screen.getByText("continue"));
    await waitFor(() => expect(screen.getByTestId("error").textContent).toBe("streak expired"));
  });

  it("normalises non-Error rejections", async () => {
    (contract.continueGame as any).mockRejectedValue(undefined);
    render(<ContinueHarness contract={contract} />);
    fireEvent.click(screen.getByText("continue"));
    await waitFor(() => expect(screen.getByTestId("error").textContent).toBe("Failed to continue game"));
  });

  it("loading returns to false after error", async () => {
    (contract.continueGame as any).mockRejectedValue(new Error("fail"));
    render(<ContinueHarness contract={contract} />);
    fireEvent.click(screen.getByText("continue"));
    await waitFor(() => expect(screen.getByTestId("loading").textContent).toBe("false"));
  });
});

// ─── Tx signing simulation ────────────────────────────────────────────────────

describe("transaction signing simulation", () => {
  it("contract is called exactly once per user action (no double-submit)", async () => {
    const contract = createContract();
    (contract.startGame as any).mockResolvedValue({ txHash: "x" });
    render(<StartHarness contract={contract} />);
    fireEvent.click(screen.getByText("start"));
    await waitFor(() => expect(contract.startGame).toHaveBeenCalledTimes(1));
    expect(contract.startGame).toHaveBeenCalledTimes(1);
  });

  it("sequential calls each trigger a separate contract invocation", async () => {
    const contract = createContract();
    (contract.startGame as any).mockResolvedValue({ txHash: "x" });
    render(<StartHarness contract={contract} />);
    fireEvent.click(screen.getByText("start"));
    await waitFor(() => expect(contract.startGame).toHaveBeenCalledTimes(1));
    fireEvent.click(screen.getByText("start"));
    await waitFor(() => expect(contract.startGame).toHaveBeenCalledTimes(2));
  });

  it("slow tx: loading stays true until contract resolves", async () => {
    const contract = createContract();
    let resolve!: (v: { txHash: string }) => void;
    (contract.continueGame as any).mockReturnValue(new Promise((r) => { resolve = r; }));
    render(<ContinueHarness contract={contract} />);
    fireEvent.click(screen.getByText("continue"));
    expect(screen.getByTestId("loading").textContent).toBe("true");
    // Still loading after 10ms
    await new Promise((r) => setTimeout(r, 10));
    expect(screen.getByTestId("loading").textContent).toBe("true");
    await act(async () => { resolve({ txHash: "done" }); });
    expect(screen.getByTestId("loading").textContent).toBe("false");
  });

  it("user-rejected signing surfaces as error", async () => {
    const contract = createContract();
    (contract.startGame as any).mockRejectedValue(new Error("User rejected signing"));
    render(<StartHarness contract={contract} />);
    fireEvent.click(screen.getByText("start"));
    await waitFor(() => expect(screen.getByTestId("error").textContent).toBe("User rejected signing"));
    expect(screen.getByTestId("loading").textContent).toBe("false");
  });

  it("network timeout surfaces as error", async () => {
    const contract = createContract();
    (contract.reveal as any).mockRejectedValue(new Error("Request timed out"));
    render(<RevealHarness contract={contract} />);
    fireEvent.click(screen.getByText("reveal"));
    await waitFor(() => expect(screen.getByTestId("error").textContent).toBe("Request timed out"));
  });
});
