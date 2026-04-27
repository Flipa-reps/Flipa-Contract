# Tossd Implementation Priorities

---

## P0 — Core Game Loop (ship first)

These are required for any playable state. Nothing else ships until these are done.

| # | Component | File | Notes |
|---|---|---|---|
| 1 | Design tokens | `tokens/tossd.tokens.css` | Must be imported before any component renders |
| 2 | Button | `Button.tsx` | All 3 variants + loading state + icon slots |
| 3 | LoadingSpinner | `LoadingSpinner.tsx` | Used inside Button and standalone |
| 4 | Modal | `Modal.tsx` | Base for WalletModal and CashOutModal |
| 5 | NavBar | `NavBar.tsx` | First visible element; wallet connect entry point |
| 6 | WalletModal | `WalletModal.tsx` | Freighter + Albedo connection required to play |
| 7 | WagerInput | `WagerInput.tsx` | XLM input with 7-decimal validation |
| 8 | SideSelector | `SideSelector.tsx` | Heads/tails radio group |
| 9 | CommitRevealFlow | `CommitRevealFlow.tsx` | Core game interaction |
| 10 | CoinFlip | `CoinFlip.tsx` | Visual outcome reveal |
| 11 | GameStateCard | `GameStateCard.tsx` | Live game state display |
| 12 | GameResult | `GameResult.tsx` | Win/loss outcome screen |
| 13 | CashOutModal | `CashOutModal.tsx` | Streak decision UI |

---

## P1 — Complete the Experience

| # | Component | File | Notes |
|---|---|---|---|
| 14 | HeroSection | `HeroSection.tsx` | Landing page entry point |
| 15 | ProofCard | `ProofCard.tsx` | Used inside HeroSection |
| 16 | TrustStrip | `TrustStrip.tsx` | Used inside HeroSection |
| 17 | EconomicsPanel | `EconomicsPanel.tsx` | Fee transparency section |
| 18 | SecuritySection | `SecuritySection.tsx` | Trust signals section |
| 19 | FairnessTimeline | `FairnessTimeline.tsx` | How-it-works section |
| 20 | CTABand | `CTABand.tsx` | Conversion section |
| 21 | Footer | `Footer.tsx` | Navigation + legal |
| 22 | ToastProvider | `ToastProvider.tsx` | System-wide notifications |
| 23 | ErrorBoundary | `ErrorBoundary.tsx` | Crash recovery wrapper |
| 24 | MobileMenu | `MobileMenu.tsx` | Mobile nav drawer |

---

## P2 — Data Display

| # | Component | File | Notes |
|---|---|---|---|
| 25 | StatsDashboard | `StatsDashboard.tsx` | Contract stats with 15s polling |
| 26 | TransactionHistory | `TransactionHistory.tsx` | Game record list |
| 27 | OutcomeChip | `OutcomeChip.tsx` | Used in TransactionHistory |
| 28 | MultiplierProgression | `MultiplierProgression.tsx` | Streak multiplier visualization |
| 29 | GameFlowSteps | `GameFlowSteps.tsx` | 4-step how-it-works list |
| 30 | StepCard | `StepCard.tsx` | Used in GameFlowSteps |
| 31 | VerificationPanel | `VerificationPanel.tsx` | Hash verification guide |

---

## P3 — Operator / Admin Tools

| # | Component | File | Notes |
|---|---|---|---|
| 32 | RbacDashboard | `RbacDashboard.tsx` | Role management UI (SuperAdmin only) |
| 33 | RoleBadge | `RoleBadge.tsx` | Role indicator chip |
| 34 | SecurityDashboard | `SecurityDashboard.tsx` | Audit log + anomaly detection |

---

## P4 — Layout Utilities

| # | Component | File | Notes |
|---|---|---|---|
| 35 | Grid / GridItem | `Grid.tsx` | Flexible grid system |

---

## Hooks (wire after P0 components)

| Hook | File | Connects to |
|---|---|---|
| `useStartGame` | `hooks/useStartGame.ts` | CommitRevealFlow submit |
| `useReveal` | `hooks/useReveal.ts` | CommitRevealFlow reveal step |
| `useCashOut` | `hooks/useCashOut.ts` | CashOutModal + GameResult |
| `useContinue` | `hooks/useContinue.ts` | GameResult continue action |
| `useRbac` | `hooks/useRbac.ts` | RbacDashboard + PermissionGuard |
| `useSecurityLog` | `hooks/useSecurityLog.ts` | SecurityDashboard |
| `useHsm` | `hooks/useHsm.ts` | CommitRevealFlow secret generation |
| `useMultisig` | `hooks/useMultisig.ts` | Admin proposal flows |

---

## Acceptance Checklist (per component)

Before marking any component done:

- [ ] All token references — no raw hex or px values
- [ ] 44px min touch target on all interactive elements
- [ ] `:focus-visible` ring on all interactive elements
- [ ] `aria-*` attributes per spec in `04-accessibility.md`
- [ ] Reduced motion: no hardcoded animation durations
- [ ] `npm run test:a11y` passes with no new violations
- [ ] Renders correctly at 320px, 768px, 1200px viewport widths
- [ ] Loading and error states implemented
- [ ] PR description includes `Closes #91`

