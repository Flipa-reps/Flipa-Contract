# Load Testing Quick Reference

## 🚀 Quick Start

```bash
# Run everything
./scripts/run-load-tests.sh

# Contract tests only
npm run contract:load

# Frontend tests only (requires dev server)
npm run load:all
```

## 📋 Test Commands

### Contract Tests
```bash
# Standard load (100-500 actors)
cargo test --release --test load_tests -- --nocapture

# Heavy load (1000+ actors)
cargo test --release --test load_tests -- --ignored --nocapture

# Specific test
cargo test --release test_100_concurrent_actors -- --nocapture
```

### Frontend Tests
```bash
# Start dev server first
npm run dev

# Then in another terminal:
k6 run frontend/tests/load/basic.js       # Basic load
k6 run frontend/tests/load/game-flow.js   # Game flow
k6 run frontend/tests/load/stress.js      # Stress test
k6 run frontend/tests/load/smoke.js       # Quick smoke test
```

## 📊 Test Scenarios

| Test | Type | Load | Duration | Purpose |
|------|------|------|----------|---------|
| 100 concurrent | Contract | 100 threads | ~1-2s | Baseline |
| 500 concurrent | Contract | 500 threads | ~5-10s | Medium load |
| 1000 concurrent | Contract | 1000 threads | ~10-20s | Heavy load |
| Reserve depletion | Contract | 100 threads | ~2-5s | Economic stress |
| Basic load | Frontend | 100-1000 VUs | 3.5 min | HTTP load |
| Game flow | Frontend | 100-1000 VUs | 2.5 min | Full lifecycle |
| Stress | Frontend | 10-1500 req/s | 9 min | Breaking point |
| Smoke | Frontend | 10 VUs | 1 min | Quick validation |

## 🎯 Success Criteria

### Contract Tests
- ✅ Success rate ≥ 95%
- ✅ p95 latency < 200ms
- ✅ p99 latency < 400ms
- ✅ No negative reserves
- ✅ No panics or crashes

### Frontend Tests
- ✅ Error rate < 5% (load tests)
- ✅ Error rate < 15% (stress test)
- ✅ p95 response < 500ms (basic)
- ✅ p95 flow < 3000ms (game flow)
- ✅ Graceful degradation under stress

## 📈 Key Metrics

### Contract
- **Throughput**: Operations per second
- **Latency p95/p99**: 95th/99th percentile response time
- **Success Rate**: Percentage of successful operations
- **Reserve Balance**: Must never go negative

### Frontend
- **Response Time**: HTTP request duration
- **Error Rate**: Failed requests / total requests
- **Game Flow Duration**: End-to-end game time
- **Concurrent Games**: Active game sessions
- **Reserve Errors**: Rejections due to insufficient reserves

## 🔧 Troubleshooting

| Issue | Likely Cause | Solution |
|-------|--------------|----------|
| High error rate | Insufficient reserves | Increase `fund()` amount |
| High latency | Too much concurrency | Reduce load or optimize |
| Reserve depletion | Expected in stress tests | Verify graceful handling |
| Timeouts | Network/server overload | Increase timeout or reduce load |
| k6 not found | Not installed | `brew install k6` or `apt install k6` |
| Dev server not running | Forgot to start | `npm run dev` |

## 📁 Output Files

```
load-test-results.json       # Basic load test results
game-flow-results.json       # Game flow test results
stress-test-results.json     # Stress test results
smoke-test-report.html       # Smoke test HTML report
```

## 🔄 CI/CD

Tests run automatically on:
- Push to `main`
- Pull requests
- Manual workflow dispatch

View results:
- GitHub Actions → Load and Stress Tests
- Artifacts → Download JSON results
- PR comments → Summary

## 📚 Documentation

- **LOAD_TESTING.md** - Comprehensive guide
- **LOAD_TESTING_IMPLEMENTATION.md** - Implementation details
- **LOAD_TESTING_ARCHITECTURE.md** - Visual diagrams
- **README.md** - Project overview

## 🎨 NPM Scripts

```json
{
  "contract:load": "Run standard contract load tests",
  "contract:load:heavy": "Run heavy contract load tests",
  "load:basic": "Run basic frontend load test",
  "load:game-flow": "Run game flow load test",
  "load:stress": "Run stress test",
  "load:all": "Run all frontend tests"
}
```

## 💡 Tips

1. **Start small**: Run smoke test first
2. **Baseline**: Establish performance before optimizing
3. **Iterate**: Test → Measure → Optimize → Repeat
4. **Monitor**: Watch metrics during tests
5. **Document**: Record baselines and improvements
6. **CI/CD**: Automate to catch regressions

## 🚨 Common Mistakes

- ❌ Running heavy tests without `--release` flag
- ❌ Forgetting to start dev server for frontend tests
- ❌ Not funding reserves before contract tests
- ❌ Comparing debug vs release build performance
- ❌ Running stress tests in CI without timeout limits

## ✅ Best Practices

- ✅ Use `--release` for contract tests
- ✅ Run smoke test before full load tests
- ✅ Monitor system resources during tests
- ✅ Document performance baselines
- ✅ Set realistic thresholds
- ✅ Test error handling, not just happy path
- ✅ Use `--nocapture` to see detailed output

## 🔗 Quick Links

- [k6 Documentation](https://k6.io/docs/)
- [Soroban Testing](https://soroban.stellar.org/docs/how-to-guides/testing)
- [GitHub Actions](https://docs.github.com/en/actions)

---

**Need help?** See LOAD_TESTING.md for detailed documentation.
