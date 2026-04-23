import React, { Profiler, Suspense, memo, useMemo, useCallback, useState } from "react";
import { render, fireEvent, act } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
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
    expect(elapsed).toBeLessThan(2000);
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

    expect(openElapsed).toBeLessThan(200);
    expect(closeElapsed).toBeLessThan(100);
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

describe("optimization and re-render tracking", () => {
  it("tracks re-render counts with React Profiler", () => {
    const onRender = vi.fn();
    
    const TestComponent = () => {
      const [count, setCount] = useState(0);
      return <button onClick={() => setCount(c => c + 1)}>Count {count}</button>;
    };

    const { getByText } = render(
      <Profiler id="TestComponent" onRender={onRender}>
        <TestComponent />
      </Profiler>
    );

    expect(onRender).toHaveBeenCalledTimes(1);
    
    fireEvent.click(getByText("Count 0"));
    expect(onRender).toHaveBeenCalledTimes(2);
  });

  it("validates memoization effectiveness with useMemo/useCallback", () => {
    const onChildRender = vi.fn();
    
    const MemoizedChild = memo(({ data, onAction }: { data: { val: number }, onAction: () => void }) => {
      return (
        <Profiler id="MemoizedChild" onRender={onChildRender}>
          <div onClick={onAction}>{data.val}</div>
        </Profiler>
      );
    });
    MemoizedChild.displayName = "MemoizedChild";

    const Parent = () => {
      const [count, setCount] = useState(0);
      
      // Memoized props
      const data = useMemo(() => ({ val: 42 }), []);
      const onAction = useCallback(() => {}, []);
      
      return (
        <div>
          <button onClick={() => setCount(c => c + 1)}>Increment {count}</button>
          <MemoizedChild data={data} onAction={onAction} />
        </div>
      );
    };

    const { getByText } = render(<Parent />);
    
    // Initial render
    expect(onChildRender).toHaveBeenCalledTimes(1);
    
    // Parent re-renders, but MemoizedChild should NOT because props didn't change
    fireEvent.click(getByText("Increment 0"));
    
    // Ensure parent re-rendered
    expect(getByText("Increment 1")).toBeInTheDocument();
    
    // Child should still only have 1 render
    expect(onChildRender).toHaveBeenCalledTimes(1);
  });

  it("tests lazy loading and code splitting boundaries", async () => {
    // Mock a dynamic import
    const LazyComponent = React.lazy(() => 
      Promise.resolve({
        default: () => <div data-testid="lazy-loaded">Loaded Content</div>
      })
    );

    const onSuspenseRender = vi.fn();
    
    const { getByTestId, findByTestId } = render(
      <Profiler id="SuspenseBoundary" onRender={onSuspenseRender}>
        <Suspense fallback={<div data-testid="loading">Loading...</div>}>
          <LazyComponent />
        </Suspense>
      </Profiler>
    );

    // Initial render should show loading state
    expect(getByTestId("loading")).toBeInTheDocument();
    
    // Wait for the lazy component to resolve and render
    const loadedContent = await findByTestId("lazy-loaded");
    expect(loadedContent).toBeInTheDocument();
    
    // Profiler should have recorded renders (initial + resolved)
    expect(onSuspenseRender.mock.calls.length).toBeGreaterThanOrEqual(2);
  });
});

describe("critical path performance", () => {
  it("game flow critical path meets budget requirements", () => {
    // Simulating the rendering of the core game components together
    const elapsed = measureRender(() => {
      render(
        <div>
          <GameFlowSteps />
          <CoinFlip state="betting" result="heads" />
          <TransactionHistory records={makeRecords(10)} mode="paginate" />
        </div>
      );
    });
    
    // Total budget for critical path (e.g. 150ms)
    expect(elapsed).toBeLessThan(150);
  });
});
