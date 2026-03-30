import { defineConfig, devices } from "@playwright/test";

const baseURL = process.env.PLAYWRIGHT_BASE_URL ?? "http://127.0.0.1:4173";
const storybookURL = process.env.PLAYWRIGHT_STORYBOOK_URL ?? "http://127.0.0.1:6006";

export default defineConfig({
  testDir: "./playwright",
  timeout: 30_000,
  expect: {
    timeout: 5_000,
    toHaveScreenshot: {
      maxDiffPixelRatio: 0.015,
    },
  },
  use: {
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
      command: "npm run storybook -- --ci --quiet",
      url: storybookURL,
      timeout: 120_000,
      reuseExistingServer: true,
    },
  ],
  metadata: {
    baseURL,
    storybookURL,
  },
});
