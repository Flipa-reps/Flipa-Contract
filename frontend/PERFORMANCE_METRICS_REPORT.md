# Frontend Performance Metrics Report

References: #349

## Scope

- TransactionHistory render performance with 120 records.
- GameFlowSteps render latency.
- Modal open/close render timing.
- Animation render timing for CoinFlip.
- Core Web Vitals budget validation (LCP, FID, CLS).

## Budgets

- TransactionHistory (120 rows): <= 90ms render in jsdom test runtime.
- GameFlowSteps: <= 40ms render.
- Modal open: <= 55ms render.
- Modal close: <= 35ms render.
- CoinFlip animation render: <= 35ms.

Core Web Vitals:

- LCP good threshold: <= 2500ms.
- FID good threshold: <= 100ms.
- CLS good threshold: <= 0.1.

## Bottleneck Findings

- TransactionHistory performs per-render sorting on full dataset before slicing.
- Date formatting is executed on every row render and can become expensive for larger lists.
- Modal mount and portal setup are lightweight, but focus trap querying can scale with deeply nested content.
- CoinFlip animation itself is GPU-friendly; runtime cost is primarily React render and style application.

## Optimization Notes

- Consider memoizing sorted transaction data with useMemo by records identity.
- Consider pre-formatting timestamps/amounts at data ingestion or caching derived values.
- Consider list virtualization for histories beyond 200 rows.

## Test Commands

- npm run test:perf
- npm run test:unit

## CI Integration

The workflow [frontend-tests.yml](../.github/workflows/frontend-tests.yml) executes performance tests on every frontend-affecting pull request.
