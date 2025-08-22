#!/bin/bash
echo "Testing kubectl commands for apps/v1 warnings..."

# Start server
rm -f krust.db
cargo run > /dev/null 2>&1 &
SERVER_PID=$!
sleep 3

# Test commands
echo -n "Testing 'kubectl get nodes': "
OUTPUT=$(kubectl --server=http://localhost:6443 get nodes 2>&1)
if echo "$OUTPUT" | grep -q "couldn't get resource list for apps/v1"; then
    echo "❌ WARNING FOUND"
    kill $SERVER_PID
    exit 1
else
    echo "✅ No warning"
fi

echo -n "Testing 'kubectl get pods': "
OUTPUT=$(kubectl --server=http://localhost:6443 get pods 2>&1)
if echo "$OUTPUT" | grep -q "couldn't get resource list for apps/v1"; then
    echo "❌ WARNING FOUND"
    kill $SERVER_PID
    exit 1
else
    echo "✅ No warning"
fi

echo -n "Testing 'kubectl get deployments': "
OUTPUT=$(kubectl --server=http://localhost:6443 get deployments 2>&1)
if echo "$OUTPUT" | grep -q "couldn't get resource list for apps/v1"; then
    echo "❌ WARNING FOUND"
    kill $SERVER_PID
    exit 1
else
    echo "✅ No warning"
fi

kill $SERVER_PID
echo ""
echo "✅ All tests passed - NO apps/v1 warnings!"