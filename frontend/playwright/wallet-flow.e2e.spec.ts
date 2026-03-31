import { test, expect } from "@playwright/test";

const SB = process.env.PLAYWRIGHT_STORYBOOK_URL ?? "http://127.0.0.1:6006";

test.describe("wallet connection flow @e2e", () => {
  test("connect wallet success", async ({ page }) => {
    await page.goto(`${SB}/?path=/story/modals-walletmodal--default`);
    await page.getByRole("button", { name: "Connect Wallet" }).click();
    await page.getByRole("button", { name: "Freighter" }).click();
    await expect(page.getByText("Wallet Connected")).toBeVisible();
    await expect(page.getByText("GDEMO...STELLAR_PUBLIC_KEY")).toBeVisible();
  });

  test("close/disconnect flow", async ({ page }) => {
    await page.goto(`${SB}/?path=/story/modals-walletmodal--default`);
    await page.getByRole("button", { name: "Connect Wallet" }).click();
    await page.getByRole("button", { name: "Freighter" }).click();
    await page.getByRole("button", { name: "Done" }).click();
    await expect(page.getByRole("heading", { name: "Wallet Connected" })).toHaveCount(0);
  });

  test("connection rejection error", async ({ page }) => {
    await page.goto(`${SB}/?path=/story/modals-walletmodal--connection-error`);
    await page.getByRole("button", { name: "Connect (will fail)" }).click();
    await page.getByRole("button", { name: "Freighter" }).click();
    await expect(page.getByRole("alert")).toContainText("Extension not installed");
  });

  test("wallet not installed scenario", async ({ page }) => {
    await page.goto(`${SB}/?path=/story/modals-walletmodal--connection-error`);
    await page.getByRole("button", { name: "Connect (will fail)" }).click();
    await page.getByRole("button", { name: "xBull" }).click();
    await expect(page.getByRole("alert")).toContainText("Extension not installed");
  });
});
