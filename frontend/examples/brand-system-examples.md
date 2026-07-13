# Flipa Brand System Examples

These snippets show how to apply Flipa visual tokens without hardcoded styles.

## 1) Import tokens

```css
@import "../tokens/flipa.tokens.css";
```

## 2) Primary and secondary buttons

```css
.btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: var(--space-2);
  min-height: 44px;
  padding: 0 var(--space-4);
  border-radius: var(--radius-md);
  font-family: var(--font-body);
  font-size: var(--font-size-sm);
  font-weight: 600;
  transition: all var(--motion-base) var(--ease-standard);
}

.btn-primary {
  background: var(--interactive-primary-bg);
  color: var(--interactive-primary-fg);
  border: 1px solid var(--interactive-primary-bg);
}

.btn-secondary {
  background: transparent;
  color: var(--text-default);
  border: 1px solid var(--interactive-secondary-border);
}
```

## 3) Outcome status chips (win/loss/pending)

```css
.status-chip {
  display: inline-flex;
  align-items: center;
  gap: var(--space-2);
  border-radius: var(--radius-pill);
  padding: var(--space-1) var(--space-3);
  font-size: var(--font-size-xs);
  font-weight: 600;
}

.status-chip--win {
  color: var(--color-state-success);
  background: color-mix(in srgb, var(--color-state-success) 10%, white);
}

.status-chip--loss {
  color: var(--color-state-danger);
  background: color-mix(in srgb, var(--color-state-danger) 10%, white);
}

.status-chip--pending {
  color: var(--color-state-info);
  background: color-mix(in srgb, var(--color-state-info) 10%, white);
}
```

## 4) Wager panel (mono numeric treatment)

```css
.wager-panel {
  background: var(--surface-default);
  border: 1px solid var(--color-border-default);
  border-radius: var(--radius-lg);
  padding: var(--space-6);
  box-shadow: var(--shadow-soft);
}

.wager-value,
.multiplier-value,
.tx-hash {
  font-family: var(--font-mono);
  font-size: var(--font-size-mono);
  color: var(--text-default);
}
```

## 5) Motion-safe interaction

```css
.flip-result {
  transition: opacity var(--motion-base) var(--ease-standard),
    transform var(--motion-base) var(--ease-standard);
}

.flip-result[data-state="enter"] {
  opacity: 0;
  transform: translateY(6px);
}

.flip-result[data-state="ready"] {
  opacity: 1;
  transform: translateY(0);
}

@media (prefers-reduced-motion: reduce) {
  .flip-result {
    transition: none;
    transform: none;
  }
}
```

## 6) Rationale summary

- Tokenized values preserve consistency across landing pages and app UI.
- Mono numerics improve confidence when reading multipliers and payouts.
- Motion is used for state comprehension, not decorative effects.
