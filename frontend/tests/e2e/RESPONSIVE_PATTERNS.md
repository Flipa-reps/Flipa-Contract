# Responsive Patterns

`frontend/tests/e2e/responsive.spec.ts` validates the responsive behavior that the current Flipa landing page actually ships today.

## Viewport matrix

- Mobile: `375x812`
- Tablet: `768x1024`
- Desktop: `1440x900`

## Orientation coverage

- Mobile portrait: `375x812`
- Mobile landscape: `812x375`
- Tablet portrait: `768x1024`
- Tablet landscape: `1024x768`
- Desktop portrait: `900x1440`
- Desktop landscape: `1440x900`

Orientation tests intentionally validate width-driven breakpoint changes after resize. In the current implementation, rotating a mobile or tablet viewport into landscape can move the header from the hamburger menu to the desktop navigation because the header breakpoint is based on width, not device category.

## Breakpoints exercised by the suite

- `768px`: `NavBar` swaps between desktop navigation and the mobile menu trigger.
- `900px`: the `#play` section switches between a two-column grid and a stacked layout.
- `480px` and `640px`: smaller component-level breakpoints exist in the component CSS and are indirectly covered by the mobile viewport assertions and overflow checks.

## Interaction strategy

- Desktop assertions validate standard mouse-driven actions such as opening and closing the wallet modal.
- Mobile assertions validate tap-sized controls and tap interactions for the menu and wallet flow.
- Touch target checks enforce the existing `44px` minimum interactive sizing used throughout the component styles.

## Responsive media strategy

The current frontend does not ship breakpoint-specific raster assets. The responsive suite therefore asserts that the landing page stays fully responsive without `<img>`, `<picture>`, `srcset`, or viewport-specific image requests.

If responsive imagery is introduced later, extend the suite to assert:

- `srcset` or `<picture>` source selection at `375`, `768`, and `1440` widths
- stable aspect ratios across portrait and landscape
- no oversized image downloads on mobile
