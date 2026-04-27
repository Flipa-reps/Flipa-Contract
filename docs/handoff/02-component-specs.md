# Tossd Component Specs & Redlines
> All measurements reference tokens from `frontend/tokens/tossd.tokens.css`.
> Every component lives in `frontend/components/<Name>.tsx` + `<Name>.module.css`.

---

## Button

**Props**
| Prop | Type | Default | Notes |
|---|---|---|---|
| `variant` | `primary \| secondary \| danger` | `primary` | Controls color scheme |
| `size` | `sm \| md` | `md` | Both maintain 44px min-height |
| `loading` | `boolean` | `false` | Inline spinner + `aria-busy` |
| `iconStart` | `ReactNode` | — | SVG with `aria-hidden="true"` |
| `iconEnd` | `ReactNode` | — | SVG with `aria-hidden="true"` |
| `disabled` | `boolean` | — | Sets native `disabled` + `aria-disabled` |

**Redlines**
- Min height: `44px` (WCAG 2.5.5)
- Min width: `44px`
- Padding: `0 var(--space-4)` (md) · `0 var(--space-3)` (sm)
- Border radius: `var(--radius-md)`
- Font: `var(--font-body)` · `var(--font-size-sm)` · weight `600`
- Transition: `background, color, border-color, opacity` · `var(--motion-fast)` · `var(--ease-standard)`

**Variant states**
| Variant | Rest bg | Hover bg | Active bg | Border |
|---|---|---|---|---|
| `primary` | `#111111` | `#2a2a2a` | `#3d3d3d` | matches bg |
| `secondary` | transparent | `--color-bg-subtle` | `--color-border-default` | `--color-border-strong` |
| `danger` | `#A12A2A` | `#8a2323` | `#721d1d` | matches bg |

**Focus:** `outline: 2px solid var(--color-focus-ring); outline-offset: 2px`
**Disabled:** `opacity: 0.4; cursor: not-allowed; pointer-events: none`
**Loading:** spinner 16px inline, `cursor: wait`, `pointer-events: none`

---

## NavBar

**Behavior**
- `position: sticky; top: 0; z-index: 40`
- Scroll > 4px → adds `box-shadow: var(--shadow-card)`, removes bottom border
- Desktop: logo left · nav links center · wallet button + CTA right
- Mobile ≤768px: logo left · hamburger right · nav links hidden

**Wallet button states**
| State | Text color | Border | Background |
|---|---|---|---|
| Disconnected | `--color-fg-primary` | `--color-border-strong` | transparent |
| Connected | `--color-state-success` | `--color-state-success` | 8% success tint |

**Nav links:** `font-size: var(--font-size-sm)` · color `--color-fg-secondary` · hover `--color-brand-accent`
**Logo:** `font-family: var(--font-display)` · `font-size: var(--font-size-h3)` · weight `700`
**All buttons:** min-height `44px`

---

## Modal

**Props**
| Prop | Type | Notes |
|---|---|---|
| `open` | `boolean` | Controls visibility |
| `onClose` | `() => void` | ESC, backdrop click, close button |
| `titleId` | `string` | Required — `aria-labelledby` |
| `descriptionId` | `string` | Optional — `aria-describedby` |
| `closeOnOverlayClick` | `boolean` | Default `true` |
| `initialFocusRef` | `RefObject` | Override initial focus target |

**Behavior**
- Rendered via `createPortal` into `document.body`
- Focus trap: Tab/Shift+Tab cycles within panel only
- ESC closes; focus returns to trigger element on close
- Open animation: scale `0.95` + fade in · `var(--motion-base)`
- Close animation: reverse · 220ms

---

## CommitRevealFlow

**State machine:** `commit → pending → reveal → verified | error`

**Step indicator (horizontal dot + connector)**
| State | Dot fill | Label color |
|---|---|---|
| Inactive | `--color-border-default` | `--color-fg-muted` |
| Active | `--color-brand-accent` | `--color-brand-accent` · weight `600` |
| Done | `--color-state-success` | `--color-state-success` |

Connector line: `2px solid --color-border-default`

**Card**
- Background: `--color-bg-surface`
- Border: `1.5px solid --color-border-default`
- Border radius: `var(--radius-lg)`
- Shadow: `var(--shadow-soft)`
- Entry animation: `opacity 0 + translateY(8px)` → full · `var(--motion-base)`

**Commit step**
- Secret input: `--font-mono` · `--color-bg-subtle` bg · focus border `--color-focus-ring`
- "Generate" button: outline style, inline with input
- SHA-256 hash preview: `--color-state-info` text in subtle box

**Pending step**
- Spinner: 36px ring · `border-top-color: --color-brand-accent` · 0.8s linear
- Auto-advances to reveal

**Verified card:** bg `#F0FAF4` · border `--color-state-success` · centered
**Error card:** bg `#FDF2F2` · border `--color-state-danger`

---

## CoinFlip

**Props**
| Prop | Type | Default |
|---|---|---|
| `result` | `heads \| tails` | `heads` |
| `state` | `idle \| flipping \| revealed` | `idle` |
| `onAnimationEnd` | `() => void` | — |

**3D flip:** CSS `perspective` + `rotateY` · 1200ms
- Heads revealed: `rotateY(0deg)` (front face)
- Tails revealed: `rotateY(180deg)` (back face)

**Reduced motion:** 3D flip → instant opacity reveal
**A11y:** `aria-live="polite"` · `aria-atomic="true"` · `.srOnly` result text

---

## GameStateCard

**Phase badge colors**
| Phase | Visual |
|---|---|
| `idle` | Placeholder text only |
| `committed` | Neutral/muted badge |
| `won` | Success green badge |
| `lost` / `timed_out` | Danger red badge |

**Multiplier table**
| Streak | Multiplier |
|---|---|
| 0 | 1.9× |
| 1 | 3.5× |
| 2 | 6.0× |
| 3+ | 10.0× |

**Stats row:** `<dl>` — Wager (XLM) · Multiplier (accent color) · Streak count
**Actions by phase:**
- `committed` → Reveal (primary)
- `won` → Cash Out (primary) + Continue Streak (secondary)
- `lost` / `timed_out` → no actions

**Live region:** `aria-live="polite"` · `aria-atomic="true"` on card root

---

## GameResult

**Win state**
- Top border: `3px solid --color-state-success`
- Icon: 48px checkmark SVG in `--color-brand-accent-soft` circle
- Confetti: 60 canvas particles · brand palette · gravity physics
- Headline: "X-Win Streak!" if streak > 0, else "You Won!"
- Payout in XLM · `--font-mono`

**Loss state**
- Top border: `3px solid --color-state-danger`
- Icon: 48px X SVG in `#FAE8E8` circle
- Headline: "Better Luck Next Time"
- Forfeited wager amount

**Entry animation:** `opacity 0 + translateY(12px) + scale(0.97)` → full · `var(--motion-slow)` (420ms)

**Actions:** Win → Cash Out (primary) + Continue Streak (secondary) · Loss → Play Again (primary)

---

## WalletModal

**Wallets:** Freighter · Albedo · xBull · Rabet

**Connection states:** `idle → connecting → connected | error`

**Wallet list item**
- Full-width button
- Left: wallet name (weight `600`) + description (`--color-fg-muted`)
- Right: arrow `→` at rest · spinner while connecting
- All buttons disabled while any connection is in-flight · `aria-busy="true"` on active item

**Connected state:** green "● Connected" badge · wallet name · truncated address · "Done" button
**Error state:** `role="alert"` banner · ⚠ icon + message

---

## WagerInput

- Validation regex: `^(\d*\.?\d{0,7})$` (XLM 7 decimal places)
- Min/max enforced on blur and submit, not on keystroke
- Error: `aria-invalid="true"` on input + `role="alert"` on message
- Style: `--font-mono` · `--color-bg-subtle` background

---

## LoadingSpinner

| Size | Diameter | Usage |
|---|---|---|
| `small` | 16px | Inside buttons |
| `medium` | 28px | Inline content |
| `large` | 44px | Full-section loading |

Modes: `inline` (flows with text) · `overlay` (centered over parent)
Reduced motion: spinning ring → pulsing dot

---

## OutcomeChip

| State | Text color | Background |
|---|---|---|
| `win` | `--color-state-success` | 10% success tint |
| `loss` | `--color-state-danger` | 10% danger tint |
| `pending` | `--color-state-info` | 10% info tint |

Shape: `border-radius: var(--radius-pill)` · padding `var(--space-1) var(--space-3)` · `font-size: var(--font-size-xs)` · weight `600`

---

## HeroSection

- Layout: 2-column grid `7fr / 5fr` → 1 column on mobile
- Headline: `--font-display` · `--font-size-hero` · `--line-height-tight`
- CTA pair: primary (filled ink) + secondary (outline) · `gap: var(--space-3)`
- Trust strip: 3 pill chips below grid · `--radius-pill` · `--font-mono` · `--font-size-xs`

---

## EconomicsPanel

- 3 panels: Fee Model · Example Payouts table · Reserve Solvency
- All numeric values: `--font-mono` treatment
- Table: semantic `<table>` with `scope="col"` headers · net payout column accented

---

## Footer

- 3-column top: Brand (logo + tagline + social) · Navigation · Resources
- Bottom bar: disclaimer left · copyright + legal links right
- Social icons: GitHub · Twitter/X · Discord — inline SVG 20×20 · `fill="currentColor"` · `aria-label` on each

---

## Toast / ToastProvider

- Types: `success` · `error` · `warning` · `info`
- Position: fixed bottom-right
- Auto-dismiss: 5s default
- Animation: slide-in from right · fade-out on exit
- Manual dismiss button always present

---

## ErrorBoundary

- Catches render errors, shows fallback UI
- Default fallback: "Try Again" + "Reload" buttons
- Supports `resetKeys` for automatic recovery
- Custom `fallback` render prop for per-context overrides
