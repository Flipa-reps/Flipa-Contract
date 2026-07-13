import React, { useState } from "react";
import { CTABand } from "../components/CTABand";
import { CommitRevealFlow } from "../components/CommitRevealFlow";
import { EconomicsPanel } from "../components/EconomicsPanel";
import { Footer } from "../components/Footer";
import { HeroSection } from "../components/HeroSection";
import { NavBar } from "../components/NavBar";
import { SecuritySection } from "../components/SecuritySection";
import { WalletModal, WalletId } from "../components/WalletModal";

const FAKE_ADDRESSES: Record<WalletId, string> = {
  freighter: "GCFREIGHTER2P3X7Q4T2W6J9Y8Z1A4V6M8N2Q5R7S3T5W8X",
  albedo: "GALBEDO4Y6N8Q2T5W7Z9A1C3E5G7J9L2N4P6R8T0V2X4",
  xbull: "GXBULL3V5X7Z9A1C3E5G7J9L2N4P6R8T0V2X4Z6B8D0",
  rabet: "GRABET5W7Y9A1C3E5G7J9L2N4P6R8T0V2X4Z6B8D0F2H4",
};

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

export function App() {
  const [walletOpen, setWalletOpen] = useState(false);
  const [walletConnected, setWalletConnected] = useState(false);
  const [walletAddress, setWalletAddress] = useState("");
  const [walletLabel, setWalletLabel] = useState("No wallet connected");
  const [activity, setActivity] = useState("Ready to play.");

  const handleConnectWallet = () => {
    setWalletOpen(true);
  };

  const connectWallet = async (walletId: WalletId) => {
    setActivity(`Connecting ${walletId}...`);
    await sleep(450);
    return FAKE_ADDRESSES[walletId];
  };

  const handleWalletConnect = (address: string, walletId: WalletId) => {
    setWalletConnected(true);
    setWalletAddress(address);
    setWalletLabel(walletId);
    setActivity(`${walletId} connected locally.`);
    setWalletOpen(false);
  };

  const handleCommit = async () => {
    setActivity("Commit submitted locally.");
    await sleep(350);
  };

  const handleReveal = async () => {
    setActivity("Reveal verified locally.");
    await sleep(350);
  };

  return (
    <div className="appShell">
      <div className="ambient ambientOne" aria-hidden="true" />
      <div className="ambient ambientTwo" aria-hidden="true" />

      <NavBar onConnectWallet={handleConnectWallet} walletConnected={walletConnected} />

      <main className="mainContent">
        <HeroSection />

        <section id="play" className="sectionFrame">
          <div className="sectionHeading">
            <p className="eyebrow">Interactive demo</p>
            <h2>Launch the game flow locally</h2>
            <p>
              This app now starts with Vite. Wallet and commit-reveal actions run
              against local demo handlers until contract wiring is finished.
            </p>
          </div>

          <div className="playGrid">
            <div className="playPanel">
              <CommitRevealFlow onCommit={handleCommit} onReveal={handleReveal} />
            </div>

            <aside className="statusPanel" aria-label="Live status">
              <p className="statusLabel">Session status</p>
              <p className="statusValue">{activity}</p>
              <dl className="statusList">
                <div>
                  <dt>Wallet</dt>
                  <dd>{walletLabel}</dd>
                </div>
                <div>
                  <dt>Address</dt>
                  <dd>{walletAddress || "Waiting for connect"}</dd>
                </div>
              </dl>
            </aside>
          </div>
        </section>

        <section id="how-it-works" className="sectionFrame">
          <div className="sectionHeading">
            <p className="eyebrow">How it works</p>
            <h2>Commit, reveal, verify, settle</h2>
            <p>
              The page is assembled from the existing Flipa UI components and now
              boots from a regular app entry.
            </p>
          </div>
        </section>

        <EconomicsPanel />
        <SecuritySection />
        <CTABand />
      </main>

      <Footer />

      <WalletModal
        open={walletOpen}
        onClose={() => setWalletOpen(false)}
        onConnect={handleWalletConnect}
        connectWallet={connectWallet}
      />
    </div>
  );
}
