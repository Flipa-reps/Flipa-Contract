/**
 * Visual regression tests @visual
 *
 * Strategy:
 *   - Tests are tagged @visual so `npm run test:visual` (--grep @visual) runs them.
 *   - First run: `npx playwright test --grep @visual --update-snapshots` generates baselines.
 *   - Subsequent runs: diffs are compared against baselines with maxDiffPixelRatio: 0.02.
 *   - animations: "disabled" in config ensures deterministic screenshots.
 *   - Each test navigates to a stable UI state before capturing.
 *   - Viewport matrix: desktop (1280×800), tablet (768×1024), mobile (375×812).
 *
 * Viewports tested:
 *   desktop  — 1280×800  (standard laptop)
 *   tablet   — 768×1024  (iPad portrait)
 *   mobile   — 375×812   (iPhone SE / 13 mini)
 */

import { test, expect, Page } from "@playwright/test";

// ─── Viewport helpers ─────────────────────────────────────────────────────────

const VIEWPORTS = {
  desktop: { width: 1280, height: 800 },
  tablet:  { width: 768,  height: 1024 },
  mobile:  { width: 375, height: 812 },
} as const;

type VP = keyof typeof VIEWPORTS;

async function atViewport(page: Page, vp: VP, fn: () => Promise<void>) {
  await page.setViewportSize(VIEWPORTS[vp]);
  await fn();
}

/** Wait for fonts + layout to settle before screenshotting. */
async function settle(page: Page) {
  await page.waitForLoadState("networkidle");
  // Give CSS transitions a tick to finish (animations are disabled in config)
  await page.waitForTimeout(100);
}

// ─── Landing page ─────────────────────────────────────────────────────────────

test.describe("Landing page visual @visual", () => {
  for (const vp of ["desktop", "tablet", "mobile"] as VP[]) {
    test(`landing — ${vp} @visual`, async ({ page }) => {
      await atViewport(page, vp, async () => {
        await page.goto("/");
        await settle(page);
        await expect(page).toHaveScreenshot(`landing-${vp}.png`, { fullPage: true });
      });
    });
  }
});

// ─── NavBar ───────────────────────────────────────────────────────────────────

test.describe("NavBar visual @visual", () => {
  test("navbar — desktop default @visual", async ({ page }) => {
    await page.setViewportSize(VIEWPORTS.desktop);
    await page.goto("/");
    await settle(page);
    await expect(page.getByRole("banner")).toHaveScreenshot("navbar-desktop.png");
  });

  test("navbar — desktop scrolled @visual", async ({ page }) => {
    await page.setViewportSize(VIEWPORTS.desktop);
    await page.goto("/");
    await page.evaluate(() => window.scrollBy(0, 100));
    await page.waitForTimeout(150);
    await expect(page.getByRole("banner")).toHaveScreenshot("navbar-desktop-scrolled.png");
  });

  test("navbar — desktop wallet connected @visual", async ({ page }) => {
    await page.setViewportSize(VIEWPORTS.desktop);
    await page.goto("/");
    await page.getByRole("button", { name: /connect wallet/i }).first().click();
    await page.getByRole("button", { name: /freighter/i }).click();
    await expect(page.getByText(/● Connected/i)).toBeVisible({ timeout: 3000 });
    await page.getByRole("button", { name: /done/i }).click();
    await expect(page.getByRole("banner")).toHaveScreenshot("navbar-desktop-connected.png");
  });

  test("navbar — mobile hamburger closed @visual", async ({ page }) => {
    await page.setViewportSize(VIEWPORTS.mobile);
    await page.goto("/");
    await settle(page);
    await expect(page.getByRole("banner")).toHaveScreenshot("navbar-mobile-closed.png");
  });

  test("navbar — mobile menu open @visual", async ({ page }) => {
    await page.setViewportSize(VIEWPORTS.mobile);
    await page.goto("/");
    await page.getByRole("button", { name: /open navigation menu/i }).click();
    await page.waitForTimeout(150);
    await expect(page.locator("body")).toHaveScreenshot("navbar-mobile-open.png");
  });
});

// ─── Wallet modal ─────────────────────────────────────────────────────────────

test.describe("Wallet modal visual @visual", () => {
  test("wallet modal — idle @visual", async ({ page }) => {
    await page.setViewportSize(VIEWPORTS.desktop);
    await page.goto("/");
    await page.getByRole("button", { name: /connect wallet/i }).first().click();
    await expect(page.getByRole("dialog")).toBeVisible();
    await expect(page.getByRole("dialog")).toHaveScreenshot("wallet-modal-idle.png");
  });

  test("wallet modal — connecting state @visual", async ({ page }) => {
    await page.setViewportSize(VIEWPORTS.desktop);
    await page.goto("/");
    await page.getByRole("button", { name: /connect wallet/i }).first().click();
    await page.getByRole("button", { name: /freighter/i }).click();
    // Capture the connecting (spinner) state
    await expect(page.locator("[aria-busy='true']")).toBeVisible();
    await expect(page.getByRole("dialog")).toHaveScreenshot("wallet-modal-connecting.png");
  });

  test("wallet modal — connected state @visual", async ({ page }) => {
    await page.setViewportSize(VIEWPORTS.desktop);
    await page.goto("/");
    await page.getByRole("button", { name: /connect wallet/i }).first().click();
    await page.getByRole("button", { name: /freighter/i }).click();
    await expect(page.getByText(/● Connected/i)).toBeVisible({ timeout: 3000 });
    await expect(page.getByRole("dialog")).toHaveScreenshot("wallet-modal-connected.png");
  });

  test("wallet modal — mobile @visual", async ({ page }) => {
    await page.setViewportSize(VIEWPORTS.mobile);
    await page.goto("/");
    await page.getByRole("button", { name: /connect wallet/i }).first().click();
    await expect(page.getByRole("dialog")).toBeVisible();
    await expect(page.getByRole("dialog")).toHaveScreenshot("wallet-modal-mobile.png");
  });
});

// ─── Commit-reveal flow ───────────────────────────────────────────────────────

test.describe("CommitRevealFlow visual @visual", () => {
  test("commit step — initial @visual", async ({ page }) => {
    await page.setViewportSize(VIEWPORTS.desktop);
    await page.goto("/");
    await settle(page);
    const flow = page.locator("[aria-label='Commit-reveal flow']");
    await expect(flow).toHaveScreenshot("commit-step-initial.png");
  });

  test("commit step — after generate @visual", async ({ page }) => {
    await page.setViewportSize(VIEWPORTS.desktop);
    await page.goto("/");
    await page.getByRole("button", { name: /generate/i }).click();
    await expect(page.getByLabel(/your secret/i)).not.toHaveValue("");
    const flow = page.locator("[aria-label='Commit-reveal flow']");
    await expect(flow).toHaveScreenshot("commit-step-generated.png");
  });

  test("pending step @visual", async ({ page }) => {
    await page.setViewportSize(VIEWPORTS.desktop);
    await page.goto("/");
    await page.getByRole("button", { name: /generate/i }).click();
    await expect(page.getByLabel(/your secret/i)).not.toHaveValue("");
    await page.getByRole("button", { name: /submit commitment/i }).click();
    await expect(page.getByText(/commitment submitted/i)).toBeVisible({ timeout: 2000 });
    const flow = page.locator("[aria-label='Commit-reveal flow']");
    await expect(flow).toHaveScreenshot("commit-step-pending.png");
  });

  test("reveal step @visual", async ({ page }) => {
    await page.setViewportSize(VIEWPORTS.desktop);
    await page.goto("/");
    await page.getByRole("button", { name: /generate/i }).click();
    await expect(page.getByLabel(/your secret/i)).not.toHaveValue("");
    await page.getByRole("button", { name: /submit commitment/i }).click();
    await expect(page.getByText(/reveal your secret/i)).toBeVisible({ timeout: 3000 });
    const flow = page.locator("[aria-label='Commit-reveal flow']");
    await expect(flow).toHaveScreenshot("commit-step-reveal.png");
  });

  test("verified step @visual", async ({ page }) => {
    await page.setViewportSize(VIEWPORTS.desktop);
    await page.goto("/");
    await page.getByRole("button", { name: /generate/i }).click();
    await expect(page.getByLabel(/your secret/i)).not.toHaveValue("");
    await page.getByRole("button", { name: /submit commitment/i }).click();
    await expect(page.getByText(/reveal your secret/i)).toBeVisible({ timeout: 3000 });
    await page.getByLabel(/your secret/i).fill("anysecret");
    await page.getByRole("button", { name: /reveal & settle/i }).click();
    await expect(page.getByRole("status")).toContainText(/commitment verified/i, { timeout: 3000 });
    const flow = page.locator("[aria-label='Commit-reveal flow']");
    await expect(flow).toHaveScreenshot("commit-step-verified.png");
  });

  test("commit flow — mobile @visual", async ({ page }) => {
    await page.setViewportSize(VIEWPORTS.mobile);
    await page.goto("/");
    await settle(page);
    const flow = page.locator("[aria-label='Commit-reveal flow']");
    await expect(flow).toHaveScreenshot("commit-step-mobile.png");
  });

  test("commit flow — tablet @visual", async ({ page }) => {
    await page.setViewportSize(VIEWPORTS.tablet);
    await page.goto("/");
    await settle(page);
    const flow = page.locator("[aria-label='Commit-reveal flow']");
    await expect(flow).toHaveScreenshot("commit-step-tablet.png");
  });
});

// ─── Play section (full panel) ────────────────────────────────────────────────

test.describe("Play section visual @visual", () => {
  for (const vp of ["desktop", "tablet", "mobile"] as VP[]) {
    test(`play section — ${vp} @visual`, async ({ page }) => {
      await page.setViewportSize(VIEWPORTS[vp]);
      await page.goto("/");
      await settle(page);
      await page.locator("#play").scrollIntoViewIfNeeded();
      await page.waitForTimeout(100);
      await expect(page.locator("#play")).toHaveScreenshot(`play-section-${vp}.png`);
    });
  }
});

// ─── Footer ───────────────────────────────────────────────────────────────────

test.describe("Footer visual @visual", () => {
  for (const vp of ["desktop", "mobile"] as VP[]) {
    test(`footer — ${vp} @visual`, async ({ page }) => {
      await page.setViewportSize(VIEWPORTS[vp]);
      await page.goto("/");
      await page.evaluate(() => window.scrollTo(0, document.body.scrollHeight));
      await page.waitForTimeout(150);
      await expect(page.locator("footer")).toHaveScreenshot(`footer-${vp}.png`);
    });
  }
});

// ─── Full-page snapshots ──────────────────────────────────────────────────────

test.describe("Full page visual @visual", () => {
  test("full page — desktop after wallet connect @visual", async ({ page }) => {
    await page.setViewportSize(VIEWPORTS.desktop);
    await page.goto("/");
    await page.getByRole("button", { name: /connect wallet/i }).first().click();
    await page.getByRole("button", { name: /freighter/i }).click();
    await expect(page.getByText(/● Connected/i)).toBeVisible({ timeout: 3000 });
    await page.getByRole("button", { name: /done/i }).click();
    await settle(page);
    await expect(page).toHaveScreenshot("full-page-connected-desktop.png", { fullPage: true });
  });

  test("full page — desktop after verified @visual", async ({ page }) => {
    await page.setViewportSize(VIEWPORTS.desktop);
    await page.goto("/");
    await page.getByRole("button", { name: /generate/i }).click();
    await expect(page.getByLabel(/your secret/i)).not.toHaveValue("");
    await page.getByRole("button", { name: /submit commitment/i }).click();
    await expect(page.getByText(/reveal your secret/i)).toBeVisible({ timeout: 3000 });
    await page.getByLabel(/your secret/i).fill("anysecret");
    await page.getByRole("button", { name: /reveal & settle/i }).click();
    await expect(page.getByRole("status")).toContainText(/commitment verified/i, { timeout: 3000 });
    await settle(page);
    await expect(page).toHaveScreenshot("full-page-verified-desktop.png", { fullPage: true });
  });
});
