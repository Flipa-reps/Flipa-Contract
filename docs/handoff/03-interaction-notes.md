# Tossd Interaction Notes

---

## Motion System

All transitions use tokens from `frontend/tokens/tossd.tokens.css`. Never hardcode durations.

| Token | Value | When to use |
|---|---|---|
| `--motion-fast` | `140ms` | Hover fills, focus rings, color shifts |
| `--motion-base` | `220ms` | Card entry, modal open/close, step transitions |
| `--motion-slow` | `420ms` | Game result entry, coin flip settle |
| `--ease-standard` | `cubic-bezier(0.2, 0.8, 0.2, 1)` | All transitions |

**Reduced motion:** `prefers-reduced-motion: reduce` sets all `--motion-*` to `0ms`. No animation should bypass this.

---

## Hover

- Background or color shift only — never move or scale on hover alone
- Duration: `var(--motion-fast)`
- Easing: `var(--ease-standard)`

---

## Focus

- Always: `outline: 2px solid var(--color-focus-ring); outline-offset: 2px`
- Use `:focus-visible` not `:focus` — avoids showing outlines on mouse click
- Never remove or suppress focus outlines
- Border radius on focus ring should match the element's own `border-radius`

---

## Loading States

When an async action is in-flight:
1. Button shows inline `LoadingSpinner` (16px)
2. `aria-busy="true"` set on the button
3. `cursor: wait`
4. `pointer-events: none` — prevents double-submit
5. Other related actions also disabled

---

## CommitRevealFlow State Machine

```
commit ──[submit]──► pending ──[auto-advance]──► reveal ──[submit]──► verified
  ▲                                                                        │
  └──────────────────────────[reset]──────────────────────────────────────┘
                                                              └──► error ──[reset]──► commit
```

**Commit step:**
- Player types or generates a random secret (32 bytes via `crypto.getRandomValues`)
- SHA-256 hash computed client-side via Web Crypto API
- Hash displayed in a read-only mono box before submit
- Submit sends commitment hash on-chain

**Pending step:**
- Spinner shown while tx confirms
- Auto-advances to reveal once confirmed

**Reveal step:**
- Player re-enters their secret (or it's pre-filled from state)
- Submit sends secret on-chain; contract XORs with house secret to determine outcome

**Verified / Error:**
- Verified: success card with green border
- Error: error card with red border + reset option

---

## Card Entry Animation

Used on CommitRevealFlow step cards:
```css
@keyframes cardIn {
  from { opacity: 0; transform: translateY(8px); }
  to   { opacity: 1; transform: none; }
}
/* duration: var(--motion-base) */
```

---

## Modal Open / Close

- Open: `scale(0.95) + opacity 0` → `scale(1) + opacity 1` · `var(--motion-base)`
- Close: reverse · 220ms fixed (exit animation completes before unmount)
- Backdrop: fade in/out simultaneously

---

## Coin Flip Animation

- State `flipping`: 3D `rotateY` spin · 1200ms
- State `revealed`: settle to `rotateY(0deg)` (heads) or `rotateY(180deg)` (tails)
- `onAnimationEnd` fires after flip completes — use to trigger result display
- Reduced motion: skip spin, directly show result face with opacity fade

---

## Game Result Entry

```css
@keyframes resultEnter {
  from { opacity: 0; transform: translateY(12px) scale(0.97); }
  to   { opacity: 1; transform: none; }
}
/* duration: var(--motion-slow) = 420ms */
```

Win confetti: 60 canvas particles, brand palette (`#0F766E`, `#1D7A45`, `#DDF3F0`, `#B5681D`, `#171717`), gravity `0.18`, alpha fade `0.012/frame`.

---

## NavBar Scroll Elevation

- `window.scrollY > 4` → add `box-shadow: var(--shadow-card)`, remove bottom border
- Transition: `box-shadow var(--motion-fast) var(--ease-standard)`
- Implemented via JS scroll listener with `{ passive: true }`

---

## Mobile Menu

- Triggered by hamburger button (44×44px)
- Slides in from side
- Focus trap active while open
- ESC or backdrop click closes
- Focus returns to hamburger button on close

---

## Wallet Connection Flow

```
Click "Connect Wallet"
  → WalletModal opens (focus trapped)
  → Player selects wallet
  → Connecting state: spinner on selected item, all others disabled
  → Success: connected state shown, onConnect(address, walletId) fires
  → Error: role="alert" banner shown, player can retry
  → Close: focus returns to NavBar wallet button
```

---

## Streak Decision (CashOutModal)

Shown after a win when player has an active streak:
- Displays current multiplier vs. next multiplier
- Shows calculated payout at current vs. potential next
- Risk messaging: "If you lose, your wager is forfeited"
- Cash Out (primary) · Continue (secondary)

---

## Background Ambient Effect

Two fixed blobs, `aria-hidden="true"`, `pointer-events: none`:
- `.ambientOne`: top-right · teal · `rgba(15, 118, 110, 0.18)` · `blur(72px)`
- `.ambientTwo`: left-center · dark · `rgba(17, 17, 17, 0.08)` · `blur(72px)`

These are purely decorative — no interaction, no reduced-motion concern.
