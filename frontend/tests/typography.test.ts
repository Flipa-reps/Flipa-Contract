/**
 * Typography system tests — frontend/tests/typography.test.ts
 *
 * Validates that:
 *   1. All token values are present and correctly typed in the JSON.
 *   2. Responsive clamp() values are well-formed.
 *   3. Font weight values are valid CSS integers.
 *   4. Contrast ratios meet WCAG AA (4.5:1 normal text, 3:1 large text).
 */

import { describe, it, expect } from "vitest";
import tokens from "../tokens/tossd.tokens.json";

// ── Helpers ──────────────────────────────────────────────────────────────────

/** Parse a hex color string to [r, g, b] in 0–255. */
function hexToRgb(hex: string): [number, number, number] {
  const h = hex.replace("#", "");
  const n = parseInt(h, 16);
  return [(n >> 16) & 0xff, (n >> 8) & 0xff, n & 0xff];
}

/** Relative luminance per WCAG 2.1 §1.4.3. */
function luminance([r, g, b]: [number, number, number]): number {
  const c = [r, g, b].map((v) => {
    const s = v / 255;
    return s <= 0.03928 ? s / 12.92 : Math.pow((s + 0.055) / 1.055, 2.4);
  });
  return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
}

/** WCAG contrast ratio between two hex colors. */
function contrast(fg: string, bg: string): number {
  const l1 = luminance(hexToRgb(fg));
  const l2 = luminance(hexToRgb(bg));
  const [light, dark] = l1 > l2 ? [l1, l2] : [l2, l1];
  return (light + 0.05) / (dark + 0.05);
}

// ── Token shape ──────────────────────────────────────────────────────────────

describe("typography tokens — JSON shape", () => {
  const { typography } = tokens;

  it("has all three font families", () => {
    expect(typography.fontFamily.display).toContain("serif");
    expect(typography.fontFamily.body).toContain("sans-serif");
    expect(typography.fontFamily.mono).toContain("monospace");
  });

  it("has all required font sizes", () => {
    const required = ["hero", "h1", "h2", "h3", "body", "sm", "xs", "mono"];
    for (const key of required) {
      expect(typography.fontSize).toHaveProperty(key);
    }
  });

  it("responsive sizes use clamp()", () => {
    expect(typography.fontSize.hero).toMatch(/^clamp\(/);
    expect(typography.fontSize.h1).toMatch(/^clamp\(/);
    expect(typography.fontSize.h2).toMatch(/^clamp\(/);
  });

  it("fixed sizes are rem values", () => {
    const fixed = ["h3", "body", "sm", "xs", "mono"] as const;
    for (const key of fixed) {
      expect(typography.fontSize[key]).toMatch(/rem$/);
    }
  });

  it("has all four font weights with correct values", () => {
    expect(typography.fontWeight.regular).toBe(400);
    expect(typography.fontWeight.medium).toBe(500);
    expect(typography.fontWeight.semibold).toBe(600);
    expect(typography.fontWeight.bold).toBe(700);
  });

  it("has all three line heights as numbers", () => {
    expect(typeof typography.lineHeight.tight).toBe("number");
    expect(typeof typography.lineHeight.normal).toBe("number");
    expect(typeof typography.lineHeight.relaxed).toBe("number");
  });

  it("line heights are in ascending order", () => {
    const { tight, normal, relaxed } = typography.lineHeight;
    expect(tight).toBeLessThan(normal);
    expect(normal).toBeLessThan(relaxed);
  });
});

// ── Contrast ratios ──────────────────────────────────────────────────────────

describe("typography contrast — WCAG AA", () => {
  const bg = tokens.color.bg;
  const fg = tokens.color.fg;

  // Primary text on both backgrounds must meet AAA (7:1) — they're our main text colors
  it("fg.primary on bg.base meets AA (4.5:1)", () => {
    expect(contrast(fg.primary, bg.base)).toBeGreaterThanOrEqual(4.5);
  });

  it("fg.primary on bg.surface meets AA (4.5:1)", () => {
    expect(contrast(fg.primary, bg.surface)).toBeGreaterThanOrEqual(4.5);
  });

  it("fg.secondary on bg.base meets AA (4.5:1)", () => {
    expect(contrast(fg.secondary, bg.base)).toBeGreaterThanOrEqual(4.5);
  });

  it("fg.secondary on bg.surface meets AA (4.5:1)", () => {
    expect(contrast(fg.secondary, bg.surface)).toBeGreaterThanOrEqual(4.5);
  });

  // Muted text is used for helper/label text — must meet AA for normal text
  it("fg.muted on bg.surface meets AA (4.5:1)", () => {
    expect(contrast(fg.muted, bg.surface)).toBeGreaterThanOrEqual(4.5);
  });

  it("fg.muted on bg.base meets AA (4.5:1)", () => {
    expect(contrast(fg.muted, bg.base)).toBeGreaterThanOrEqual(4.5);
  });
});
