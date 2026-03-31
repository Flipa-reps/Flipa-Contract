import { test, expect } from "@playwright/test";

const SB = process.env.PLAYWRIGHT_STORYBOOK_URL ?? "http://127.0.0.1:6006";

test.describe("visual regression @visual", () => {
  test("button-like variants in OutcomeChip", async ({ page }) => {
    await page.goto(`${SB}/?path=/story/primitives-outcomechip--all-variants`);
    await expect(page.locator("#storybook-root")).toHaveScreenshot("outcome-chip-variants.png");
  });

  test("game flow components", async ({ page }) => {
    await page.goto(`${SB}/?path=/story/sections-gameflowsteps--default`);
    await expect(page.locator("#storybook-root")).toHaveScreenshot("game-flow-steps.png");
  });

  test("responsive transaction history desktop", async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 900 });
    await page.goto(`${SB}/?path=/story/game-transactionhistory--paginated`);
    await expect(page.locator("#storybook-root")).toHaveScreenshot("transaction-history-desktop.png");
  });

  test("responsive transaction history mobile", async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    await page.goto(`${SB}/?path=/story/game-transactionhistory--paginated`);
    await expect(page.locator("#storybook-root")).toHaveScreenshot("transaction-history-mobile.png");
  });
});
