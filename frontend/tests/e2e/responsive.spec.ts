import { test, expect } from '@playwright/test';
import { devices } from '@playwright/test';

test.describe('Responsive Behavior @mobile', () => {
  test('landing page responsive desktop/mobile', async ({ page }) => {
    // Desktop
    await page.setViewportSize({ width: 1920, height: 1080 });
    await page.goto('/');
    await expect(page).toHaveScreenshot('landing-desktop.png');
    
    // Mobile
    await page.setViewportSize({ width: 375, height: 812 });
    await page.goto('/');
    await expect(page).toHaveScreenshot('landing-mobile.png');
  });
  
  test('game interface responsive', async ({ page }) => {
    test.use(devices['iPhone 12']);
    
    await page.goto('/');
    // Interact to load game
    await page.getByRole('button', { name: /play|start/i }).first().click();
    await expect(page.locator('.game-container')).toBeVisible();
    await expect(page).toHaveScreenshot('game-mobile.png');
  });
});

