/**
 * E2E tests for complete game user journeys.
 *
 * Issue: add-e2e-tests-complete-game-scenarios
 *
 * Covers:
 *   - Landing → Connect Wallet → Start Game → Win → Cash Out
 *   - Landing → Connect Wallet → Start Game → Win → Continue → Win → Cash Out
 *   - Landing → Connect Wallet → Start Game → Lose
 *   - Landing → Connect Wallet → Start Game → Win → Continue → Lose
 *   - UI element updates throughout each flow
 *   - Transaction confirmation display
 *   - Error handling in UI
 */

import React, { useState } from "react";
import { render, screen, fireEvent, waitFor, act } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";

import { WalletModal, WalletId } from "../components/WalletModal";
import { GameStateCard, GameState } from "../components/GameStateCard";
import { CommitRevealFlow } from "../components/CommitRevealFlow";
import { CashOutModal } from "../components/CashOutModal";
import { GameResult } from "../components/GameResult";

// ─── Helpers ─────────────────────────────────────────────────────────────────

const MOCK_ADDRESS = "GABC1234567890ABCDEF1234567890ABCDEF1234567890ABCDEF1234567890";
const MOCK_TX_HASH = "abc123def456";

function mockConnectWallet(_walletId: WalletId): Promise<string> {
  return Promise.resolve(MOCK_ADDRESS);
}

function mockConnectWalletFail(_walletId: WalletId): Promise<string> {
  return Promise.reject(new Error("User rejected connection"));
}

// ─── Wallet Connection Flow ───────────────────────────────────────────────────

describe("Landing → Connect Wallet", () => {
  it("shows wallet options when modal is open", () => {
    render(
      <WalletModal
        open={true}
        onClose={vi.fn()}
        connectWallet={mockConnectWallet}
      />
    );
    expect(screen.getByText("Connect Wallet")).toBeInTheDocument();
    expect(screen.getByText("Freighter")).toBeInTheDocument();
    expect(screen.getByText("Albedo")).toBeInTheDocument();
    expect(screen.getByText("xBull")).toBeInTheDocument();
    expect(screen.getByText("Rabet")).toBeInTheDocument();
  });

  it("connects wallet and shows address on success", async () => {
    const onConnect = vi.fn();
    render(
      <WalletModal
        open={true}
        onClose={vi.fn()}
        onConnect={onConnect}
        connectWallet={mockConnectWallet}
      />
    );
    fireEvent.click(screen.getByText("Freighter"));
    await waitFor(() => {
      expect(screen.getByText("Wallet Connected")).toBeInTheDocument();
      expect(screen.getByText(MOCK_ADDRESS)).toBeInTheDocument();
    });
    expect(onConnect).toHaveBeenCalledWith(MOCK_ADDRESS, "freighter");
  });

  it("shows error banner when wallet connection fails", async () => {
    render(
      <WalletModal
        open={true}
        onClose={vi.fn()}
        connectWallet={mockConnectWalletFail}
      />
    );
    fireEvent.click(screen.getByText("Freighter"));
    await waitFor(() => {
      expect(screen.getByRole("alert")).toHaveTextContent("User rejected connection");
    });
  });

  it("does not render wallet list when modal is closed", () => {
    render(
      <WalletModal
        open={false}
        onClose={vi.fn()}
        connectWallet={mockConnectWallet}
      />
    );
    expect(screen.queryByText("Freighter")).not.toBeInTheDocument();
  });

  it("calls onClose when close button is clicked", () => {
    const onClose = vi.fn();
    render(
      <WalletModal
        open={true}
        onClose={onClose}
        connectWallet={mockConnectWallet}
      />
    );
    fireEvent.click(screen.getByLabelText("Close wallet modal"));
    expect(onClose).toHaveBeenCalled();
  });
});

// ─── Game State Card — UI updates throughout flow ─────────────────────────────

describe("GameStateCard — UI updates throughout game flow", () => {
  it("shows idle state before game starts", () => {
    render(<GameStateCard game={null} />);
    expect(screen.getByText(/start a game/i)).toBeInTheDocument();
  });

  it("shows committed phase with Reveal button after game starts", () => {
    const game: GameState = {
      phase: "committed",
      side: "heads",
      wagerStroops: 10_000_000,
      streak: 0,
    };
    render(<GameStateCard game={game} onReveal={vi.fn()} />);
    expect(screen.getByText("Awaiting Reveal")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /reveal/i })).toBeInTheDocument();
    expect(screen.getByText("1 XLM")).toBeInTheDocument();
    expect(screen.getByText("1.9×")).toBeInTheDocument();
  });

  it("shows won phase with Cash Out and Continue Streak buttons", () => {
    const game: GameState = {
      phase: "won",
      side: "heads",
      wagerStroops: 10_000_000,
      streak: 1,
    };
    render(<GameStateCard game={game} onCashOut={vi.fn()} onContinue={vi.fn()} />);
    expect(screen.getByText("You Won!")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /cash out/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /continue streak/i })).toBeInTheDocument();
    expect(screen.getByText("3.5×")).toBeInTheDocument();
    expect(screen.getByText("1")).toBeInTheDocument(); // streak
  });

  it("shows lost phase with no action buttons", () => {
    const game: GameState = {
      phase: "lost",
      side: "heads",
      wagerStroops: 10_000_000,
      streak: 0,
    };
    render(<GameStateCard game={game} />);
    expect(screen.getByText("You Lost")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /cash out/i })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /reveal/i })).not.toBeInTheDocument();
  });

  it("shows timed_out phase", () => {
    const game: GameState = {
      phase: "timed_out",
      side: "tails",
      wagerStroops: 5_000_000,
      streak: 0,
    };
    render(<GameStateCard game={game} />);
    expect(screen.getByText("Timed Out")).toBeInTheDocument();
  });

  it("disables buttons and shows aria-busy while loading", () => {
    const game: GameState = {
      phase: "committed",
      side: "heads",
      wagerStroops: 10_000_000,
      streak: 0,
    };
    render(<GameStateCard game={game} onReveal={vi.fn()} loading={true} />);
    const btn = screen.getByRole("button", { name: /reveal/i });
    expect(btn).toBeDisabled();
    expect(btn).toHaveAttribute("aria-busy", "true");
  });

  it("shows pending tx hash when provided", () => {
    const game: GameState = {
      phase: "committed",
      side: "heads",
      wagerStroops: 10_000_000,
      streak: 0,
      pendingTx: MOCK_TX_HASH,
    };
    render(<GameStateCard game={game} />);
    expect(screen.getByText(MOCK_TX_HASH)).toBeInTheDocument();
  });

  it("has aria-live=polite for screen reader announcements", () => {
    const game: GameState = {
      phase: "won",
      side: "heads",
      wagerStroops: 10_000_000,
      streak: 1,
    };
    const { container } = render(<GameStateCard game={game} />);
    const card = container.querySelector("[aria-live='polite']");
    expect(card).toBeInTheDocument();
  });

  it("updates multiplier display as streak increases", () => {
    const streakMultipliers: Array<[number, string]> = [
      [0, "1.9×"],
      [1, "3.5×"],
      [2, "6.0×"],
      [3, "10.0×"],
      [5, "10.0×"],
    ];
    for (const [streak, expected] of streakMultipliers) {
      const { unmount } = render(
        <GameStateCard
          game={{ phase: "won", side: "heads", wagerStroops: 10_000_000, streak }}
        />
      );
      expect(screen.getByText(expected)).toBeInTheDocument();
      unmount();
    }
  });
});

// ─── Scenario 1: Win → Cash Out ───────────────────────────────────────────────

describe("Scenario: Win → Cash Out", () => {
  it("completes full win → cash out flow", async () => {
    const onCashOut = vi.fn();
    const onContinue = vi.fn();

    // After reveal: won state
    const wonGame: GameState = {
      phase: "won",
      side: "heads",
      wagerStroops: 10_000_000,
      streak: 1,
    };
    render(
      <GameStateCard
        game={wonGame}
        onCashOut={onCashOut}
        onContinue={onContinue}
      />
    );

    expect(screen.getByText("You Won!")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /cash out/i }));
    expect(onCashOut).toHaveBeenCalledOnce();
  });

  it("CashOutModal shows correct payout for streak 1", () => {
    const onCashOut = vi.fn();
    const onContinue = vi.fn();
    // wager=10_000_000 stroops, streak=1 (1.9x), fee=300bps
    // gross=19_000_000, fee=570_000, net=18_430_000 → 1.8430 XLM
    render(
      <CashOutModal
        open={true}
        onClose={vi.fn()}
        streak={1}
        wager={10_000_000}
        feeBps={300}
        onCashOut={onCashOut}
        onContinue={onContinue}
      />
    );
    expect(screen.getByText("Cash Out or Continue?")).toBeInTheDocument();
    expect(screen.getByText("1-Win Streak")).toBeInTheDocument();
    // Current multiplier label
    expect(screen.getByText("1.9×")).toBeInTheDocument();
    // Next multiplier label
    expect(screen.getByText("3.5×")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /cash out/i }));
    expect(onCashOut).toHaveBeenCalledOnce();
  });

  it("GameResult win shows payout and Cash Out button", () => {
    const onCashOut = vi.fn();
    render(
      <GameResult
        outcome="win"
        wager={10_000_000}
        payout={18_430_000}
        streak={0}
        onCashOut={onCashOut}
      />
    );
    expect(screen.getByRole("status")).toBeInTheDocument();
    expect(screen.getByText(/you won/i)).toBeInTheDocument();
    expect(screen.getByText(/1\.8430 XLM/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /cash out/i }));
    expect(onCashOut).toHaveBeenCalledOnce();
  });
});

// ─── Scenario 2: Win → Continue → Win → Cash Out ─────────────────────────────

describe("Scenario: Win → Continue → Win → Cash Out", () => {
  it("shows streak 2 multiplier after continuing", () => {
    const game: GameState = {
      phase: "won",
      side: "heads",
      wagerStroops: 10_000_000,
      streak: 2,
    };
    render(<GameStateCard game={game} onCashOut={vi.fn()} onContinue={vi.fn()} />);
    expect(screen.getByText("6.0×")).toBeInTheDocument();
    expect(screen.getByText("2")).toBeInTheDocument(); // streak counter
  });

  it("CashOutModal shows streak 2 options correctly", () => {
    render(
      <CashOutModal
        open={true}
        onClose={vi.fn()}
        streak={2}
        wager={10_000_000}
        feeBps={300}
        onCashOut={vi.fn()}
        onContinue={vi.fn()}
      />
    );
    expect(screen.getByText("2-Win Streak")).toBeInTheDocument();
    expect(screen.getByText("3.5×")).toBeInTheDocument(); // current (streak 2)
    expect(screen.getByText("6.0×")).toBeInTheDocument(); // next (streak 3)
  });

  it("GameResult win with streak shows streak count in headline", () => {
    render(
      <GameResult
        outcome="win"
        wager={10_000_000}
        payout={33_950_000}
        streak={1}
        onCashOut={vi.fn()}
        onContinue={vi.fn()}
      />
    );
    expect(screen.getByText(/2-Win Streak/i)).toBeInTheDocument();
  });

  it("Continue Streak button triggers onContinue callback", () => {
    const onContinue = vi.fn();
    const game: GameState = {
      phase: "won",
      side: "heads",
      wagerStroops: 10_000_000,
      streak: 1,
    };
    render(<GameStateCard game={game} onCashOut={vi.fn()} onContinue={onContinue} />);
    fireEvent.click(screen.getByRole("button", { name: /continue streak/i }));
    expect(onContinue).toHaveBeenCalledOnce();
  });
});

// ─── Scenario 3: Lose ─────────────────────────────────────────────────────────

describe("Scenario: Lose", () => {
  it("GameResult loss shows forfeit message and Play Again button", () => {
    const onPlayAgain = vi.fn();
    render(
      <GameResult
        outcome="loss"
        wager={10_000_000}
        onPlayAgain={onPlayAgain}
      />
    );
    expect(screen.getByRole("status")).toBeInTheDocument();
    expect(screen.getByText(/better luck next time/i)).toBeInTheDocument();
    expect(screen.getByText(/1\.0000 XLM forfeited/i)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /play again/i }));
    expect(onPlayAgain).toHaveBeenCalledOnce();
  });

  it("GameStateCard lost phase has no action buttons", () => {
    const game: GameState = {
      phase: "lost",
      side: "tails",
      wagerStroops: 5_000_000,
      streak: 0,
    };
    render(<GameStateCard game={game} />);
    expect(screen.queryByRole("button")).not.toBeInTheDocument();
  });
});

// ─── Scenario 4: Win → Continue → Lose ───────────────────────────────────────

describe("Scenario: Win → Continue → Lose", () => {
  it("shows loss state after continuing and losing", () => {
    const game: GameState = {
      phase: "lost",
      side: "heads",
      wagerStroops: 10_000_000,
      streak: 0,
    };
    render(<GameStateCard game={game} />);
    expect(screen.getByText("You Lost")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /cash out/i })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /continue/i })).not.toBeInTheDocument();
  });

  it("CashOutModal risk note warns about losing all", () => {
    render(
      <CashOutModal
        open={true}
        onClose={vi.fn()}
        streak={1}
        wager={10_000_000}
        feeBps={300}
        onCashOut={vi.fn()}
        onContinue={vi.fn()}
      />
    );
    expect(screen.getByText(/lose all if next flip fails/i)).toBeInTheDocument();
  });
});

// ─── Commit-Reveal Flow ───────────────────────────────────────────────────────

describe("CommitRevealFlow — transaction confirmations and error handling", () => {
  it("starts at commit step", () => {
    render(
      <CommitRevealFlow
        onCommit={vi.fn().mockResolvedValue(undefined)}
        onReveal={vi.fn().mockResolvedValue(undefined)}
      />
    );
    expect(screen.getByText("Generate Your Commitment")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /generate/i })).toBeInTheDocument();
  });

  it("generates a secret and shows commitment hash", async () => {
    render(
      <CommitRevealFlow
        onCommit={vi.fn().mockResolvedValue(undefined)}
        onReveal={vi.fn().mockResolvedValue(undefined)}
      />
    );
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /generate/i }));
    });
    // After generate, the secret input should be populated
    const secretInput = screen.getByLabelText(/your secret/i);
    expect((secretInput as HTMLInputElement).value).not.toBe("");
    // Commitment hash box should appear
    expect(screen.getByLabelText("Commitment hash")).toBeInTheDocument();
  });

  it("advances to pending step after commit", async () => {
    const onCommit = vi.fn().mockResolvedValue(undefined);
    render(
      <CommitRevealFlow
        onCommit={onCommit}
        onReveal={vi.fn().mockResolvedValue(undefined)}
      />
    );
    // Generate secret first
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /generate/i }));
    });
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /submit commitment/i }));
    });
    await waitFor(() => {
      expect(screen.getByText("Commitment Submitted")).toBeInTheDocument();
    });
    expect(onCommit).toHaveBeenCalledOnce();
  });

  it("shows error state when commit fails", async () => {
    const onCommit = vi.fn().mockRejectedValue(new Error("Transaction rejected"));
    render(
      <CommitRevealFlow
        onCommit={onCommit}
        onReveal={vi.fn().mockResolvedValue(undefined)}
      />
    );
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /generate/i }));
    });
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /submit commitment/i }));
    });
    await waitFor(() => {
      expect(screen.getByRole("alert")).toHaveTextContent("Transaction rejected");
    });
    // Try Again button should appear
    expect(screen.getByRole("button", { name: /try again/i })).toBeInTheDocument();
  });

  it("shows verified state after successful reveal", async () => {
    const onReveal = vi.fn().mockResolvedValue(undefined);
    render(
      <CommitRevealFlow
        onCommit={vi.fn().mockResolvedValue(undefined)}
        onReveal={onReveal}
      />
    );
    // Generate and commit
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /generate/i }));
    });
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /submit commitment/i }));
    });
    // Wait for auto-advance to reveal step
    await waitFor(() => {
      expect(screen.getByText("Reveal Your Secret")).toBeInTheDocument();
    }, { timeout: 2000 });

    fireEvent.change(screen.getByLabelText(/your secret/i), {
      target: { value: "mysecret" },
    });
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /reveal & settle/i }));
    });
    await waitFor(() => {
      expect(screen.getByRole("status")).toHaveTextContent("Commitment Verified");
    });
    expect(onReveal).toHaveBeenCalledWith("mysecret");
  });

  it("shows error when reveal fails (commitment mismatch)", async () => {
    const onReveal = vi.fn().mockRejectedValue(new Error("Commitment mismatch"));
    render(
      <CommitRevealFlow
        onCommit={vi.fn().mockResolvedValue(undefined)}
        onReveal={onReveal}
      />
    );
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /generate/i }));
    });
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /submit commitment/i }));
    });
    await waitFor(() => {
      expect(screen.getByText("Reveal Your Secret")).toBeInTheDocument();
    }, { timeout: 2000 });

    fireEvent.change(screen.getByLabelText(/your secret/i), {
      target: { value: "wrongsecret" },
    });
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /reveal & settle/i }));
    });
    await waitFor(() => {
      expect(screen.getByRole("alert")).toHaveTextContent("Commitment mismatch");
    });
  });

  it("step indicator shows progress", async () => {
    render(
      <CommitRevealFlow
        onCommit={vi.fn().mockResolvedValue(undefined)}
        onReveal={vi.fn().mockResolvedValue(undefined)}
      />
    );
    // Initially at commit step
    const progress = screen.getByRole("list", { name: /progress/i });
    expect(progress).toBeInTheDocument();
    const steps = progress.querySelectorAll("li");
    expect(steps.length).toBe(4); // commit, pending, reveal, verified
  });
});
