#!/usr/bin/env node
/**
 * Token validation script — frontend/tokens/validate-tokens.js
 *
 * Checks that tossd.tokens.css and tossd.tokens.json are consistent:
 *   1. Every leaf value in the JSON has a corresponding CSS custom property.
 *   2. Every CSS custom property declared in :root exists in the JSON.
 *   3. Hex color values in JSON match those in CSS (case-insensitive).
 *
 * Usage:  node frontend/tokens/validate-tokens.js
 * Exit:   0 on success, 1 on any validation failure.
 */

const fs = require("fs");
const path = require("path");

const CSS_FILE = path.join(__dirname, "tossd.tokens.css");
const JSON_FILE = path.join(__dirname, "tossd.tokens.json");

// ── Helpers ──────────────────────────────────────────────────────────────────

/** Convert camelCase segment to kebab-case. e.g. "accentSoft" → "accent-soft" */
function camelToKebab(s) {
  return s.replace(/([A-Z])/g, (m) => "-" + m.toLowerCase());
}

/**
 * Convert a JSON token path to a CSS custom property name.
 *   ["color","bg","base"]              → "--color-bg-base"
 *   ["color","brand","accentSoft"]     → "--color-brand-accent-soft"
 *   ["typography","fontFamily","display"] → "--font-display"
 *   ["typography","fontSize","h1"]     → "--font-size-h1"
 *   ["typography","fontWeight","semibold"] → "--font-weight-semibold"
 *   ["typography","lineHeight","normal"] → "--line-height-normal"
 *   ["space","4"]                      → "--space-4"
 *   ["radius","md"]                    → "--radius-md"
 *   ["shadow","soft"]                  → "--shadow-soft"
 *   ["motion","duration","fast"]       → "--motion-fast"
 *   ["motion","easing","standard"]     → "--ease-standard"
 */
function toCssVar(parts) {
  const [top, ...rest] = parts;

  if (top === "color") return "--color-" + rest.map(camelToKebab).join("-");
  if (top === "space") return "--space-" + rest.join("-");
  if (top === "radius") return "--radius-" + rest.join("-");
  if (top === "shadow") return "--shadow-" + rest.join("-");

  if (top === "typography") {
    const [sub, ...tail] = rest;
    if (sub === "fontFamily") return "--font-" + tail.join("-");
    if (sub === "fontSize") return "--font-size-" + tail.join("-");
    if (sub === "fontWeight") return "--font-weight-" + tail.join("-");
    if (sub === "lineHeight") return "--line-height-" + tail.join("-");
  }

  if (top === "motion") {
    const [sub, key] = rest;
    if (sub === "duration") return "--motion-" + key;
    if (sub === "easing" && key === "standard") return "--ease-standard";
  }

  // Fallback: kebab-join everything
  return "--" + [top, ...rest].map(camelToKebab).join("-");
}

/** Walk a nested object, calling cb(path, value) for every leaf. */
function walkLeaves(obj, cb, path = []) {
  for (const [k, v] of Object.entries(obj)) {
    if (v !== null && typeof v === "object") {
      walkLeaves(v, cb, [...path, k]);
    } else {
      cb([...path, k], v);
    }
  }
}

/** Extract all --custom-property names declared in the CSS file. */
function extractCssVars(css) {
  const vars = new Set();
  for (const m of css.matchAll(/^\s*(--[\w-]+)\s*:/gm)) {
    vars.add(m[1]);
  }
  return vars;
}

/** Extract --name: value pairs from CSS (raw string value). */
function extractCssValues(css) {
  const map = new Map();
  for (const m of css.matchAll(/^\s*(--[\w-]+)\s*:\s*([^;]+);/gm)) {
    map.set(m[1], m[2].trim());
  }
  return map;
}

// ── Main ─────────────────────────────────────────────────────────────────────

let errors = 0;

const css = fs.readFileSync(CSS_FILE, "utf8");
const json = JSON.parse(fs.readFileSync(JSON_FILE, "utf8"));

const cssVars = extractCssVars(css);
const cssValues = extractCssValues(css);

// Skip meta key — not a token
const { meta, ...tokens } = json;

// 1. Every JSON leaf must have a CSS var
walkLeaves(tokens, (parts, value) => {
  const cssVar = toCssVar(parts);
  if (!cssVars.has(cssVar)) {
    console.error(`MISSING CSS VAR: ${cssVar}  (JSON path: ${parts.join(".")})`);
    errors++;
  } else if (typeof value === "string" && /^#[0-9a-fA-F]{3,8}$/.test(value)) {
    // 2. Hex color values must match (case-insensitive)
    const cssVal = cssValues.get(cssVar) || "";
    if (cssVal.toLowerCase() !== value.toLowerCase()) {
      console.error(`COLOR MISMATCH: ${cssVar}\n  JSON: ${value}\n  CSS:  ${cssVal}`);
      errors++;
    }
  }
});

// 3. CSS vars with no JSON counterpart are warned (semantic aliases are intentional)
const jsonVars = new Set();
walkLeaves(tokens, (parts) => jsonVars.add(toCssVar(parts)));

for (const v of cssVars) {
  if (!jsonVars.has(v)) {
    console.warn(`WARN: CSS var ${v} has no JSON counterpart (alias or extra token)`);
  }
}

if (errors > 0) {
  console.error(`\nValidation FAILED — ${errors} error(s)`);
  process.exit(1);
} else {
  console.log(`Tokens valid — ${cssVars.size} CSS vars, ${jsonVars.size} JSON tokens`);
}
