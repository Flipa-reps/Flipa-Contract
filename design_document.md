# Tossd Cash-Out & Continue Decision Screens
Resolves Issue #86

## Overview
This document details the UX/UI design specifications for the high-stakes decision interface presented immediately after a player wins a round. The interface forces a calculated choice: securely cash out current winnings or risk them for a larger payout via a streak multiplier.

## 1. Layout & Visual Hierarchy (Wireframe)
The decision interface is designed as a **full-screen overlay** (or a very prominent high-stakes modal on desktop) to ensure the player's complete focus is on the choice.

### Wireframe Structure
```text
+-------------------------------------------------------------+
|                                                             |
|   [Streak Tracker]  🔥  3 / 5 Wins to Milestone            |
|                                                             |
|   +-----------------------------------------------------+   |
|   |                  YOU WON ROUND 3!                   |   |
|   |                                                     |   |
|   |         Current Winnings:    $10.00                 |   |
|   |                                                     |   |
|   |         If you Continue:                            |   |
|   |         - Win Next:      $25.00  (2.5x Multiplier)  |   |
|   |         - Lose Next:     -$10.00 (Total Payout $0)  |   |
|   +-----------------------------------------------------+   |
|                                                             |
|                                                             |
|   [================= CASH OUT ACTIONS =================]    |
|                                                             |
|      +-----------------------+   +-----------------------+  |
|      |     SECURE $10.00     |   |    RISK FOR $25.00    |  |
|      |  (Guaranteed Payout)  |   |   (Continue Streak)   |  |
|      +-----------------------+   +-----------------------+  |
|                                                             |
+-------------------------------------------------------------+
```

### Visual Weight & Differentiation
- **Cash Out (Safe/Secure):** Positioned clearly, using a solid, calming, and high-contrast color (Reward Palette). The button border and shadow should feel grounded and heavy.
- **Continue (Risk/Streak):** Uses energetic, vibrant, and bold colors (Risk Palette) with a slight pulse or glowing micro-animation to emphasize the potential multiplier and attract attention to the high reward.

## 2. Payout Math & Streak Context
Transparency in the risk is paramount to fintech gamification.
- **Payout Calculation Display:** The math must be explicit.
  - **[Current Winnings]:** Bold and central.
  - **[Potential Winnings if Next Round is Won]:** Highlighted in green/accent color, showing the exact dollar amount and the multiplier effect.
  - **[Loss Amount if Next Round is Lost]:** Clearly stated in a warning color (red/orange) to emphasize transparency of the risk (e.g., "Total Payout: $0.00").
- **Streak Tracker:** Located at the top of the interface, providing context (e.g., "3/5 wins"). Progress bars or lit-up icons (🔥) should be used to visually represent how close the player is to a major milestone.

## 3. States & Feedback

### 3.1 Blockchain Transaction Feedback States (Cash Out)
Because "Cash Out" involves a Stellar/Soroban smart contract transaction rather than a simple database update, the interface must handle asynchronous blockchain states effectively:

#### 1. The "Signing" State (Waiting for Wallet)
- **Trigger:** Player taps "Secure [Amount]".
- **UI State:** The decision screen dims slightly. A high-priority, non-blocking loader appears reading "**Waiting for Wallet Approval...**"
- **Purpose:** Prevents the player from thinking the application has frozen while their external wallet (e.g., Freighter, Lobstr) prompts them for a signature. All underlying decision buttons are disabled to prevent double-submission.

#### 2. The "Submitting" State (Network Verification)
- **Trigger:** Player signs the transaction in their wallet.
- **UI State:** The loader text transitions to "**Verifying on Stellar...**" accompanied by a pulsing network icon or a circular progress ring.
- **Purpose:** Sets the expectation that the network is actively processing the transaction (~3-5 seconds on Stellar).

#### 3. The "Revert/Failure" State
- **Trigger:** Transaction fails due to network congestion, expired TTL, or the user rejecting the signature in their wallet.
- **UI State:** The loader turns red and displays a brief, clear error message (e.g., *"Transaction Failed. Please try again."*).
- **Recovery:** The error toast dismisses after 3 seconds, and the UI safely returns the player exactly to the original "Cash Out vs Continue" decision screen. Winnings remain on the table and are not lost due to network errors.

#### 4. Final Success State
- **Trigger:** Stellar network explicitly confirms the transaction.
- **Feedback:** A satisfying cashier/coin chink sound plays (if audio is enabled).
- **Animation:** The screen transitions to a "Funds Secured" success modal. A confetti micro-animation or a green checkmark pulses. The player's balance updates explicitly in real-time.

### 3.2 Transition State (Choosing to Continue)
- **Trigger:** Player taps "Risk for [Amount]".
- **Feedback:** The interface creates immediate tension. The "Continue" button flares or emits a short burst of particles.
- **Animation:** The screen zooms in slightly or uses a "whoosh" transition to immediately thrust the player into the next game round, maintaining the adrenaline of the risk.

## 4. Copy Strategy (Microcopy)
The language must be fiercely neutral yet mathematically clear, balancing the psychological pull of gamification with the clarity required for financial decisions.
- **Primary Action (Safe):** `Secure $10.00` (Subtext: *Guaranteed Payout* or *Take Winnings*)
- **Secondary Action (Risk):** `Risk for $25.00` (Subtext: *Continue Streak* or *Play Round 4*)
- **Avoid:** Manipulative "dark patterns" like "I'm a coward, take $10" or "No thanks, I hate money."

## 5. Visual Design System (Color Palette)

### "Reward" (Cash Out / Safe) Palette
- **Primary Background:** Solid Green (`#10B981` / Emerald)
- **Text/Iconography:** High-contrast White (`#FFFFFF`) or Deep Slate (`#064E3B`)
- **Psychology:** Signifies security, completion, and financial realization.

### "Risk" (Continue / Streak) Palette
- **Primary Background:** Vibrant Orange/Amber (`#F59E0B` or `#EA580C`)
- **Accent/Glow:** Neon Pink or Electric Purple (`#D946EF`) for the multiplier.
- **Text:** High-contrast White (`#FFFFFF`) or Dark Charcoal (`#1E293B`)
- **Psychology:** Signifies high energy, warning, excitement, and potential.

## 6. Accessibility & Usability Notes
- **Touch Targets:** Buttons must have a minimum height of `56px` to prevent "fat-finger" accidental clicks on mobile devices, especially during adrenaline-filled moments.
- **Spacing:** Significant padding (at least `24px`) between the "Cash Out" and "Continue" buttons to ensure a distinct, deliberate choice.
- **Contrast Ratios:** All text within the buttons and payout math must meet the WCAG AAA standard (at least 7:1) for outdoor visibility, ensuring players can make financial decisions clearly even in direct sunlight.

## 7. Interaction Notes for Frontend Developers
- **Entry Animation:** The modal should enter with a slight scale-up (`0.95` to `1.0`) and fade-in (`opacity: 0` to `1`) over `150ms` using an ease-out timing function.
- **Button Hover/Tap:** Implement a CSS `:active` state that physically presses the button down by translating the Y-axis (`transform: translateY(2px)`) to provide tactile feedback.
- **Multiplier Pulse:** Add a subtle `@keyframes` pulse animation to the Potential Winnings text or the "Continue" button border to draw the eye, but ensure it stops after 3 iterations to pass accessibility guidelines regarding flashing content.
