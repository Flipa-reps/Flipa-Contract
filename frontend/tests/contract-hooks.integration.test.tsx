import React from "react";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
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
