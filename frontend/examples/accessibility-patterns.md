# Tossd Accessibility Patterns

This note captures the accessibility conventions used across the frontend and
the checks that protect them.

## Landmark structure

- Use a single `banner`, `main`, and `contentinfo` landmark per page shell.
- Give each major content section a stable accessible name with
  `aria-label` or `aria-labelledby`.
- Prefer list semantics for grouped items such as trust chips, timelines,
  histories, and step indicators.

## Keyboard behavior

- Every action must be reachable with the keyboard.
- Dialogs trap focus while open and restore focus to the opener when closed.
- Hamburger and mobile menu flows should support `Tab`, `Shift+Tab`, and `Escape`.
- Radio groups should support arrow-key navigation.

## Screen reader support

- Use `role="status"` for non-blocking announcements.
- Use `role="alert"` for failures that need immediate attention.
- Keep `aria-live="polite"` for background updates and `aria-live="assertive"`
  for true errors.
- Mark decorative SVGs and animation layers with `aria-hidden="true"`.

## Contrast and focus

- Text and controls should meet WCAG 2.1 AA contrast in both the default and
  elevated surface themes.
- Focus rings should remain visible on every interactive element.
- Respect reduced-motion preferences for decorative animations.

## Test coverage

- `npm run test:a11y` runs the dedicated axe and interaction checks.
- `frontend/tests/typography.test.ts` validates token contrast ratios.
- `frontend/tests/a11y.test.tsx` covers landmarks, dialogs, keyboard flows,
  live regions, focus restoration, and token contrast.

## Review checklist

- Can the component be operated without a pointer?
- Does it expose a clear name, role, and state?
- Does focus move predictably when the UI opens or closes?
- Does any text or icon-only control still make sense to a screen reader?
