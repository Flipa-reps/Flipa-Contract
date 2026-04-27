# Tossd Design Tokens
> Source of truth: `frontend/tokens/tossd.tokens.css`
> Style direction: `webapp-02-japaneseswiss_light`
>
> **Rule: never use raw hex or px values in components. Always reference a token.**

---

## Color

### Backgrounds
| Token | Value | Usage |
|---|---|---|
| `--color-bg-base` | `#F7F6F3` | Page background |
| `--color-bg-surface` | `#FFFFFF` | Cards, modals, panels |
| `--color-bg-subtle` | `#EFEDE8` | Input backgrounds, hover fills |

### Foregrounds
| Token | Value | Usage |
|---|---|---|
| `--color-fg-primary` | `#171717` | Body text, headings |
| `--color-fg-secondary` | `#4D4D4D` | Supporting text |
| `--color-fg-muted` | `#6E6E6E` | Labels, captions, placeholders |

### Borders
| Token | Value | Usage |
|---|---|---|
| `--color-border-default` | `#D8D5CF` | Default borders |
| `--color-border-strong` | `#B9B5AE` | Emphasized borders, secondary button |

### Brand
| Token | Value | Usage |
|---|---|---|
| `--color-brand-ink` | `#111111` | Primary button bg, logo |
| `--color-brand-accent` | `#0F766E` | Teal accent, links, active states |
| `--color-brand-accent-soft` | `#DDF3F0` | Accent tint backgrounds |

### Semantic States
| Token | Value | Usage |
|---|---|---|
| `--color-state-success` | `#1D7A45` | Win outcome, connected wallet |
| `--color-state-warning` | `#B5681D` | Warning states |
| `--color-state-danger` | `#A12A2A` | Loss outcome, danger button |
| `--color-state-info` | `#1F5FAF` | Commit hash, info states |
| `--color-focus-ring` | `#0F766E` | Focus outline on all interactive elements |

### Semantic Aliases (component shortcuts)
| Alias | Resolves to |
|---|---|
| `--surface-default` | `--color-bg-surface` |
| `--text-default` | `--color-fg-primary` |
| `--text-muted` | `--color-fg-secondary` |
| `--interactive-primary-bg` | `--color-brand-ink` |
| `--interactive-primary-fg` | `--color-bg-surface` |
| `--interactive-secondary-border` | `--color-border-strong` |

---

## Typography

### Font Families
| Token | Stack | Role |
|---|---|---|
| `--font-display` | Ivar Display → Canela → Times New Roman (serif) | Hero headlines, card titles |
| `--font-body` | Suisse Intl → Inter → Helvetica Neue (sans-serif) | Body copy, UI labels, buttons |
| `--font-mono` | JetBrains Mono → IBM Plex Mono (monospace) | Hashes, addresses, numeric values, eyebrows |

### Font Sizes
| Token | Value | Usage |
|---|---|---|
| `--font-size-hero` | `clamp(3rem, 8vw, 6.5rem)` | Hero `<h1>` |
| `--font-size-h1` | `clamp(2rem, 4vw, 3.5rem)` | Section `<h1>` |
| `--font-size-h2` | `clamp(1.5rem, 3vw, 2.25rem)` | Section `<h2>`, result headline |
| `--font-size-h3` | `1.25rem` | Card titles, panel headings |
| `--font-size-body` | `1rem` | Default body text |
| `--font-size-sm` | `0.875rem` | Secondary text, button labels |
| `--font-size-xs` | `0.75rem` | Eyebrows, captions, mono labels |
| `--font-size-mono` | `0.875rem` | Monospace numeric values |

### Font Weights
| Token | Value |
|---|---|
| `--font-weight-regular` | `400` |
| `--font-weight-medium` | `500` |
| `--font-weight-semibold` | `600` |
| `--font-weight-bold` | `700` |

### Line Heights
| Token | Value | Usage |
|---|---|---|
| `--line-height-tight` | `1.2` | Headlines |
| `--line-height-normal` | `1.5` | Body text |
| `--line-height-relaxed` | `1.7` | Long-form descriptions |

### Eyebrow Pattern
Eyebrows (section labels above headings) always use:
- `font-family: var(--font-mono)`
- `font-size: var(--font-size-xs)`
- `letter-spacing: 0.18em`
- `text-transform: uppercase`
- `color: var(--color-brand-accent)`

---

## Spacing
| Token | Value |
|---|---|
| `--space-1` | `4px` |
| `--space-2` | `8px` |
| `--space-3` | `12px` |
| `--space-4` | `16px` |
| `--space-6` | `24px` |
| `--space-8` | `32px` |
| `--space-12` | `48px` |
| `--space-16` | `64px` |

---

## Border Radius
| Token | Value | Usage |
|---|---|---|
| `--radius-sm` | `6px` | Small controls, tags |
| `--radius-md` | `10px` | Buttons, inputs |
| `--radius-lg` | `16px` | Cards, modals, panels |
| `--radius-pill` | `999px` | Status chips, badges |

---

## Shadows
| Token | Value | Usage |
|---|---|---|
| `--shadow-soft` | `0 2px 10px rgba(0,0,0,0.06)` | Subtle card lift |
| `--shadow-card` | `0 8px 30px rgba(0,0,0,0.08)` | Elevated cards, scrolled nav |

---

## Motion
| Token | Value | Usage |
|---|---|---|
| `--motion-fast` | `140ms` | Hover transitions, focus rings |
| `--motion-base` | `220ms` | Card entry, modal open/close |
| `--motion-slow` | `420ms` | Game result entry, coin flip settle |
| `--ease-standard` | `cubic-bezier(0.2, 0.8, 0.2, 1)` | All transitions |

**Reduced motion:** all `--motion-*` tokens resolve to `0ms` under `prefers-reduced-motion: reduce`. Never hardcode durations outside these tokens.
