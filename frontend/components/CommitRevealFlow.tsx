import React, { useCallback, useReducer } from "react";
import { flushSync } from "react-dom";
import styles from "./CommitRevealFlow.module.css";

type Step = "commit" | "pending" | "reveal" | "verified" | "error";

type State = {
  step: Step;
  secret: string;
  commitment: string;
  revealSecret: string;
  loading: boolean;
  error: string | null;
};

type Action =
  | { type: "SET_SECRET"; secret: string; commitment: string }
  | { type: "COMMIT_START" }
  | { type: "COMMIT_SUCCESS" }
  | { type: "COMMIT_ERROR"; error: string }
  | { type: "ADVANCE_REVEAL" }
  | { type: "SET_REVEAL_SECRET"; value: string }
  | { type: "REVEAL_SUCCESS" }
  | { type: "REVEAL_ERROR"; error: string }
  | { type: "RESET" };

const INITIAL: State = {
  step: "commit",
  secret: "",
  commitment: "",
  revealSecret: "",
  loading: false,
  error: null,
};

function reducer(state: State, action: Action): State {
  switch (action.type) {
    case "SET_SECRET":
      return { ...state, secret: action.secret, commitment: action.commitment };
    case "COMMIT_START":
      return { ...state, loading: true, error: null };
    case "COMMIT_SUCCESS":
      return { ...state, loading: false, step: "pending" };
    case "COMMIT_ERROR":
      return { ...state, loading: false, step: "commit", error: action.error };
    case "ADVANCE_REVEAL":
      return { ...state, step: "reveal" };
    case "SET_REVEAL_SECRET":
      return { ...state, revealSecret: action.value };
    case "REVEAL_SUCCESS":
      return { ...state, loading: false, step: "verified" };
    case "REVEAL_ERROR":
      return { ...state, loading: false, step: "error", error: action.error };
    case "RESET":
      return INITIAL;
    default:
      return state;
  }
}

export interface CommitRevealFlowProps {
  /** Called with the player's secret when they submit the commit step. */
  onCommit: (secret: string, commitment: string) => Promise<void>;
  /** Called with the player's secret on the reveal step. */
  onReveal: (secret: string) => Promise<void>;
}

/** SHA-256 via Web Crypto — returns hex string. */
async function sha256Hex(input: string): Promise<string> {
  // Use Node.js crypto if available (test environment), otherwise Web Crypto
  if (typeof process !== "undefined" && process.versions?.node) {
    const { createHash } = await import("node:crypto");
    return createHash("sha256").update(input).digest("hex");
  }
  const encoded = new TextEncoder().encode(input);
  const buf = await crypto.subtle.digest("SHA-256", encoded);
  return Array.from(new Uint8Array(buf))
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

/** Generate a random 32-byte hex secret. */
function generateSecret(): string {
  const bytes = new Uint8Array(32);
  crypto.getRandomValues(bytes);
  return Array.from(bytes)
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

const STEP_LABELS: Record<Step, string> = {
  commit: "1. Commit",
  pending: "2. Waiting",
  reveal: "3. Reveal",
  verified: "✓ Verified",
  error: "Error",
};

export function CommitRevealFlow({ onCommit, onReveal }: CommitRevealFlowProps) {
  const [state, dispatch] = useReducer(reducer, INITIAL);
  const { step, secret, commitment, revealSecret, loading, error } = state;

  const handleGenerate = useCallback(async () => {
    const s = generateSecret();
    const hash = await sha256Hex(s);
    flushSync(() => dispatch({ type: "SET_SECRET", secret: s, commitment: hash }));
  }, []);

  const handleCommit = useCallback(() => {
    if (!secret || !commitment) return;
    dispatch({ type: "COMMIT_START" });
    onCommit(secret, commitment).then(
      () => React.startTransition(() => dispatch({ type: "COMMIT_SUCCESS" })),
      (e: unknown) => React.startTransition(() => dispatch({ type: "COMMIT_ERROR", error: e instanceof Error ? e.message : "Commit failed" }))
    );
  }, [secret, commitment, onCommit]);

  // Auto-advance from pending to reveal
  React.useEffect(() => {
    if (step !== "pending") return;
    const id = setTimeout(() => dispatch({ type: "ADVANCE_REVEAL" }), 50);
    return () => clearTimeout(id);
  }, [step]);

  const handleReveal = useCallback(() => {
    if (!revealSecret) return;
    dispatch({ type: "COMMIT_START" });
    onReveal(revealSecret).then(
      () => React.startTransition(() => dispatch({ type: "REVEAL_SUCCESS" })),
      (e: unknown) => React.startTransition(() => dispatch({ type: "REVEAL_ERROR", error: e instanceof Error ? e.message : "Reveal failed — commitment mismatch" }))
    );
  }, [revealSecret, onReveal]);

  const handleReset = () => dispatch({ type: "RESET" });

  return (
    <div className={styles.root} aria-label="Commit-reveal flow">
      {/* Step indicator */}
      <ol className={styles.stepIndicator} aria-label="Progress">
        {(["commit", "pending", "reveal", "verified"] as Step[]).map((s, i) => (
          <li
            key={s}
            className={[
              styles.stepDot,
              step === s ? styles.stepActive : "",
              ["verified", "reveal", "pending"].includes(step) && i < ["commit", "pending", "reveal", "verified"].indexOf(step)
                ? styles.stepDone
                : "",
            ]
              .filter(Boolean)
              .join(" ")}
            aria-current={step === s ? "step" : undefined}
          >
            <span className={styles.stepDotInner} />
            <span className={styles.stepDotLabel}>{STEP_LABELS[s]}</span>
          </li>
        ))}
      </ol>

      {/* Step: Commit */}
      {step === "commit" && (
        <div className={styles.card}>
          <h3 className={styles.cardTitle}>Generate Your Commitment</h3>
          <p className={styles.cardDesc}>
            A random secret is generated locally and hashed. Only the hash is sent
            on-chain — your secret stays private until the reveal step.
          </p>

          <div className={styles.field}>
            <label className={styles.label} htmlFor="secret-input">
              Your Secret
            </label>
            <div className={styles.inputRow}>
              <input
                id="secret-input"
                className={styles.monoInput}
                type="text"
                value={secret}
                onChange={(e) => dispatch({ type: "SET_SECRET", secret: e.target.value, commitment })}
                placeholder="Click Generate or paste your own"
                spellCheck={false}
                aria-describedby="secret-hint"
              />
              <button className={styles.btnOutline} onClick={handleGenerate} type="button">
                Generate
              </button>
            </div>
            <span id="secret-hint" className={styles.hint}>
              Never share this value before the reveal step.
            </span>
          </div>

          {commitment && (
            <div className={styles.field}>
              <label className={styles.label}>Commitment Hash (SHA-256)</label>
              <div className={styles.hashBox} aria-label="Commitment hash">
                <code className={styles.hash}>{commitment}</code>
              </div>
              <span className={styles.hint}>This hash is submitted on-chain.</span>
            </div>
          )}

          <button
            className={styles.btnPrimary}
            onClick={handleCommit}
            disabled={!secret || !commitment || loading}
            type="button"
          >
            {loading ? "Submitting…" : "Submit Commitment"}
          </button>

          {error && step === "commit" && (
            <div role="alert" className={styles.inlineError}>
              {error}
              <button className={styles.btnOutline} onClick={handleReset} type="button">
                Try Again
              </button>
            </div>
          )}
        </div>
      )}

      {/* Step: Pending */}
      {step === "pending" && (
        <div className={styles.card}>
          <div className={styles.spinner} aria-label="Waiting for confirmation" />
          <h3 className={styles.cardTitle}>Commitment Submitted</h3>
          <p className={styles.cardDesc}>
            Waiting for on-chain confirmation before proceeding to reveal…
          </p>
        </div>
      )}

      {/* Step: Reveal */}
      {step === "reveal" && (
        <div className={styles.card}>
          <h3 className={styles.cardTitle}>Reveal Your Secret</h3>
          <p className={styles.cardDesc}>
            Enter the secret you generated earlier. The contract will verify it
            matches the commitment hash and determine the outcome.
          </p>

          <div className={styles.field}>
            <label className={styles.label} htmlFor="reveal-input">
              Your Secret
            </label>
            <input
              id="reveal-input"
              className={styles.monoInput}
              type="text"
              value={revealSecret}
              onChange={(e) => dispatch({ type: "SET_REVEAL_SECRET", value: e.target.value })}
              placeholder="Paste your secret here"
              spellCheck={false}
            />
          </div>

          <button
            className={styles.btnPrimary}
            onClick={handleReveal}
            disabled={!revealSecret || loading}
            type="button"
          >
            {loading ? "Verifying…" : "Reveal & Settle"}
          </button>
        </div>
      )}

      {/* Step: Verified */}
      {step === "verified" && (
        <div className={[styles.card, styles.cardSuccess].join(" ")} role="status">
          <svg width="40" height="40" viewBox="0 0 40 40" fill="none" aria-hidden="true">
            <circle cx="20" cy="20" r="20" fill="var(--color-brand-accent-soft)" />
            <path d="M12 21l6 6 10-12" stroke="var(--color-state-success)" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round" />
          </svg>
          <h3 className={styles.cardTitle}>Commitment Verified</h3>
          <p className={styles.cardDesc}>
            Your secret matched the on-chain commitment. The outcome is settled.
          </p>
        </div>
      )}

      {/* Step: Error */}
      {step === "error" && (
        <div className={[styles.card, styles.cardError].join(" ")} role="alert">
          <h3 className={styles.cardTitle}>Verification Failed</h3>
          <p className={styles.cardDesc}>{error}</p>
          <button className={styles.btnOutline} onClick={handleReset} type="button">
            Try Again
          </button>
        </div>
      )}
    </div>
  );
}
