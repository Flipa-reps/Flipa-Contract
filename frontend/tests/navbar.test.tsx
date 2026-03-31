/**
 * Tests for NavBar (header navigation component)
 * Covers: rendering, links, wallet button, hamburger, mobile menu, scroll elevation
 */

import React from "react";
import { render, screen, fireEvent, act, waitFor } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { NavBar } from "../components/NavBar";

// ── Rendering ─────────────────────────────────────────────────────────────────

describe("NavBar — rendering", () => {
  it("renders as a banner landmark", () => {
    render(<NavBar />);
    expect(screen.getByRole("banner")).toBeInTheDocument();
  });

  it("renders the logo link", () => {
    render(<NavBar />);
    expect(screen.getByRole("link", { name: /tossd home/i })).toBeInTheDocument();
  });

  it("renders all desktop nav links", () => {
    render(<NavBar />);
    expect(screen.getByRole("link", { name: /how it works/i })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /economics/i })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /security/i })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /audit contract/i })).toBeInTheDocument();
  });

  it("renders the Launch App CTA link", () => {
    render(<NavBar />);
    expect(screen.getByRole("link", { name: /launch app/i })).toBeInTheDocument();
  });
});

// ── Wallet button ─────────────────────────────────────────────────────────────

describe("NavBar — wallet button", () => {
  it("shows 'Connect Wallet' by default", () => {
    render(<NavBar />);
    // Both desktop and mobile render the label; getAll to avoid ambiguity
    const btns = screen.getAllByRole("button", { name: /connect wallet/i });
    expect(btns.length).toBeGreaterThanOrEqual(1);
  });

  it("shows 'Connected' when walletConnected=true", () => {
    render(<NavBar walletConnected />);
    const btns = screen.getAllByRole("button", { name: /connected/i });
    expect(btns.length).toBeGreaterThanOrEqual(1);
  });

  it("calls onConnectWallet when desktop wallet button is clicked", () => {
    const onConnect = vi.fn();
    render(<NavBar onConnectWallet={onConnect} />);
    // The desktop wallet button has aria-label="Connect Wallet"
    const btn = screen.getAllByRole("button", { name: /connect wallet/i })[0];
    fireEvent.click(btn);
    expect(onConnect).toHaveBeenCalledTimes(1);
  });

  it("wallet button has data-wallet-connected=false by default", () => {
    render(<NavBar />);
    const btn = screen.getAllByRole("button", { name: /connect wallet/i })[0];
    expect(btn).toHaveAttribute("data-wallet-connected", "false");
  });

  it("wallet button has data-wallet-connected=true when connected", () => {
    render(<NavBar walletConnected />);
    const btn = screen.getAllByRole("button", { name: /connected/i })[0];
    expect(btn).toHaveAttribute("data-wallet-connected", "true");
  });
});

// ── Hamburger / mobile menu ───────────────────────────────────────────────────

describe("NavBar — mobile menu", () => {
  it("hamburger button is present", () => {
    render(<NavBar />);
    expect(screen.getByRole("button", { name: /open navigation menu/i })).toBeInTheDocument();
  });

  it("hamburger has aria-expanded=false initially", () => {
    render(<NavBar />);
    expect(screen.getByRole("button", { name: /open navigation menu/i }))
      .toHaveAttribute("aria-expanded", "false");
  });

  it("opens mobile menu on hamburger click", () => {
    render(<NavBar />);
    fireEvent.click(screen.getByRole("button", { name: /open navigation menu/i }));
    expect(screen.getByRole("dialog", { name: /navigation menu/i })).toBeInTheDocument();
  });

  it("hamburger has aria-expanded=true when menu is open", () => {
    render(<NavBar />);
    const hamburger = screen.getByRole("button", { name: /open navigation menu/i });
    fireEvent.click(hamburger);
    expect(hamburger).toHaveAttribute("aria-expanded", "true");
  });

  it("closes mobile menu when close button is clicked", async () => {
    render(<NavBar />);
    fireEvent.click(screen.getByRole("button", { name: /open navigation menu/i }));
    fireEvent.click(screen.getByRole("button", { name: /close navigation menu/i }));
    await waitFor(() =>
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument()
    , { timeout: 500 });
  });

  it("closes mobile menu when a nav link is clicked", async () => {
    render(<NavBar />);
    fireEvent.click(screen.getByRole("button", { name: /open navigation menu/i }));
    const dialog = screen.getByRole("dialog");
    const link = dialog.querySelector("a");
    if (link) fireEvent.click(link);
    await waitFor(() =>
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument()
    , { timeout: 500 });
  });
});

// ── Scroll elevation ──────────────────────────────────────────────────────────

describe("NavBar — scroll elevation", () => {
  beforeEach(() => {
    Object.defineProperty(window, "scrollY", { writable: true, configurable: true, value: 0 });
  });

  afterEach(() => {
    Object.defineProperty(window, "scrollY", { writable: true, configurable: true, value: 0 });
  });

  it("does not have scrolled attribute at top", () => {
    render(<NavBar />);
    expect(screen.getByRole("banner")).toHaveAttribute("data-scrolled", "false");
  });

  it("gains scrolled attribute after scrolling past 4px", () => {
    render(<NavBar />);
    act(() => {
      Object.defineProperty(window, "scrollY", { writable: true, configurable: true, value: 10 });
      window.dispatchEvent(new Event("scroll"));
    });
    expect(screen.getByRole("banner")).toHaveAttribute("data-scrolled", "true");
  });
});
