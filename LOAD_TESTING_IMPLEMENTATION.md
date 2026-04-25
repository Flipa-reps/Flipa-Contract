# Load Testing Implementation Summary

## Overview

Implemented a comprehensive distributed load generation framework simulating hundreds to thousands of concurrent actors for the Tossd coinflip game.

## What Was Implemented

### 1. Contract Load Tests (`contract/tests/load_tests.rs`)

**Features:**
- Concurrent actor simulation using Rust threads
- Comprehensive metrics collection (throughput, latency percentiles, success/failure rates)
- Multiple test scenarios:
  - 100 concurrent actors (baseline)
  - 500 concurrent actors (medium load)
  - 1000 concurrent actors (heavy load, ignored by default)
  - Reserve depletion stress test

**Metrics Tracked:**
- Total operations
- Success/failure counts
- Duration and throughput (ops/sec)
- P95 and P99 latency percentiles

**Key Implementation Details:**
- Uses `Arc<Mutex<>>` for thread-safe result aggregation
- Measures per-operation latency
- Tests reserve protection under concurrent load
- Validates no negative reserve balances

### 2. Frontend Load Tests (k6)

**Location:** `frontend/tests/load/`

**Test Files:**

#### a. Basic Load Test (`basic.js`)
- Simulates 100 → 500 → 1000 virtual users
- Tests homepage load and response times
- Thresholds: p95 < 500ms, p99 < 1000ms, error rate < 5%
- Exports results to JSON

#### b. Game Flow Test (`game-flow.js`)
- Full game lifecycle simulation (connect → start → reveal → cash out)
- Ramps up to 1000 concurrent players
- Tracks game-specific metrics:
  - Game start latency
  - Cash out latency
  - End-to-end flow duration
- Thresholds: flow p95 < 3000ms, p99 < 5000ms

#### c. Stress Test (`stress.js`)
- Arrival-rate based executor (10 → 1500 req/sec)
- Tests reserve depletion and error handling
- Tracks:
  - Reserve depletion errors
  - Timeout errors
  - Successful games counter
  - Active users gauge
- Higher error tolerance (15%) for stress conditions

#### d. Smoke Test (`smoke.js`)
- Quick validation test (10 VUs for 1 minute)
- Generates HTML report
- Use before running full load tests

### 3. Automation and Tooling

#### NPM Scripts (`package.json`)
```bash
npm run contract:load          # Standard contract load tests
npm run contract:load:heavy    # Heavy load tests (1000+ actors)
npm run load:basic             # Frontend basic load test
npm run load:game-flow         # Frontend game flow test
npm run load:stress            # Frontend stress test
npm run load:all               # Run all frontend tests
```

#### Interactive Script (`scripts/run-load-tests.sh`)
- Checks prerequisites (cargo, k6)
- Runs contract tests with progress indicators
- Optionally starts dev server
- Runs frontend tests sequentially
- Cleans up processes
- Color-coded output

#### GitHub Actions Workflow (`.github/workflows/load-tests.yml`)
- Automated CI/CD integration
- Runs on push to main and PRs
- Separate jobs for contract and frontend tests
- Uploads test artifacts
- Comments PR with results summary
- Installs k6 on Ubuntu runner

### 4. Documentation

#### LOAD_TESTING.md
Comprehensive guide including:
- Prerequisites and setup
- How to run each test type
- Test scenario descriptions
- Performance baselines and thresholds
- Metrics interpretation guide
- Success criteria
- Troubleshooting common issues
- CI/CD integration examples
- Scaling recommendations

#### README.md Updates
- Added load testing section
- Quick start commands
- Link to detailed documentation

## Performance Baselines

### Contract (Expected)
| Scenario | Throughput | p95 Latency | p99 Latency |
|----------|-----------|-------------|-------------|
| 100 concurrent | ~500 ops/s | <50ms | <100ms |
| 500 concurrent | ~1000 ops/s | <100ms | <200ms |
| 1000 concurrent | ~1500 ops/s | <200ms | <400ms |

### Frontend (Thresholds)
| Scenario | Target VUs | p95 Response | Error Rate |
|----------|-----------|--------------|------------|
| Basic load | 1000 | <500ms | <5% |
| Game flow | 1000 | <3000ms | <5% |
| Stress test | 1500 | <2000ms | <15% |

## Test Execution Flow

### Contract Tests
1. Create Harness with mock environment
2. Fund contract with reserves
3. Spawn N threads, each simulating a player
4. Each thread:
   - Measures operation start time
   - Executes game flow (start → reveal → cash out)
   - Records latency and success/failure
5. Aggregate results with thread-safe mutex
6. Calculate metrics (throughput, percentiles)
7. Print formatted summary
8. Assert success criteria

### Frontend Tests
1. k6 spawns virtual users (VUs)
2. Each VU executes test scenario:
   - Basic: GET homepage
   - Game flow: Full game lifecycle
   - Stress: Random wagers with high concurrency
3. k6 collects metrics automatically
4. Custom metrics tracked via Rate, Trend, Counter, Gauge
5. Thresholds evaluated in real-time
6. Results exported to JSON
7. Summary handler formats output

## Key Features

### Distributed Load Generation
- Contract: Native Rust threads for true parallelism
- Frontend: k6 VUs for efficient HTTP load generation
- Both support 1000+ concurrent actors

### Comprehensive Metrics
- Latency percentiles (p95, p99)
- Throughput (ops/sec, req/sec)
- Error rates and categorization
- Custom business metrics (successful games, reserve errors)

### Realistic Scenarios
- Full game lifecycle simulation
- Reserve depletion testing
- Timeout and error handling
- Variable wager amounts
- Random side selection

### CI/CD Integration
- Automated test execution
- Result artifacts
- PR comments with summaries
- Configurable thresholds

## Files Created/Modified

### New Files
- `contract/tests/load_tests.rs` - Contract load tests
- `frontend/tests/load/stress.js` - Stress test
- `frontend/tests/load/smoke.js` - Smoke test
- `LOAD_TESTING.md` - Comprehensive documentation
- `scripts/run-load-tests.sh` - Interactive test runner
- `.github/workflows/load-tests.yml` - CI/CD workflow

### Modified Files
- `frontend/tests/load/basic.js` - Enhanced with metrics
- `frontend/tests/load/game-flow.js` - Full lifecycle simulation
- `package.json` - Added load test scripts
- `README.md` - Added load testing section

## Usage Examples

### Quick Start
```bash
# Run everything
./scripts/run-load-tests.sh

# Contract only
npm run contract:load

# Frontend only (dev server must be running)
npm run load:all
```

### Individual Tests
```bash
# Contract - specific scenario
cargo test --release test_100_concurrent_actors -- --nocapture

# Frontend - specific test
k6 run frontend/tests/load/game-flow.js
```

### CI/CD
Tests run automatically on:
- Push to main
- Pull requests
- Manual workflow dispatch

## Next Steps

1. **Run baseline tests** to establish actual performance characteristics
2. **Tune thresholds** based on real results
3. **Identify bottlenecks** using metrics
4. **Optimize** contract or frontend
5. **Re-test** to validate improvements
6. **Document** final performance in LOAD_TESTING.md

## Notes

- Contract tests use mock Soroban environment (not real network)
- Frontend tests require running dev server
- k6 must be installed separately
- Heavy load tests (1000+) are ignored by default to avoid CI timeouts
- Stress tests intentionally push system to failure to test error handling
