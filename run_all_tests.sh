#!/bin/bash
set -e

echo "🧹 Cleaning build artifacts..."
cargo clean

echo "🔨 Building project..."
cargo build --release

echo "🚀 Starting server in background..."
cargo run --release > server_test.log 2>&1 &
SERVER_PID=$!
echo "Server PID: $SERVER_PID"

# Wait for server to start
echo "⏳ Waiting for server to start..."
for i in {1..30}; do
    if curl -s http://localhost:6443/livez > /dev/null 2>&1; then
        echo "✅ Server is running"
        break
    fi
    sleep 1
done

# Run tests
echo ""
echo "🧪 Running tests..."
echo "=================="

# Run main tests
echo "Running integration tests..."
cargo test --test integration_test -- --nocapture

echo "Running edge case tests..."
cargo test --test edge_cases -- --nocapture

echo "Running TDD validation tests..."
cargo test --test tdd_validation -- --nocapture

# Kill server
echo ""
echo "🛑 Stopping server..."
kill $SERVER_PID 2>/dev/null || true

echo ""
echo "✅ All tests completed!"
