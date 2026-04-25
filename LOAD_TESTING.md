# Load and Stress Testing Guide

## Overview

This framework simulates hundreds to thousands of concurrent users to validate performance, stability, and error handling under load.

## Prerequisites

### Contract Tests
- Rust toolchain
- `cargo test` working

### Frontend Tests
- k6 installed: `brew install k6` (macOS) or `sudo apt install k6` (Linux)
- Frontend dev server running on `http://localhost:5173`

## Contract Load Tests

Located in: `contract/load_tests.rs`

### Run Tests

```bash
# Run standard load tests (100-500 concurrent actors)
cargo test --release --test load_tests

# Run heavy load tests (1000+ concurrent actors)
cargo test --release --test load_tests -- --ignored --nocapture
```

### Test Scenarios

1. **100 Concurrent Actors** - Baseline concurrency test
   - 100 simultaneous game sessions
   - Measures throughput and latency percentiles
   - Expected: >95% success rate

2. **500 Concurrent Actors** - Medium load
   - 500 simultaneous game sessions
   - Tests contract scalability
   - Expected: >95% success rate

3. **1000 Concurrent Actors** - Heavy load (ignored by default)
   - 1000 simultaneous game sessions
   - Stress test for maximum throughput
   - Expected: >95% success rate

4. **Reserve Depletion Concurrent** - Economic stress test
   - 100 concurrent actors with limited reserves
   - Validates reserve protection logic
   - Expected: Some rejections, no negative reserves

### Metrics Collected

- **Total operations**: Number of game sessions attempted
- **Success/Failed**: Breakdown of outcomes
- **Duration**: Total test execution time
- **Throughput**: Operations per second
- **Latency p95/p99**: 95th and 99th percentile response times

## Frontend Load Tests

Located in: `frontend/tests/load/`

### Run Tests

```bash
# Start frontend dev server first
npm run dev

# In another terminal, run load tests:

# Basic load test (100-1000 VUs)
k6 run frontend/tests/load/basic.js

# Game flow test (100-1000 VUs with full game lifecycle)
k6 run frontend/tests/load/game-flow.js

# Stress test (up to 2000 VUs with reserve depletion)
k6 run frontend/tests/load/stress.js
```

### Test Scenarios

#### 1. Basic Load Test (`basic.js`)
- **Ramp-up**: 100 → 500 → 1000 VUs over 3.5 minutes
- **Sustained**: 1000 VUs for 1 minute
- **Tests**: Homepage load, response times
- **Thresholds**:
  - p95 < 500ms
  - p99 < 1000ms
  - Error rate < 5%

#### 2. Game Flow Test (`game-flow.js`)
- **Ramp-up**: 100 → 500 → 1000 VUs over 2.5 minutes
- **Tests**: Full game lifecycle (connect → start → reveal → cash out)
- **Metrics**:
  - Game start latency
  - Cash out latency
  - End-to-end flow duration
- **Thresholds**:
  - Game flow p95 < 3000ms
  - Game flow p99 < 5000ms
  - Error rate < 5%

#### 3. Stress Test (`stress.js`)
- **Ramp-up**: 10 → 100 → 500 → 1000 → 1500 requests/sec
- **Duration**: 9 minutes
- **Tests**: Reserve depletion, timeout handling, error recovery
- **Metrics**:
  - Reserve depletion errors
  - Timeout errors
  - Successful games counter
- **Thresholds**:
  - Error rate < 15% (higher tolerance for stress)
  - p95 < 2000ms
  - Successful games > 1000

### Output Files

All tests generate JSON result files:
- `load-test-results.json` - Basic load test results
- `game-flow-results.json` - Game flow test results
- `stress-test-results.json` - Stress test results

## Performance Baselines

### Contract Performance (Release Build)

| Scenario | Throughput | p95 Latency | p99 Latency |
|----------|-----------|-------------|-------------|
| 100 concurrent | ~500 ops/s | <50ms | <100ms |
| 500 concurrent | ~1000 ops/s | <100ms | <200ms |
| 1000 concurrent | ~1500 ops/s | <200ms | <400ms |

### Frontend Performance

| Scenario | Target VUs | p95 Response | Error Rate |
|----------|-----------|--------------|------------|
| Basic load | 1000 | <500ms | <5% |
| Game flow | 1000 | <3000ms | <5% |
| Stress test | 1500 | <2000ms | <15% |

## Interpreting Results

### Success Criteria

✅ **Pass**: 
- Success rate ≥ 95% (load tests)
- Success rate ≥ 85% (stress tests)
- Latency within thresholds
- No reserve balance violations

⚠️ **Warning**:
- Success rate 90-95%
- Latency approaching thresholds
- Occasional reserve rejections

❌ **Fail**:
- Success rate < 90%
- Latency exceeding thresholds
- Negative reserve balances
- Crashes or panics

### Common Issues

1. **High error rate**: Check reserve funding, increase initial balance
2. **High latency**: Reduce concurrent load or optimize contract logic
3. **Reserve depletion**: Expected in stress tests, verify graceful handling
4. **Timeouts**: Increase timeout thresholds or reduce load

## Continuous Integration

Add to CI pipeline:

```yaml
# .github/workflows/load-tests.yml
- name: Contract Load Tests
  run: cargo test --release --test load_tests

- name: Frontend Load Tests
  run: |
    npm run dev &
    sleep 5
    k6 run --quiet frontend/tests/load/basic.js
```

## Scaling Recommendations

Based on test results:

- **< 100 concurrent users**: Single instance sufficient
- **100-500 concurrent users**: Consider horizontal scaling
- **500-1000 concurrent users**: Load balancing recommended
- **> 1000 concurrent users**: Distributed architecture required

## Next Steps

1. Run baseline tests to establish performance characteristics
2. Identify bottlenecks using metrics
3. Optimize contract or frontend based on results
4. Re-run tests to validate improvements
5. Document final performance characteristics
