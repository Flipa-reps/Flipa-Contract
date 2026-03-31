import React, { useRef, useState, useEffect } from "react";
import { MobileMenu } from "./MobileMenu";
import styles from "./NavBar.module.css";

const NAV_LINKS = [
  { label: "How It Works", href: "#how-it-works" },
  { label: "Economics", href: "#economics" },
  { label: "Security", href: "#security" },
  { label: "Audit Contract", href: "https://github.com/Tossd-Org/Tossd" },
];

export interface NavBarProps {
  /** Called when the wallet button is clicked. Swap for real wallet logic. */
  onConnectWallet?: () => void;
  /** When true the wallet button shows a connected state. */
  walletConnected?: boolean;
}

export function NavBar({ onConnectWallet, walletConnected = false }: NavBarProps) {
  const [menuOpen, setMenuOpen] = useState(false);
  const [scrolled, setScrolled] = useState(false);
  const triggerRef = useRef<HTMLButtonElement>(null);

  // Elevation change on scroll
  useEffect(() => {
    const onScroll = () => setScrolled(window.scrollY > 4);
    window.addEventListener("scroll", onScroll, { passive: true });
    return () => window.removeEventListener("scroll", onScroll);
  }, []);

  const open = () => setMenuOpen(true);
  const close = () => setMenuOpen(false);

  const walletLabel = walletConnected ? "Connected" : "Connect Wallet";

  return (
    <header
      className={[styles.header, scrolled ? styles.scrolled : ""].filter(Boolean).join(" ")}
      role="banner"
      data-scrolled={scrolled}
    >
      <a href="/" className={styles.logo} aria-label="Tossd home">
        Tossd
      </a>

      {/* Desktop nav */}
      <nav className={styles.desktopNav} aria-label="Primary navigation">
        {NAV_LINKS.map((l) => (
          <a key={l.href} href={l.href} className={styles.navLink}>
            {l.label}
          </a>
        ))}
        <button
          className={[styles.walletBtn, walletConnected ? styles.walletConnected : ""]
            .filter(Boolean)
            .join(" ")}
          onClick={onConnectWallet}
          aria-label={walletLabel}
          data-wallet-connected={walletConnected}
        >
          {walletLabel}
        </button>
        <a href="#play" className={styles.ctaBtn}>Launch App</a>
      </nav>

      {/* Mobile hamburger */}
      <button
        ref={triggerRef}
        className={styles.hamburger}
        aria-label="Open navigation menu"
        aria-expanded={menuOpen}
        aria-controls="mobile-menu"
        onClick={open}
      >
        <span className={styles.bar} />
        <span className={styles.bar} />
        <span className={styles.bar} />
      </button>

      {/* Mobile menu */}
      <MobileMenu open={menuOpen} onClose={close} triggerRef={triggerRef}>
        <div id="mobile-menu">
          <button
            className={styles.closeBtn}
            aria-label="Close navigation menu"
            onClick={close}
          >
            ✕
          </button>
          <nav aria-label="Mobile navigation">
            <ul className={styles.mobileNavList} role="list">
              {NAV_LINKS.map((l) => (
                <li key={l.href}>
                  <a href={l.href} className={styles.mobileNavLink} onClick={close}>
                    {l.label}
                  </a>
                </li>
              ))}
            </ul>
          </nav>
          <button
            className={[styles.mobileWalletBtn, walletConnected ? styles.walletConnected : ""]
              .filter(Boolean)
              .join(" ")}
            onClick={() => { onConnectWallet?.(); close(); }}
            aria-label={walletLabel}
          >
            {walletLabel}
          </button>
          <a href="#play" className={styles.mobileCta} onClick={close}>
            Launch App
          </a>
        </div>
      </MobileMenu>
    </header>
  );
}
