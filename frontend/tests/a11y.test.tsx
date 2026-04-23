/**
 * Accessibility test suite — WCAG AA compliance via axe-core (jest-axe).
 *
 * Run: npx jest --testPathPattern="a11y"
 * CI:  npx jest --testPathPattern="a11y" --ci
 *
 * References: #322
 */

import React, { useEffect, useRef, useState } from "react";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { axe, toHaveNoViolations } from "jest-axe";
import { vi } from "vitest";
import tokens from "../tokens/tossd.tokens.json";
import { App } from "../src/App";
import { Button } from "../components/Button";
import { CoinFlip } from "../components/CoinFlip";
import { GameFlowSteps } from "../components/GameFlowSteps";
import { CommitRevealFlow } from "../components/CommitRevealFlow";
import { GameResult } from "../components/GameResult";
import { GameStateCard } from "../components/GameStateCard";
import { HeroSection } from "../components/HeroSection";
import { LoadingSpinner } from "../components/LoadingSpinner";
import { CTABand } from "../components/CTABand";
import { MultiplierProgression } from "../components/MultiplierProgression";
import { OutcomeChip } from "../components/OutcomeChip";
import { ProofCard } from "../components/ProofCard";
import { EconomicsPanel } from "../components/EconomicsPanel";
import { SecuritySection } from "../components/SecuritySection";
import { SideSelector } from "../components/SideSelector";
import { FairnessTimeline } from "../components/FairnessTimeline";
import { VerificationPanel } from "../components/VerificationPanel";
import { Footer } from "../components/Footer";
import { TrustStrip } from "../components/TrustStrip";
import { NavBar } from "../components/NavBar";
import { MobileMenu } from "../components/MobileMenu";
import { Modal } from "../components/Modal";
import { ErrorBoundary } from "../components/ErrorBoundary";
import { WagerInput } from "../components/WagerInput";
import { ToastProvider } from "../components/ToastProvider";
import { StatsDashboard } from "../components/StatsDashboard";
import { TransactionHistory } from "../components/TransactionHistory";
import { WalletModal } from "../components/WalletModal";
import { useToast } from "../components/ToastContext";

expect.extend(toHaveNoViolations);

// ─── Helpers ────────────────────────────────────────────────────────────────

async function expectNoViolations(
  ui: React.ReactElement,
  target: "container" | "body" = "container"
) {
  const { container } = render(ui);
  const results = await axe(target === "body" ? document.body : container);
  expect(results).toHaveNoViolations();
}

/** Parse a hex color string to [r, g, b] in 0-255. */
function hexToRgb(hex: string): [number, number, number] {
  const h = hex.replace("#", "");
  const n = parseInt(h, 16);
  return [(n >> 16) & 0xff, (n >> 8) & 0xff, n & 0xff];
}

/** Relative luminance per WCAG 2.1 1.4.3. */
function luminance([r, g, b]: [number, number, number]): number {
  const c = [r, g, b].map((v) => {
    const s = v / 255;
    return s <= 0.03928 ? s / 12.92 : Math.pow((s + 0.055) / 1.055, 2.4);
  });
  return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
}

/** WCAG contrast ratio between two hex colors. */
function contrast(fg: string, bg: string): number {
  const l1 = luminance(hexToRgb(fg));
  const l2 = luminance(hexToRgb(bg));
  const [light, dark] = l1 > l2 ? [l1, l2] : [l2, l1];
  return (light + 0.05) / (dark + 0.05);
}

// ─── Static sections ────────────────────────────────────────────────────────

describe("HeroSection a11y", () => {
  it("has no axe violations", () => expectNoViolations(<HeroSection />));
  it("has a labelled landmark", () => {
    const { getByRole } = render(<HeroSection />);
    expect(getByRole("region", { name: /hero/i })).toBeInTheDocument();
  });
  it("CTA links have accessible names", () => {
    const { getAllByRole } = render(<HeroSection />);
    getAllByRole("link").forEach((l) => expect(l).toHaveAccessibleName());
  });
});

describe("CTABand a11y", () => {
  it("has no axe violations", () => expectNoViolations(<CTABand />));
  it("section has accessible name", () => {
    const { getByRole } = render(<CTABand />);
    expect(getByRole("region", { name: /play with proof/i })).toBeInTheDocument();
  });
  it("buttons/links have accessible names", () => {
    const { getAllByRole } = render(<CTABand />);
    getAllByRole("link").forEach((l) => expect(l).toHaveAccessibleName());
  });
});

describe("SecuritySection a11y", () => {
  it("has no axe violations", () => expectNoViolations(<SecuritySection />));
  it("has labelled section", () => {
    const { getByRole } = render(<SecuritySection />);
    expect(getByRole("region", { name: /security/i })).toBeInTheDocument();
  });
  it("feature list uses role=list", () => {
    const { getByRole } = render(<SecuritySection />);
    expect(getByRole("list")).toBeInTheDocument();
  });
});

describe("EconomicsPanel a11y", () => {
  it("has no axe violations", () => expectNoViolations(<EconomicsPanel />));
});

describe("FairnessTimeline a11y", () => {
  it("has no axe violations", () => expectNoViolations(<FairnessTimeline />));
});

describe("VerificationPanel a11y", () => {
  it("has no axe violations", () => expectNoViolations(<VerificationPanel />));
});

// ─── NavBar ─────────────────────────────────────────────────────────────────

describe("NavBar a11y", () => {
  it("has no axe violations", () => expectNoViolations(<NavBar />));
  it("has a banner landmark", () => {
    const { getByRole } = render(<NavBar />);
    expect(getByRole("banner")).toBeInTheDocument();
  });
  it("hamburger button has accessible name", () => {
    const { getByRole } = render(<NavBar />);
    expect(getByRole("button", { name: /open navigation menu/i })).toBeInTheDocument();
  });
  it("hamburger has aria-expanded=false initially", () => {
    const { getByRole } = render(<NavBar />);
    expect(getByRole("button", { name: /open navigation menu/i })).toHaveAttribute(
      "aria-expanded",
      "false"
    );
  });
  it("desktop nav has accessible name", () => {
    const { getByRole } = render(<NavBar />);
    expect(getByRole("navigation", { name: /primary navigation/i })).toBeInTheDocument();
  });
});

// ─── MobileMenu ─────────────────────────────────────────────────────────────

function MobileMenuFixture({ open }: { open: boolean }) {
  const triggerRef = useRef<HTMLButtonElement>(null);
  return (
    <>
      <button ref={triggerRef}>Trigger</button>
      <MobileMenu open={open} onClose={() => {}} triggerRef={triggerRef}>
        <button>Item 1</button>
        <button>Item 2</button>
      </MobileMenu>
    </>
  );
}

describe("MobileMenu a11y", () => {
  it("has no axe violations when closed", () =>
    expectNoViolations(<MobileMenuFixture open={false} />));
  it("has no axe violations when open", () =>
    expectNoViolations(<MobileMenuFixture open={true} />));
  it("dialog has aria-modal=true when open", () => {
    const { getByRole } = render(<MobileMenuFixture open={true} />);
    expect(getByRole("dialog")).toHaveAttribute("aria-modal", "true");
  });
  it("dialog has accessible label", () => {
    const { getByRole } = render(<MobileMenuFixture open={true} />);
    expect(getByRole("dialog", { name: /navigation menu/i })).toBeInTheDocument();
  });
  it("not rendered when closed", () => {
    const { queryByRole } = render(<MobileMenuFixture open={false} />);
    expect(queryByRole("dialog")).not.toBeInTheDocument();
  });
  it("mobile nav has accessible name", () => {
    const { getByRole } = render(<MobileMenuFixture open={true} />);
    expect(getByRole("dialog", { name: /navigation menu/i })).toBeInTheDocument();
  });
});

// ─── Modal ──────────────────────────────────────────────────────────────────

function ModalFixture({ open }: { open: boolean }) {
  return (
    <Modal open={open} onClose={() => {}} titleId="modal-title">
      <h2 id="modal-title">Test Modal</h2>
      <button>Action</button>
    </Modal>
  );
}

describe("Modal a11y", () => {
  it("has no axe violations when open", () =>
    expectNoViolations(<ModalFixture open={true} />, "body"));
  it("has no axe violations when closed", () =>
    expectNoViolations(<ModalFixture open={false} />));
  it("dialog has aria-modal=true", () => {
    const { getByRole } = render(<ModalFixture open={true} />);
    expect(getByRole("dialog")).toHaveAttribute("aria-modal", "true");
  });
  it("dialog is labelled by titleId", () => {
    const { getByRole } = render(<ModalFixture open={true} />);
    expect(getByRole("dialog")).toHaveAttribute("aria-labelledby", "modal-title");
  });
  it("not rendered when closed", () => {
    const { queryByRole } = render(<ModalFixture open={false} />);
    expect(queryByRole("dialog")).not.toBeInTheDocument();
  });
});

// ─── ErrorBoundary ──────────────────────────────────────────────────────────

describe("ErrorBoundary a11y", () => {
  it("has no axe violations in normal state", () =>
    expectNoViolations(
      <ErrorBoundary>
        <p>Content</p>
      </ErrorBoundary>
    ));
  it("renders children without violations", () => {
    const { getByText } = render(
      <ErrorBoundary>
        <p>Content</p>
      </ErrorBoundary>
    );
    expect(getByText("Content")).toBeInTheDocument();
  });
});

// --- Additional component coverage ---

describe("Button a11y", () => {
  it("has no axe violations in the default state", () =>
    expectNoViolations(<Button>Launch game</Button>));

  it("exposes a loading spinner to assistive tech", () => {
    render(<Button loading>Save</Button>);
    expect(screen.getByRole("status", { name: /loading/i })).toBeInTheDocument();
    expect(screen.getByRole("button")).toHaveAttribute("aria-busy", "true");
  });
});

describe("CoinFlip a11y", () => {
  it("has no axe violations when revealed", () =>
    expectNoViolations(<CoinFlip state="revealed" result="heads" />));

  it("announces the landed face", () => {
    render(<CoinFlip state="revealed" result="tails" />);
    expect(screen.getByLabelText(/coin landed on tails/i)).toBeInTheDocument();
    expect(screen.getByText(/result: tails/i)).toBeInTheDocument();
  });
});

describe("OutcomeChip a11y", () => {
  it("has no axe violations in win state", () =>
    expectNoViolations(<OutcomeChip state="win" />));

  it("exposes a status role and meaningful name", () => {
    render(<OutcomeChip state="pending" />);
    expect(screen.getByRole("status", { name: /outcome: pending/i })).toBeInTheDocument();
  });
});

describe("MultiplierProgression a11y", () => {
  it("has no axe violations", () =>
    expectNoViolations(<MultiplierProgression activeStep={2} />));

  it("marks the active multiplier step", () => {
    render(<MultiplierProgression activeStep={1} />);
    expect(screen.getByRole("list", { name: /multiplier progression/i })).toBeInTheDocument();
    expect(screen.getByText("3.5x").closest("li")).toHaveAttribute("aria-current", "step");
  });
});

describe("ProofCard a11y", () => {
  it("has no axe violations", () => expectNoViolations(<ProofCard />));

  it("exposes a useful accessible label", () => {
    render(<ProofCard />);
    expect(screen.getByLabelText(/proof card mock/i)).toBeInTheDocument();
  });
});

describe("WagerInput a11y", () => {
  it("has no axe violations in the default state", () =>
    expectNoViolations(<WagerInput />));

  it("associates the error message with the input", () => {
    render(<WagerInput min={10} max={100} value="5" />);
    const input = screen.getByRole("textbox");
    expect(input).toHaveAttribute("aria-invalid", "true");
    expect(input).toHaveAttribute("aria-describedby");
    expect(screen.getByRole("alert")).toHaveTextContent(/minimum wager is 10/i);
  });
});

describe("SideSelector a11y", () => {
  it("has no axe violations with heads selected", () =>
    expectNoViolations(<SideSelector value="heads" onChange={() => {}} />));

  it("supports keyboard toggling", async () => {
    const onChange = vi.fn();
    const user = userEvent.setup();
    render(<SideSelector value="heads" onChange={onChange} />);
    await user.click(screen.getByRole("radio", { name: /heads/i }));
    await user.keyboard("{ArrowRight}");
    expect(onChange).toHaveBeenCalledWith("tails");
  });
});

describe("GameResult a11y", () => {
  it("has no axe violations when winning", () =>
    expectNoViolations(
      <GameResult
        outcome="win"
        wager={10_000_000}
        payout={18_430_000}
        streak={1}
        onCashOut={() => {}}
        onContinue={() => {}}
      />
    ));

  it("announces win and loss states with status role", () => {
    render(
      <GameResult
        outcome="loss"
        wager={10_000_000}
        onPlayAgain={() => {}}
      />
    );
    expect(screen.getByRole("status", { name: /you lost/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /play again/i })).toBeInTheDocument();
  });
});

describe("GameStateCard a11y", () => {
  const baseGame = {
    phase: "won" as const,
    side: "heads" as const,
    wagerStroops: 10_000_000,
    streak: 2,
  };

  it("has no axe violations in a settled state", () =>
    expectNoViolations(
      <GameStateCard
        game={baseGame}
        onCashOut={() => {}}
        onContinue={() => {}}
      />
    ));

  it("announces live updates and busy actions", () => {
    render(
      <GameStateCard
        game={baseGame}
        onCashOut={() => {}}
        onContinue={() => {}}
        loading
      />
    );
    expect(screen.getByText(/you won!/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /cash out/i })).toHaveAttribute("aria-busy", "true");
    expect(screen.getByRole("button", { name: /continue streak/i })).toHaveAttribute(
      "aria-busy",
      "true"
    );
  });
});

describe("LoadingSpinner a11y", () => {
  it("has no axe violations", () => expectNoViolations(<LoadingSpinner />));

  it("is announced as status text", () => {
    render(<LoadingSpinner size="small" label="Loading contract stats" />);
    expect(screen.getByRole("status", { name: /loading contract stats/i })).toBeInTheDocument();
  });
});

describe("TrustStrip a11y", () => {
  it("has no axe violations", () => expectNoViolations(<TrustStrip />));

  it("is exposed as a labelled list", () => {
    render(<TrustStrip />);
    expect(screen.getByRole("list", { name: /trust indicators/i })).toBeInTheDocument();
  });
});

describe("Footer a11y", () => {
  it("has no axe violations", () => expectNoViolations(<Footer />));

  it("exposes landmarks and link names", () => {
    render(<Footer />);
    expect(screen.getByRole("contentinfo")).toBeInTheDocument();
    screen.getAllByRole("link").forEach((link) => {
      expect(link).toHaveAccessibleName();
    });
  });
});

describe("GameFlowSteps a11y", () => {
  it("has no axe violations", () => expectNoViolations(<GameFlowSteps />));

  it("uses an accessible region and ordered list", () => {
    render(<GameFlowSteps />);
    expect(screen.getByRole("region", { name: /game flow steps/i })).toBeInTheDocument();
    expect(screen.getByRole("list")).toBeInTheDocument();
  });
});

describe("CommitRevealFlow a11y", () => {
  const noop = async () => {};

  it("has no axe violations on the commit step", () =>
    expectNoViolations(<CommitRevealFlow onCommit={noop} onReveal={noop} />));

  it("keeps the secret field associated with its hint", () => {
    render(<CommitRevealFlow onCommit={noop} onReveal={noop} />);
    expect(screen.getByRole("textbox", { name: /your secret/i })).toHaveAttribute(
      "aria-describedby",
      expect.stringContaining("secret-hint")
    );
    expect(screen.getByRole("button", { name: /generate/i })).toBeInTheDocument();
  });
});

function ModalFocusFixture() {
  const [open, setOpen] = useState(false);
  const initialFocusRef = useRef<HTMLButtonElement>(null);

  return (
    <>
      <button onClick={() => setOpen(true)}>Open modal</button>
      <Modal open={open} onClose={() => setOpen(false)} titleId="focus-modal-title" initialFocusRef={initialFocusRef}>
        <h2 id="focus-modal-title">Focus Fixture</h2>
        <button ref={initialFocusRef} onClick={() => setOpen(false)}>
          Primary action
        </button>
        <button>Secondary action</button>
      </Modal>
    </>
  );
}

function MobileMenuFocusFixture({ open }: { open: boolean }) {
  const triggerRef = useRef<HTMLButtonElement>(null);
  return (
    <>
      <button ref={triggerRef}>Menu trigger</button>
      <MobileMenu open={open} onClose={() => {}} triggerRef={triggerRef}>
        <button>Item 1</button>
        <button>Item 2</button>
      </MobileMenu>
    </>
  );
}

describe("Modal a11y", () => {
  it("has no axe violations when open", () =>
    expectNoViolations(<ModalFocusFixture />));

  it("moves focus into the modal and restores it on close", async () => {
    const user = userEvent.setup();
    render(<ModalFocusFixture />);

    const openButton = screen.getByRole("button", { name: /open modal/i });
    openButton.focus();
    await user.click(openButton);

    await waitFor(() =>
      expect(screen.getByRole("button", { name: /primary action/i })).toHaveFocus()
    );

    await user.keyboard("{Escape}");
    await waitFor(() =>
      expect(screen.queryByRole("dialog", { name: /focus fixture/i })).not.toBeInTheDocument()
    );
  });
});

describe("MobileMenu a11y", () => {
  it("has no axe violations when open", () =>
    expectNoViolations(<MobileMenuFocusFixture open />));

  it("is labelled as a navigation dialog", () => {
    render(<MobileMenuFocusFixture open />);
    expect(screen.getByRole("dialog", { name: /navigation menu/i })).toBeInTheDocument();
  });
});

describe("NavBar a11y", () => {
  it("has no axe violations", () => expectNoViolations(<NavBar />));

  it("opens the mobile menu with a labelled dialog and keyboard focus trap", async () => {
    const user = userEvent.setup();
    render(<NavBar />);

    const hamburger = screen.getByRole("button", { name: /open navigation menu/i });
    await user.click(hamburger);

    const dialog = screen.getByRole("dialog", { name: /navigation menu/i });
    await waitFor(() =>
      expect(within(dialog).getByRole("button", { name: /close navigation menu/i })).toHaveFocus()
    );

    const launchLink = within(dialog).getByRole("link", { name: /launch app/i });
    launchLink.focus();
    await user.keyboard("{Tab}");
    expect(within(dialog).getByRole("button", { name: /close navigation menu/i })).toHaveFocus();

    await user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("dialog", { name: /navigation menu/i })).not.toBeInTheDocument());
  });
});

describe("WalletModal a11y", () => {
  it("has no axe violations when open", () =>
    expectNoViolations(
      <WalletModal open onClose={() => {}} connectWallet={async () => "GABC123"} />,
      "body"
    ));

  it("labels the dialog and the wallet options", () => {
    render(<WalletModal open onClose={() => {}} connectWallet={async () => "GABC123"} />);
    expect(screen.getByRole("dialog", { name: /connect wallet/i })).toBeInTheDocument();
    expect(screen.getAllByRole("list").length).toBeGreaterThan(0);
    screen.getAllByRole("button").forEach((button) => {
      expect(button).toHaveAccessibleName();
    });
  });
});

describe("ToastProvider a11y", () => {
  function ToastProbe() {
    const { addToast } = useToast();
    useEffect(() => {
      addToast({ type: "info", message: "Wallet connected", duration: 0 });
    }, [addToast]);

    return <p>Toast host</p>;
  }

  it("has no axe violations with a live toast", async () => {
    render(
      <ToastProvider>
        <main aria-label="Toast host">
          <ToastProbe />
        </main>
      </ToastProvider>
    );
    await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent(/wallet connected/i));
    const results = await axe(document.body);
    expect(results).toHaveNoViolations();
  });

  it("exposes a polite notification region", async () => {
    render(
      <ToastProvider>
        <main aria-label="Toast host">
          <ToastProbe />
        </main>
      </ToastProvider>
    );
    await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent(/wallet connected/i));
    expect(screen.getByLabelText(/notifications/i)).toHaveAttribute("aria-live", "polite");
  });
});

describe("StatsDashboard a11y", () => {
  it("has no axe violations in the loading state", () =>
    expectNoViolations(
      <StatsDashboard
        fetchStats={async () => {
          await new Promise<never>(() => {});
          return {
            total_games: 0,
            total_volume: 0,
            total_fees: 0,
            reserve_balance: 0,
          };
        }}
        pollInterval={0}
      />
    ));

  it("announces fetch failures", async () => {
    render(
      <StatsDashboard
        fetchStats={async () => {
          throw new Error("stats unavailable");
        }}
        pollInterval={0}
      />
    );
    await waitFor(() => expect(screen.getByRole("alert")).toHaveTextContent(/failed to load/i));
  });
});

describe("TransactionHistory a11y", () => {
  const records = Array.from({ length: 24 }, (_, i) => ({
    id: `game-${i + 1}`,
    timestamp: Date.now() - i * 60_000,
    side: i % 2 === 0 ? ("heads" as const) : ("tails" as const),
    wagerStroops: 10_000_000,
    payoutStroops: i % 3 === 0 ? 18_430_000 : null,
    outcome: i % 3 === 0 ? ("win" as const) : i % 3 === 1 ? ("loss" as const) : ("pending" as const),
    streak: i % 4,
  }));

  it("has no axe violations in paginated mode", () =>
    expectNoViolations(<TransactionHistory records={records} mode="paginate" />));

  it("exposes an accessible history navigation region", () => {
    render(<TransactionHistory records={records} mode="paginate" />);
    expect(screen.getByRole("navigation", { name: /history pages/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /next page/i })).toBeInTheDocument();
  });
});

function ExplodingChild() {
  throw new Error("Synthetic boundary failure");
}

describe("ErrorBoundary a11y", () => {
  it("announces failures and keeps the fallback focusable", async () => {
    const consoleError = vi.spyOn(console, "error").mockImplementation(() => {});

    render(
      <ErrorBoundary showDetails>
        <ExplodingChild />
      </ErrorBoundary>
    );

    expect(screen.getByRole("alert")).toHaveAttribute("aria-atomic", "true");
    await waitFor(() =>
      expect(screen.getByRole("heading", { name: /something went wrong/i })).toHaveFocus()
    );
    expect(screen.getByRole("button", { name: /try again/i })).toBeInTheDocument();

    consoleError.mockRestore();
  });
});

describe("App a11y", () => {
  it("has no axe violations across the composed shell", () =>
    expectNoViolations(<App />, "body"));
});

describe("Color contrast", () => {
  it("token foreground colors meet WCAG AA on key backgrounds", () => {
    const { color } = tokens;
    expect(contrast(color.fg.primary, color.bg.base)).toBeGreaterThanOrEqual(4.5);
    expect(contrast(color.fg.primary, color.bg.surface)).toBeGreaterThanOrEqual(4.5);
    expect(contrast(color.fg.secondary, color.bg.surface)).toBeGreaterThanOrEqual(4.5);
    expect(contrast(color.fg.muted, color.bg.base)).toBeGreaterThanOrEqual(4.5);
  });

  it("focus ring color is visible against the surface background", () => {
    const { color } = tokens;
    expect(contrast(color.focus.ring, color.bg.surface)).toBeGreaterThanOrEqual(3);
  });
});
