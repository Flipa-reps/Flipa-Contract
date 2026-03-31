# Mutation Testing Implementation (#380) - add-mutation-testing-test-suite-quality

## Completed ✅
- [x] Create branch `add-mutation-testing-test-suite-quality`
- [x] Install Rust toolchain (rustup)
- [x] Frontend Stryker deps + stryker.conf.json + npm scripts
- [x] Contract cargo clean

## Remaining Steps ⏳
1. Install cargo-mutants: `cargo install cargo-mutants`
2. Run baseline contract mutations: `cargo mutants`
3. Install Stryker (frontend): `cd frontend && npm i -D @stryker-mutator/core @stryker-mutator/vitest-runner @stryker-mutator/typescript-checker @stryker-mutator/html-reporter`
4. Create frontend/stryker.conf.json
5. Run baseline frontend mutations: `cd frontend && npx stryker run`
6. Analyze surviving mutants (contract + frontend)
7. Add targeted tests to kill top mutants (aim 80% score)
8. Generate reports: MUTATION_REPORT_CONTRACT.md, MUTATION_REPORT_FRONTEND.md
9. Update README.md, package.json scripts, Cargo.toml
10. Full validation: `cargo test`, `npm test`, `npm run test:e2e`
11. Commit + PR with #380 reference

**Next:** Install tools and run baseline mutations.

