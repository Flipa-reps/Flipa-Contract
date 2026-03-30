import { test, expect } from '@playwright/test';

test.describe('Wallet Connection @cross-browser', () => {
  test.use({ viewport: { width: 1280, height: 720 } });

  test('opens wallet modal and simulates connect across browsers', async ({ page }) => {
    await page.goto('/');
    
    // Click wallet button (assume NavBar or CTA)
    await page.getByRole('button', { name: /wallet|connect/i }).first().click();
    
    // Assert modal visible
    await expect(page.getByRole('dialog')).or(page.getByTestId('wallet-modal')).toBeVisible();
    
    // Mock Stellar connect response
    await page.route('**/stellar/**', route => route.fulfill({ status: 200, body: '{}' }));
    
    // Click connect
    await page.getByRole('button', { name: /connect|sign in/i }).click();
    
    // Assert connected state
    await expect(page.getByText(/connected|wallet ready/i)).toBeVisible();
    
    // Screenshot for visual regression
    await expect(page).toHaveScreenshot('wallet-connected.png');
  });
  
  test('wallet modal responsive on mobile', async ({ page }) => {
    test.use({ ...devices['iPhone 12'] });
    
    await page.goto('/');
    await page.getByRole('button', { name: /wallet/i }).click();
    await expect(page.getByRole('dialog')).toBeVisible();
  });
});

