/**
 * Cross-browser compatibility tests — Issue #433
 *
 * Runs against all projects defined in playwright.config.ts:
 *   chromium · firefox · webkit (Safari) · edge · mobile-chrome · mobile-safari
 *
 * Browser support matrix
 * ──────────────────────
 * Feature                    Chrome  Firefox  Safari  Edge  Notes
 * CSS custom properties       ✓       ✓        ✓       ✓
 * CSS 3D transforms           ✓       ✓        ✓       ✓    coin flip animation
 * prefers-reduced-motion      ✓       ✓        ✓       ✓
 * Web Crypto (SHA-256)        ✓       ✓        ✓       ✓    commit-reveal
 * ES2020 (BigInt, optional ?) ✓       ✓        ✓       ✓
 * CSS :focus-visible          ✓       ✓        ✓       ✓
 * dialog / role=dialog        ✓       ✓        ✓       ✓
 * Freighter wallet extension  ✓       ✓        ✗       ✓    Safari: no extension
 * Albedo (web wallet)         ✓       ✓        ✓       ✓    popup-based, all browsers
 *
 * Known limitations
 * ─────────────────
 * - Safari does not support browser extensions, so Freighter is unavailable.
 *   The UI must degrade gracefully and offer Albedo as an alternative.
 * - Edge uses the Chromium engine; behaviour is identical to Chrome for JS/CSS.
 *   The Edge project primarily validates channel-specific quirks.
 */

import { test, expect, type Page, type BrowserContext } from "@playwright/test";

// ─── Helpers ──────────────────────────────────────────────────────────────────

/** Inject a stub window.freighter so wallet-detection logic can be tested. */
async function injectFreighterStub(page: Page) {
  await page.addInitScript(() => {
    (window as any).freighter = {
      isConnected: () => Promise.resolve(true),
      getPublicKey: () => Promise.resolve("GABC1234TESTPUBLICKEY"),
      signTransaction: (_xdr: string) => Promise.resolve({ signedTxXdr: _xdr }),
    };
  });
}

/** Inject a stub window.albedo for Albedo wallet detection. */
async function injectAlbedoStub(page: Page) {
  await page.addInitScript(() => {
    (window as any).albedo = {
      publicKey: () => Promise.resolve({ pubkey: "GABC1234TESTPUBLICKEY" }),
    };
  });
}

// ─── Page load & critical rendering ──────────────────────────────────────────

test.describe("Page load @cross-browser", () => {
  test("landing page loads and renders hero content", async ({ page }) => {
    await page.goto("/");
    await expect(page).toHaveTitle(/flipa/i);
    // At least one primary CTA or heading must be visible
    const hero = page.getByRole("heading", { level: 1 }).or(
      page.getByRole("button", { name: /play|start|connect/i }).first()
    );
    await expect(hero).toBeVisible();
  });

  test("no uncaught JS errors on load", async ({ page }) => {
    const errors: string[] = [];
    page.on("pageerror", (err) => errors.push(err.message));
    await page.goto("/");
    await page.waitForLoadState("networkidle");
    expect(errors).toHaveLength(0);
  });
});

// ─── CSS feature compatibility ────────────────────────────────────────────────

test.describe("CSS compatibility @cross-browser", () => {
  test("CSS custom properties resolve on body", async ({ page }) => {
    await page.goto("/");
    const bgColor = await page.evaluate(() =>
      getComputedStyle(document.body).getPropertyValue("--color-bg-base").trim()
    );
    // Token is defined — non-empty string means custom properties work
    expect(bgColor.length).toBeGreaterThan(0);
  });

  test("CSS 3D transforms are supported (coin flip)", async ({ page }) => {
    await page.goto("/");
    const supported = await page.evaluate(() => {
      const el = document.createElement("div");
      el.style.transform = "rotateY(180deg)";
      return el.style.transform !== "";
    });
    expect(supported).toBe(true);
  });

  test("prefers-reduced-motion media query is parseable", async ({ page }) => {
    await page.goto("/");
    const parseable = await page.evaluate(() =>
      typeof window.matchMedia("(prefers-reduced-motion: reduce)").matches === "boolean"
    );
    expect(parseable).toBe(true);
  });

  test(":focus-visible is supported", async ({ page }) => {
    await page.goto("/");
    const supported = await page.evaluate(() => {
      try {
        document.querySelector(":focus-visible");
        return true;
      } catch {
        return false;
      }
    });
    expect(supported).toBe(true);
  });
});

// ─── JavaScript feature compatibility ────────────────────────────────────────

test.describe("JS feature compatibility @cross-browser", () => {
  test("Web Crypto SHA-256 is available", async ({ page }) => {
    await page.goto("/");
    const result = await page.evaluate(async () => {
      const data = new TextEncoder().encode("test");
      const hash = await crypto.subtle.digest("SHA-256", data);
      return new Uint8Array(hash).length;
    });
    expect(result).toBe(32);
  });

  test("BigInt is supported", async ({ page }) => {
    await page.goto("/");
    const result = await page.evaluate(() => {
      const n = BigInt("9007199254740993");
      return n.toString();
    });
    expect(result).toBe("9007199254740993");
  });

  test("optional chaining and nullish coalescing work", async ({ page }) => {
    await page.goto("/");
    const result = await page.evaluate(() => {
      const obj: any = null;
      return (obj?.value ?? "fallback");
    });
    expect(result).toBe("fallback");
  });

  test("Promise.allSettled is available", async ({ page }) => {
    await page.goto("/");
    const result = await page.evaluate(async () => {
      const results = await Promise.allSettled([
        Promise.resolve(1),
        Promise.reject(new Error("x")),
      ]);
      return results.map((r) => r.status);
    });
    expect(result).toEqual(["fulfilled", "rejected"]);
  });
});

// ─── Wallet provider compatibility ───────────────────────────────────────────

test.describe("Wallet provider compatibility @cross-browser", () => {
  test("Freighter stub is detected when injected", async ({ page }) => {
    await injectFreighterStub(page);
    await page.goto("/");
    const detected = await page.evaluate(() => !!(window as any).freighter);
    expect(detected).toBe(true);
  });

  test("Albedo stub is detected when injected", async ({ page }) => {
    await injectAlbedoStub(page);
    await page.goto("/");
    const detected = await page.evaluate(() => !!(window as any).albedo);
    expect(detected).toBe(true);
  });

  test("UI renders without wallet extension present", async ({ page }) => {
    // No stub injected — simulates Safari or a clean browser profile
    await page.goto("/");
    // Page must still render; no crash
    await expect(page.locator("body")).toBeVisible();
    // A connect/wallet button or prompt should be present
    const walletEntry = page
      .getByRole("button", { name: /wallet|connect/i })
      .or(page.getByText(/connect wallet/i))
      .first();
    await expect(walletEntry).toBeVisible();
  });

  test("wallet modal opens and closes on all browsers", async ({ page }) => {
    await page.goto("/");
    const walletBtn = page.getByRole("button", { name: /wallet|connect/i }).first();
    await walletBtn.click();
    const dialog = page.getByRole("dialog");
    await expect(dialog).toBeVisible();
    // Close via Escape
    await page.keyboard.press("Escape");
    await expect(dialog).not.toBeVisible();
  });
});

// ─── Accessibility features across browsers ───────────────────────────────────

test.describe("Accessibility compatibility @cross-browser", () => {
  test("focus trap works inside modal", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: /wallet|connect/i }).first().click();
    const dialog = page.getByRole("dialog");
    await expect(dialog).toBeVisible();

    // Tab through focusable elements — focus must stay inside dialog
    await page.keyboard.press("Tab");
    const focusedInDialog = await page.evaluate(() => {
      const dialog = document.querySelector("[role='dialog']");
      return dialog?.contains(document.activeElement) ?? false;
    });
    expect(focusedInDialog).toBe(true);
  });

  test("aria-live regions are present for game announcements", async ({ page }) => {
    await page.goto("/");
    const liveRegions = await page.locator("[aria-live]").count();
    expect(liveRegions).toBeGreaterThan(0);
  });
});
