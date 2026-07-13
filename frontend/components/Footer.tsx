import React from "react";
import styles from "./Footer.module.css";

const NAV_LINKS = [
  { label: "How It Works", href: "#how-it-works" },
  { label: "Economics", href: "#economics" },
  { label: "Fairness", href: "#fairness" },
  { label: "Security", href: "#security" },
  { label: "Audit Contract", href: "https://github.com/Flipa-Org/Flipa", external: true },
];

const SOCIAL_LINKS = [
  {
    label: "GitHub",
    href: "https://github.com/Flipa-Org/Flipa",
    icon: (
      <svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
        <path d="M12 2C6.477 2 2 6.477 2 12c0 4.418 2.865 8.166 6.839 9.489.5.092.682-.217.682-.482 0-.237-.009-.868-.013-1.703-2.782.604-3.369-1.342-3.369-1.342-.454-1.154-1.11-1.462-1.11-1.462-.908-.62.069-.608.069-.608 1.003.07 1.531 1.03 1.531 1.03.892 1.529 2.341 1.087 2.91.832.092-.647.35-1.088.636-1.338-2.22-.253-4.555-1.11-4.555-4.943 0-1.091.39-1.984 1.029-2.683-.103-.253-.446-1.27.098-2.647 0 0 .84-.269 2.75 1.025A9.578 9.578 0 0 1 12 6.836a9.59 9.59 0 0 1 2.504.337c1.909-1.294 2.747-1.025 2.747-1.025.546 1.377.202 2.394.1 2.647.64.699 1.028 1.592 1.028 2.683 0 3.842-2.339 4.687-4.566 4.935.359.309.678.919.678 1.852 0 1.336-.012 2.415-.012 2.743 0 .267.18.578.688.48C19.138 20.163 22 16.418 22 12c0-5.523-4.477-10-10-10z" />
      </svg>
    ),
  },
  {
    label: "Twitter / X",
    href: "https://twitter.com/FlipaOrg",
    icon: (
      <svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
        <path d="M18.244 2.25h3.308l-7.227 8.26 8.502 11.24H16.17l-5.214-6.817L4.99 21.75H1.68l7.73-8.835L1.254 2.25H8.08l4.713 6.231zm-1.161 17.52h1.833L7.084 4.126H5.117z" />
      </svg>
    ),
  },
  {
    label: "Discord",
    href: "https://discord.gg/flipa",
    icon: (
      <svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
        <path d="M20.317 4.37a19.791 19.791 0 0 0-4.885-1.515.074.074 0 0 0-.079.037c-.21.375-.444.864-.608 1.25a18.27 18.27 0 0 0-5.487 0 12.64 12.64 0 0 0-.617-1.25.077.077 0 0 0-.079-.037A19.736 19.736 0 0 0 3.677 4.37a.07.07 0 0 0-.032.027C.533 9.046-.32 13.58.099 18.057a.082.082 0 0 0 .031.057 19.9 19.9 0 0 0 5.993 3.03.078.078 0 0 0 .084-.028c.462-.63.874-1.295 1.226-1.994a.076.076 0 0 0-.041-.106 13.107 13.107 0 0 1-1.872-.892.077.077 0 0 1-.008-.128 10.2 10.2 0 0 0 .372-.292.074.074 0 0 1 .077-.01c3.928 1.793 8.18 1.793 12.062 0a.074.074 0 0 1 .078.01c.12.098.246.198.373.292a.077.077 0 0 1-.006.127 12.299 12.299 0 0 1-1.873.892.077.077 0 0 0-.041.107c.36.698.772 1.362 1.225 1.993a.076.076 0 0 0 .084.028 19.839 19.839 0 0 0 6.002-3.03.077.077 0 0 0 .032-.054c.5-5.177-.838-9.674-3.549-13.66a.061.061 0 0 0-.031-.03zM8.02 15.33c-1.183 0-2.157-1.085-2.157-2.419 0-1.333.956-2.419 2.157-2.419 1.21 0 2.176 1.096 2.157 2.42 0 1.333-.956 2.418-2.157 2.418zm7.975 0c-1.183 0-2.157-1.085-2.157-2.419 0-1.333.955-2.419 2.157-2.419 1.21 0 2.176 1.096 2.157 2.42 0 1.333-.946 2.418-2.157 2.418z" />
      </svg>
    ),
  },
];

const LEGAL_LINKS = [
  { label: "Terms of Service", href: "/terms" },
  { label: "Privacy Policy", href: "/privacy" },
];

export function Footer() {
  return (
    <footer className={styles.footer} aria-label="Site footer">
      <div className={styles.inner}>
        <div className={styles.top}>
          {/* Brand column */}
          <div className={styles.brand}>
            <a href="/" className={styles.logo} aria-label="Flipa home">
              Flipa
            </a>
            <p className={styles.tagline}>
              Trustless coinflips on Soroban. Every outcome provably fair, every
              settlement on-chain.
            </p>
            <div className={styles.social} aria-label="Social media links">
              {SOCIAL_LINKS.map(({ label, href, icon }) => (
                <a
                  key={label}
                  href={href}
                  className={styles.socialLink}
                  aria-label={label}
                  target="_blank"
                  rel="noopener noreferrer"
                >
                  {icon}
                </a>
              ))}
            </div>
          </div>

          {/* Navigation column */}
          <nav className={styles.nav} aria-label="Footer navigation">
            <p className={styles.colHeading}>Navigation</p>
            <ul className={styles.navList}>
              {NAV_LINKS.map(({ label, href, external }) => (
                <li key={label}>
                  <a
                    href={href}
                    className={styles.navLink}
                    {...(external
                      ? { target: "_blank", rel: "noopener noreferrer" }
                      : {})}
                  >
                    {label}
                  </a>
                </li>
              ))}
            </ul>
          </nav>

          {/* Resources column */}
          <div className={styles.resources}>
            <p className={styles.colHeading}>Resources</p>
            <ul className={styles.navList}>
              <li>
                <a
                  href="https://github.com/Flipa-Org/Flipa"
                  className={styles.navLink}
                  target="_blank"
                  rel="noopener noreferrer"
                >
                  Open Source Contract
                </a>
              </li>
              <li>
                <a href="#how-it-works" className={styles.navLink}>
                  Documentation
                </a>
              </li>
              <li>
                <a href="#fairness" className={styles.navLink}>
                  Fairness Proof
                </a>
              </li>
            </ul>
          </div>
        </div>

        {/* Bottom bar */}
        <div className={styles.bottom}>
          <p className={styles.disclaimer}>
            Flipa is a provably fair on-chain game. Participation may be
            restricted in certain jurisdictions. This is not financial advice.
            Play responsibly — only wager what you can afford to lose.
          </p>
          <div className={styles.bottomRight}>
            <p className={styles.copyright}>
              &copy; {new Date().getFullYear()} Flipa. All rights reserved.
            </p>
            <div className={styles.legalLinks}>
              {LEGAL_LINKS.map(({ label, href }) => (
                <a key={label} href={href} className={styles.legalLink}>
                  {label}
                </a>
              ))}
            </div>
          </div>
        </div>
      </div>
    </footer>
  );
}
