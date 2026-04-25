#!/bin/bash
set -e

echo "==================================="
echo "Tossd Load Testing Suite"
echo "==================================="
echo ""

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

# Check prerequisites
echo "Checking prerequisites..."

if ! command -v cargo &> /dev/null; then
    echo -e "${RED}✗ Cargo not found${NC}"
    exit 1
fi
echo -e "${GREEN}✓ Cargo found${NC}"

if ! command -v k6 &> /dev/null; then
    echo -e "${YELLOW}⚠ k6 not found - frontend tests will be skipped${NC}"
    echo "  Install: brew install k6 (macOS) or sudo apt install k6 (Linux)"
    K6_AVAILABLE=false
else
    echo -e "${GREEN}✓ k6 found${NC}"
    K6_AVAILABLE=true
fi

echo ""

# Contract load tests
echo "==================================="
echo "Running Contract Load Tests"
echo "==================================="
echo ""

echo "Running standard load tests (100-500 concurrent actors)..."
cargo test --release --manifest-path contract/Cargo.toml load_tests -- --nocapture

echo ""
read -p "Run heavy load tests (1000+ concurrent actors)? [y/N] " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]; then
    echo "Running heavy load tests..."
    cargo test --release --manifest-path contract/Cargo.toml load_tests -- --ignored --nocapture
fi

echo ""

# Frontend load tests
if [ "$K6_AVAILABLE" = true ]; then
    echo "==================================="
    echo "Running Frontend Load Tests"
    echo "==================================="
    echo ""
    
    # Check if dev server is running
    if ! curl -s http://localhost:5173 > /dev/null; then
        echo -e "${YELLOW}⚠ Frontend dev server not running on localhost:5173${NC}"
        echo "Starting dev server..."
        npm --prefix frontend run dev &
        DEV_SERVER_PID=$!
        echo "Waiting for server to start..."
        sleep 10
    else
        echo -e "${GREEN}✓ Frontend dev server is running${NC}"
        DEV_SERVER_PID=""
    fi
    
    echo ""
    echo "Running basic load test (100-1000 VUs)..."
    k6 run frontend/tests/load/basic.js
    
    echo ""
    echo "Running game flow test (100-1000 VUs)..."
    k6 run frontend/tests/load/game-flow.js
    
    echo ""
    read -p "Run stress test (up to 2000 VUs)? [y/N] " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        echo "Running stress test..."
        k6 run frontend/tests/load/stress.js
    fi
    
    # Cleanup
    if [ ! -z "$DEV_SERVER_PID" ]; then
        echo ""
        echo "Stopping dev server..."
        kill $DEV_SERVER_PID 2>/dev/null || true
    fi
fi

echo ""
echo "==================================="
echo "Load Testing Complete"
echo "==================================="
echo ""
echo "Results saved to:"
echo "  - load-test-results.json"
echo "  - game-flow-results.json"
echo "  - stress-test-results.json"
echo ""
echo "See LOAD_TESTING.md for detailed analysis guide"
