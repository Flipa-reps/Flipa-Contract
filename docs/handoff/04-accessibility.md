# Tossd Accessibility Baseline

All components must meet these requirements before shipping. The test suite includes `jest-axe` checks — run `npm run test:a11y` to verify.

---

## Touch Targets

- Minimum `44×44px` on every interactive element (WCAG 2.5.5)
- Applies even to `size="sm"` buttons — `min-height` stays `44px`
- Applies to icon-only buttons, close buttons, hamburger, social links

---

## Focus Management

- Focus ring: `2px solid #0F766E` · `outline-offset: 2px`
- Use `:focus-visible` not `:focus` — avoids outlines on mouse click
- Never suppress or override focus outlines
- **Modals:** trap focus within the dialog panel while open; return focus to trigger on close
- **Mobile menu:** trap focus within the drawer; return focus to hamburger on close
- **Initial focus:** first focusable element in panel, or `initialFocusRef` if provided

---

## ARIA Patterns

### Dynamic regions
```tsx
// Game state, coin flip result — polite announcement
<div aria-live="polite" aria-atomic="true">...</div>
```

### Errors and alerts
```tsx
// Immediate announcement — wallet error, form error
<div role="alert">...</div>
```

### Loading buttons
```tsx
<button aria-busy={loading} disabled={loading}>...</button>
```

### Disabled buttons
```tsx
// Set both — native disabled + ARIA
<button disabled aria-disabled="true">...</button>
```

### Icon-only buttons
```tsx
// Always provide aria-label
<button aria-label="Close wallet modal">✕</button>
```

### Modal
```tsx
<div
  role="dialog"
  aria-modal="true"
  aria-labelledby="modal-title-id"
  aria-describedby="modal-desc-id"
>
```

### Tables
```tsx
<th scope="col">Column Header</th>
```

### Navigation lists
```tsx
// When CSS resets remove list semantics, restore them
<ul role="list">
```

### Decorative elements
```tsx
// Ambient blobs, decorative SVGs, confetti canvas
<div aria-hidden="true" />
<svg aria-hidden="true" />
<canvas aria-hidden="true" />
```

---

## Screen Reader Only Text

Use `.srOnly` for visually hidden but announced content (e.g. coin flip result):

```css
.srOnly {
  position: absolute;
  width: 1px;
  height: 1px;
  padding: 0;
  margin: -1px;
  overflow: hidden;
  clip: rect(0, 0, 0, 0);
  white-space: nowrap;
  border: 0;
}
```

---

## Color Contrast

- Body text `#171717` on `#F7F6F3`: passes AAA
- Secondary text `#4D4D4D` on `#F7F6F3`: passes AA
- Brand accent `#0F766E` on white: passes AA for large text / bold — pair with icon or bold weight for small text
- State success `#1D7A45` on white: passes AA
- State danger `#A12A2A` on white: passes AA

**Rule: never use color as the sole indicator of state.** Always pair with an icon, text label, or pattern.

---

## Semantic HTML

- Use `<button>` for actions, `<a>` for navigation
- Use `<dl>` for key-value pairs (game stats, wallet address)
- Use `<table>` with proper `<thead>` / `<tbody>` / `scope` for data tables
- Use `<section>` with `aria-labelledby` for page sections
- Use `<nav>` with `aria-label` for navigation landmarks
- Use `<footer>` with `aria-label="Site footer"`
- Use `<header>` with `role="banner"` on the NavBar

---

## Form Inputs

- Every `<input>` must have an associated `<label>` (visible or `.srOnly`)
- Error messages: `aria-invalid="true"` on input + `role="alert"` on message element
- Hint text: linked via `aria-describedby`
- Controlled components throughout — no uncontrolled inputs

---

## Testing

```bash
# Run a11y test suite
npm run test:a11y

# Run all tests including a11y
npm run test:all
```

The a11y suite uses `jest-axe` to catch violations automatically. Manual testing with a screen reader (NVDA/VoiceOver) is required before P0 components ship.
