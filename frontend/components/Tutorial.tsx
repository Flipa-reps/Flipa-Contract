import React, { useCallback, useEffect, useRef, useState } from "react";
import styles from "./Tutorial.module.css";

export interface TutorialStep {
  title: string;
  description: string;
  /** CSS selector for the element to highlight. Omit for a centered tooltip. */
  target?: string;
  /** Tooltip placement relative to the target */
  placement?: "top" | "bottom" | "left" | "right";
}

interface TutorialProps {
  steps: TutorialStep[];
  /** Called when the tutorial finishes or is skipped */
  onComplete: () => void;
  /** Storage key for persisting progress (default: "tossd_tutorial_step") */
  storageKey?: string;
}

const STORAGE_KEY = "tossd_tutorial_step";

function getTargetRect(selector?: string): DOMRect | null {
  if (!selector) return null;
  const el = document.querySelector(selector);
  return el ? el.getBoundingClientRect() : null;
}

function tooltipPosition(
  rect: DOMRect | null,
  placement: TutorialStep["placement"] = "bottom"
): React.CSSProperties {
  if (!rect) {
    return { top: "50%", left: "50%", transform: "translate(-50%, -50%)" };
  }
  const gap = 12;
  switch (placement) {
    case "top":
      return { bottom: window.innerHeight - rect.top + gap, left: rect.left };
    case "left":
      return { top: rect.top, right: window.innerWidth - rect.left + gap };
    case "right":
      return { top: rect.top, left: rect.right + gap };
    case "bottom":
    default:
      return { top: rect.bottom + gap, left: rect.left };
  }
}

export function Tutorial({
  steps,
  onComplete,
  storageKey = STORAGE_KEY,
}: TutorialProps) {
  const [stepIndex, setStepIndex] = useState<number>(() => {
    try {
      const saved = localStorage.getItem(storageKey);
      return saved ? Math.min(parseInt(saved, 10), steps.length - 1) : 0;
    } catch {
      return 0;
    }
  });

  const [targetRect, setTargetRect] = useState<DOMRect | null>(null);
  const rafRef = useRef<number>(0);

  const step = steps[stepIndex];

  // Track target element position (handles scroll/resize)
  useEffect(() => {
    function update() {
      setTargetRect(getTargetRect(step?.target));
      rafRef.current = requestAnimationFrame(update);
    }
    rafRef.current = requestAnimationFrame(update);
    return () => cancelAnimationFrame(rafRef.current);
  }, [step?.target]);

  // Persist progress
  useEffect(() => {
    try {
      localStorage.setItem(storageKey, String(stepIndex));
    } catch {}
  }, [stepIndex, storageKey]);

  const advance = useCallback(() => {
    if (stepIndex < steps.length - 1) {
      setStepIndex((i) => i + 1);
    } else {
      finish();
    }
  }, [stepIndex, steps.length]);

  const finish = useCallback(() => {
    try {
      localStorage.removeItem(storageKey);
    } catch {}
    onComplete();
  }, [onComplete, storageKey]);

  if (!step) return null;

  const highlightStyle: React.CSSProperties | undefined = targetRect
    ? {
        top: targetRect.top - 6,
        left: targetRect.left - 6,
        width: targetRect.width + 12,
        height: targetRect.height + 12,
      }
    : undefined;

  return (
    <div className={styles.overlay} role="dialog" aria-modal="true" aria-label="Tutorial">
      {/* Backdrop only when no specific target */}
      {!targetRect && <div className={styles.backdrop} onClick={finish} />}

      {/* Highlight ring around target */}
      {highlightStyle && <div className={styles.highlight} style={highlightStyle} aria-hidden="true" />}

      {/* Tooltip */}
      <div
        className={styles.tooltip}
        style={tooltipPosition(targetRect, step.placement)}
      >
        <p className={styles.stepLabel}>
          Step {stepIndex + 1} of {steps.length}
        </p>
        <h3 className={styles.title}>{step.title}</h3>
        <p className={styles.description}>{step.description}</p>

        <div className={styles.actions}>
          <div className={styles.dots} aria-hidden="true">
            {steps.map((_, i) => (
              <span
                key={i}
                className={`${styles.dot} ${i === stepIndex ? styles.dotActive : ""}`}
              />
            ))}
          </div>

          <button className={styles.btnSkip} onClick={finish} aria-label="Skip tutorial">
            Skip
          </button>

          <button className={styles.btnNext} onClick={advance} autoFocus>
            {stepIndex < steps.length - 1 ? "Next" : "Done"}
          </button>
        </div>
      </div>
    </div>
  );
}

// Default steps for Tossd first-time users
export const DEFAULT_TUTORIAL_STEPS: TutorialStep[] = [
  {
    title: "Welcome to Tossd",
    description: "A provably fair coinflip game on Stellar. Let's walk you through your first game.",
  },
  {
    title: "Connect your wallet",
    description: "Click 'Connect Wallet' to link your Freighter, Albedo, or xBull wallet.",
    target: "[data-tutorial='connect-wallet']",
    placement: "bottom",
  },
  {
    title: "Set your wager",
    description: "Enter the amount of XLM you want to wager. The minimum is set by the contract.",
    target: "[data-tutorial='wager-input']",
    placement: "bottom",
  },
  {
    title: "Choose heads or tails",
    description: "Pick your side. The outcome is determined by a commit-reveal scheme — no house manipulation.",
    target: "[data-tutorial='side-selector']",
    placement: "right",
  },
  {
    title: "Commit your move",
    description: "Submit your commitment on-chain. This locks in your choice before the result is revealed.",
    target: "[data-tutorial='commit-btn']",
    placement: "top",
  },
  {
    title: "Reveal and collect",
    description: "After the reveal, you can cash out or keep playing to build your streak multiplier.",
    target: "[data-tutorial='reveal-btn']",
    placement: "top",
  },
];
