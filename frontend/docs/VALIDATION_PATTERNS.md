# Frontend Validation Patterns

This document outlines the validation and sanitization patterns used in the Flipa frontend to ensure data integrity and prevent security vulnerabilities like XSS.

## Core Principles

1.  **Strict Regex Filtering**: Input fields (like `WagerInput`) use controlled components with regex filtering on `onChange`. This prevents invalid characters (like non-numeric or excessive precision) from ever reaching the component state.
2.  **React-Driven XSS Prevention**: All user-provided strings are rendered using standard React JSX curly braces `{}`. React automatically escapes HTML entities, ensuring that payloads like `<script>` are rendered as literal text rather than being executed by the browser.
3.  **Aria-Live Error Messaging**: Validation errors are displayed using `<p role="alert">` or containers with `aria-live="polite"`. This ensures accessibility for screen readers while providing immediate visual feedback.
4.  **Boundary Testing**: All numeric inputs are validated against `min` and `max` constraints defined in the business logic (e.g., XLM wager limits) before form submission is enabled.

## Wager Input Pattern

`WagerInput` prevents non-numeric input and enforces XLM precision (7 decimal places).

```typescript
function handleWagerChange(e: React.ChangeEvent<HTMLInputElement>) {
  const raw = e.target.value;
  // Allow digits, one leading dot, and up to 7 decimal places
  // This is the first line of defense against XSS and invalid data
  if (!/^(\d*\.?\d{0,7})$/.test(raw)) return;
  // ... set state
}
```

## XSS Prevention Guarantee

The combination of **Input Filtering** and **React Auto-Escaping** provides a strong guarantee:
-   **Filtering**: Malicious scripts cannot be pasted into fields that enforce strict formats (like `WagerInput`).
-   **Escaping**: In fields that allow arbitrary text (like `CommitRevealFlow` secrets), any HTML tags are escaped during render.

Example of safe rendering:
```tsx
// Even if secret contains <script>, it is rendered safely
<p className={styles.cardDesc}>{secret}</p>
```

## Commitment Pattern

Secrets generated locally use `crypto.getRandomValues` for cryptographically strong randomness. Manual inputs are hashed using SHA-256 via Web Crypto API before being sent on-chain.

## Testing Strategy

All new forms must be tested in `frontend/tests/form-validation.test.tsx` for:
-   Valid and invalid boundary values.
-   Regex filtering of illegal characters.
-   Error message visibility and clearing.
-   Safe handling of XSS payloads in text fields.
