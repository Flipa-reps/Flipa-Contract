import React from "react";
import styles from "./CTABand.module.css";

export function CTABand() {
  return (
    <section className={styles.band} aria-labelledby="cta-heading">
      <div className={styles.inner}>
        <div className={styles.copy}>
          <h2 id="cta-heading" className={styles.heading}>
            Play with Proof, Not Hype.
          </h2>
          <p className={styles.subtext}>
            Launch Flipa to play, or audit the contract before your first flip.
          </p>
        </div>
        <div className={styles.ctaGroup}>
          <a href="#launch" className={styles.btnPrimary}>
            Launch Flipa
          </a>
          <a
            href="#audit"
            className={styles.btnSecondary}
            rel="noopener noreferrer"
          >
            Audit Contract
          </a>
        </div>
      </div>
    </section>
  );
}
