# Cross-Browser Compatibility Tests - #377

## Setup
- Playwright e2e tests in `tests/e2e/`
- Browsers: Chrome, Firefox, Safari(WebKit), Edge, Mobile Chrome/Safari emulations
- Reporter: HTML (`playwright-report/index.html`)

## Run Tests
```bash
cd frontend
npm run playwright:install  # Download browsers (if fails, manual Chromium ok)
npm run test:e2e            # All projects
npm run test:browsers       # Alias
npx playwright test --project=chromium  # Single browser
npx playwright show-report  # View report/screenshots/videos
```

## Tested Flows
- Wallet connection (modal, connect)
- Game flow (wager, side, commit, reveal, cashout)
- Responsive (desktop/mobile screenshots)

## Results Matrix
View `playwright-report/index.html` for pass/fail per browser, screenshots for visual consistency.

## Browser-Specific Issues
- None found (verified locally)
- Mobile: Responsive modals/game UI pass on iPhone/Pixel emulations

## Coverage
All major user flows verified consistent behavior across targets.

