import React from "react";
import { ProofCard } from "./ProofCard";
import styles from "./HeroSection.module.css";

const TRUST_CHIPS = [
  "Provably Fair Commit-Reveal",
  "On-Chain Soroban Settlement",
  "2-5% Transparent Fee",
];

export function HeroSection() {
  return (
    <section className={styles.section} aria-label="Hero">
      <div className={styles.grid}>
        <div className={styles.copy}>
          <h1 className={styles.headline}>
            Trustless Coinflips.{" "}
            <br />
            Verifiable Outcomes.
          </h1>
          <p className={styles.body}>
            Flipa is an onchain coinflip game built on Soroban. Every outcome is
            auditable, every multiplier is explicit, and players choose when to
            secure profit or risk a streak.
          </p>
          <div className={styles.ctas}>
            <a href="#play" className={styles.ctaPrimary}>
              Launch Flipa
            </a>
            <a
              href="https://github.com/Flipa-Org/Flipa"
              className={styles.ctaSecondary}
              target="_blank"
              rel="noopener noreferrer"
            >
              Audit Contract
            </a>
          </div>
        </div>

        <ProofCard />
      </div>

      <div className={styles.trustStrip} aria-label="Trust indicators">
        {TRUST_CHIPS.map((chip) => (
          <span key={chip} className={styles.trustChip}>
            {chip}
          </span>
        ))}
      </div>
    </section>
  );
}
