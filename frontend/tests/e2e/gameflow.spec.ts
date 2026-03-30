import { test, expect } from '@playwright/test';
import { devices } from '@playwright/test';

test.describe('Game Flow @cross-browser', () => {
  test('complete wager → commit → reveal → cashout', async ({ page }) => {
    await page.goto('/');
    
    // Open wallet if needed
    if (await page.getByRole('button', { name: /wallet/i }).isVisible()) {
      await page.getByRole('button', { name: /wallet/i }).click();
      await page.getByRole('button', { name: /connect/i }).click();
    }
    
    // Wager input
    await page.getByRole('spinbutton', { name: /wager/i }).fill('10');
    
    // Select side
    await page.getByRole('radio', { name: /heads/i }).check();
    
    // Commit wager
    await page.getByRole('button', { name: /commit|place bet/i }).click();
    
    // Assert committed state
    await expect(page.getByText(/awaiting reveal|committed/i)).toBeVisible();
    
    // Reveal
    await page.getByRole('button', { name: /reveal/i }).click();
    
    // Assert result
    await expect(page.getByRole('status', { name: /win|loss/i })).toBeVisible();
    
    // Cash out if won
    if (await page.getByRole('button', { name: /cash out/i }).isVisible()) {
      await page.getByRole('button', { name: /cash out/i }).click();
    }
    
    await expect(page).toHaveScreenshot('gameflow-complete.png');
  });
});

