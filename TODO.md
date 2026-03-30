# Cross-Browser Compatibility Tests - Issue #377

## Plan Progress

✅ **Step 1**: Plan approved by user. Create TODO.md for tracking.

✅ **Step 2**: Create git branch `add-cross-browser-compatibility-tests`.

⏳ **Step 3**: cd frontend && npm install -D @playwright/test && npx playwright install --with-deps.

⏳ **Step 4**: Update frontend/package.json with Playwright scripts/deps.

✅ **Step 5**: Create frontend/playwright.config.ts.

✅ **Step 6**: Update frontend/vite.config.ts for baseURL.

✅ **Step 7**: Create frontend/tests/e2e/ directory and tests:
   - wallet.spec.ts (wallet connection)
   - gameflow.spec.ts (wager/commit/reveal/cashout)
   - responsive.spec.ts (mobile/desktop)

⏳ **Step 8**: Update frontend/README.md or create CROSS_BROWSER_TESTS.md with instructions/report.

⏳ **Step 9**: Run tests locally across browsers, verify screenshots/videos.

⏳ **Step 10**: Commit changes with scoped messages, create PR referencing #377.

**Notes**:
- Testing Chrome, Firefox, Safari(WebKit), Edge, mobile emulations.
- Use Playwright HTML reporter for cross-browser matrix.
- Mock Stellar wallet SDK for tests.


