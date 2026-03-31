import React from "react";
import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { GameFlowSteps } from "../components/GameFlowSteps";
import { Modal } from "../components/Modal";
import { TransactionHistory, GameRecord } from "../components/TransactionHistory";
import { CoinFlip } from "../components/CoinFlip";
import { VITAL_THRESHOLDS, rateVital } from "../perf/vitals";

function measureRender(fn: () => void): number {
  const start = performance.now();
  fn();
  return performance.now() - start;
}

function makeRecords(count: number): GameRecord[] {
  const now = Date.now();
  return Array.from({ length: count }, (_, i) => ({
    id: `perf-${i}`,
    timestamp: now - i * 1000,
    side: i % 2 === 0 ? "heads" : "tails",
    wagerStroops: 10_000_000,
    payoutStroops: i % 3 === 0 ? null : 19_000_000,
    outcome: i % 3 === 0 ? "loss" : "win",
    streak: i % 4,
    txHash: `0x${i}`,
  }));
}

describe("frontend render performance", () => {
  it("renders TransactionHistory with 120 items under budget", () => {
    const records = makeRecords(120);
    const elapsed = measureRender(() => render(<TransactionHistory records={records} mode="paginate" />));
    expect(elapsed).toBeLessThan(90);
  });

  it("renders GameFlowSteps under budget", () => {
    const elapsed = measureRender(() => render(<GameFlowSteps />));
    expect(elapsed).toBeLessThan(40);
  });

  it("modal open/close render remains under budget", () => {
    const openElapsed = measureRender(() =>
      render(
        <Modal open={true} onClose={() => {}} titleId="t1">
          <h2 id="t1">Title</h2>
          <button>Ok</button>
        </Modal>
      )
    );

    const closeElapsed = measureRender(() =>
      render(
        <Modal open={false} onClose={() => {}} titleId="t2">
          <h2 id="t2">Title</h2>
        </Modal>
      )
    );

    expect(openElapsed).toBeLessThan(55);
    expect(closeElapsed).toBeLessThan(35);
  });

  it("animation component render stays under budget", () => {
    const elapsed = measureRender(() => render(<CoinFlip state="flipping" result="heads" />));
    expect(elapsed).toBeLessThan(35);
  });
});

describe("core web vitals budgets", () => {
  it("uses strict budgets for LCP, FID, CLS", () => {
    expect(VITAL_THRESHOLDS.LCP[0]).toBeLessThanOrEqual(2500);
    expect(VITAL_THRESHOLDS.FID[0]).toBeLessThanOrEqual(100);
    expect(VITAL_THRESHOLDS.CLS[0]).toBeLessThanOrEqual(0.1);
  });

  it("classifies budget breaches correctly", () => {
    expect(rateVital("LCP", 1800)).toBe("good");
    expect(rateVital("LCP", 3200)).toBe("needs-improvement");
    expect(rateVital("LCP", 4300)).toBe("poor");

    expect(rateVital("FID", 80)).toBe("good");
    expect(rateVital("FID", 220)).toBe("needs-improvement");
    expect(rateVital("FID", 350)).toBe("poor");

    expect(rateVital("CLS", 0.05)).toBe("good");
    expect(rateVital("CLS", 0.2)).toBe("needs-improvement");
    expect(rateVital("CLS", 0.5)).toBe("poor");
  });
});
