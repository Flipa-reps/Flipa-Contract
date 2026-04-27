# Tossd Asset & Export Notes

---

## Design Tokens

**Canonical file:** `frontend/tokens/tossd.tokens.css`
**JSON mirror:** `frontend/tokens/tossd.tokens.json` (source of truth for tooling/Figma sync)

Import in every component CSS module:
```css
@import "../tokens/tossd.tokens.css";
```

Never import tokens from a relative path that skips the `tokens/` directory. Always use the canonical path.

---

## Fonts

Fonts are **not bundled** in the repo. Load via `@font-face` or a font service before first paint.

**Priority load order:**
1. `Suisse Intl` (body) — highest usage, load first
2. `Ivar Display` (display) — hero headline, load second
3. `JetBrains Mono` (mono) — hashes and numerics, can defer

**Fallback stacks** are defined in the token file and render acceptably without custom fonts:
- Display → Times New Roman (serif)
- Body → Helvetica Neue / system-ui (sans-serif)
- Mono → system monospace

**Recommendation:** use `font-display: swap` to avoid invisible text during font load.

---

## Icons

All icons are **inline SVGs** — no icon font, no external sprite sheet.

Rules:
- Decorative icons: `aria-hidden="true"` on the `<svg>`
- Meaningful icons (standalone buttons): `aria-label` on the parent `<button>`, icon stays `aria-hidden`
- Size: match context — social icons 20×20, outcome icons 48×48, button icons 16×16
- Color: `fill="currentColor"` so icons inherit text color via CSS

**Social icons available in Footer:** GitHub · Twitter/X · Discord (all inline SVG, 20×20)

---

## Background

The page background is pure CSS — no image asset required:

```css
/* Page body */
background:
  radial-gradient(circle at top left, rgba(15, 118, 110, 0.14), transparent 32%),
  radial-gradient(circle at top right, rgba(166, 224, 208, 0.18), transparent 28%),
  linear-gradient(180deg, #f7f6f3 0%, #f2efe9 100%);
```

**Ambient blobs** (`.ambientOne`, `.ambientTwo`):
- `position: fixed` · `border-radius: 50%` · `filter: blur(72px)` · `pointer-events: none` · `aria-hidden="true"`
- `.ambientOne`: top-right · `rgba(15, 118, 110, 0.18)` · 32rem × 32rem
- `.ambientTwo`: left-center · `rgba(17, 17, 17, 0.08)` · 32rem × 32rem

---

## Confetti (GameResult)

No external library. Implemented as a lightweight canvas animation in `GameResult.tsx`:
- 60 particles
- Colors: `#0F766E`, `#1D7A45`, `#DDF3F0`, `#B5681D`, `#171717`
- Gravity: `0.18` per frame
- Alpha fade: `0.012` per frame
- Canvas is `aria-hidden="true"`, `pointer-events: none`

---

## CSS Modules

Every component has a co-located `.module.css` file. No global class names except:
- `appShell`, `mainContent`, `sectionFrame`, `sectionHeading`, `eyebrow` — defined in `frontend/src/styles.css`
- `.srOnly` — should be defined globally or in a shared utility file

---

## Brand Usage Rules

1. **Logo text:** "Tossd" — `--font-display` · weight `700` · `--color-fg-primary`
2. **Tagline:** "Trustless coinflips on Soroban." — `--font-body` · `--color-fg-secondary`
3. **Accent color** (`#0F766E`) is used for: active states, focus rings, eyebrows, step indicators, spinner accent, brand-accent-soft backgrounds
4. **Ink color** (`#111111`) is used for: primary buttons, logo, CTA links
5. **Mono font** is used for: all numeric values (wagers, payouts, multipliers), hashes, addresses, eyebrow labels

---

## Figma / Design Tool Handoff

- Token values are in `frontend/tokens/tossd.tokens.json` — import into Figma via Tokens Studio or equivalent
- Component structure mirrors the file tree: one component per file, co-located CSS
- All spacing, color, and type decisions map 1:1 to tokens — no custom values in Figma
