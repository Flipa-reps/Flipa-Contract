# Tossd Frontend — Design Handoff Pack
> Issue #91 · Branch: `design/handoff-pack` · Closes #91

Assembled for engineers implementing the Tossd frontend. Read these in order before writing any component code.

---

## Contents

| File | What's inside |
|---|---|
| [01-tokens.md](./01-tokens.md) | All design tokens — color, type, spacing, radius, shadow, motion |
| [02-component-specs.md](./02-component-specs.md) | Component props, redlines, variant states, visual specs |
| [03-interaction-notes.md](./03-interaction-notes.md) | Motion system, state machines, animation specs, UX flows |
| [04-accessibility.md](./04-accessibility.md) | Touch targets, focus, ARIA patterns, contrast, semantic HTML |
| [05-assets-and-exports.md](./05-assets-and-exports.md) | Fonts, icons, token exports, background CSS, brand rules |
| [06-implementation-priorities.md](./06-implementation-priorities.md) | P0→P4 build order, hook wiring, per-component acceptance checklist |

---

## Quick Reference

**Token file:** `frontend/tokens/tossd.tokens.css`
**Component directory:** `frontend/components/`
**Tech stack:** React 19 · TypeScript · CSS Modules · Vite · Stellar/Soroban

**Three rules before touching any component:**
1. Import tokens — `@import "../tokens/tossd.tokens.css";`
2. No raw hex or px values — token references only
3. Run `npm run test:a11y` before opening a PR

---

## Acceptance Criteria (Issue #91)

- [x] Component specs and redlines documented
- [x] Design tokens exported with usage rules
- [x] Interaction notes and state machines documented
- [x] Accessibility baseline defined
- [x] Asset and font export notes included
- [x] Implementation priorities ordered P0→P4
- [x] Per-component acceptance checklist provided

