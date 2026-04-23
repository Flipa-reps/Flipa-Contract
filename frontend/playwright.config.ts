import { defineConfig, devices } from "@playwright/test";

const baseURL = process.env.PLAYWRIGHT_BASE_URL ?? "http://127.0.0.1:5173";

export default defineConfig({
  testDir: "./tests",
  testMatch: ["**/e2e/**/*.spec.ts", "**/visual/**/*.spec.ts"],
  timeout: 30_000,
  expect: {
    timeout: 5_000,
    toHaveScreenshot: {
      maxDiffPixelRatio: 0.02,
      animations: "disabled",
    },
  },
  use: {
    baseURL,
    trace: "on-first-retry",
    video: "retain-on-failure",
    screenshot: "only-on-failure",
  },
  projects: [
    { name: "chromium", use: { ...devices["Desktop Chrome"] } },
    { name: "mobile", use: { ...devices["iPhone 13"] } },
  ],
  webServer: [
    {
      command: "npm run dev -- --host 127.0.0.1",
      url: baseURL,
      timeout: 120_000,
      reuseExistingServer: true,
    },
  ],
  metadata: { baseURL },
});
