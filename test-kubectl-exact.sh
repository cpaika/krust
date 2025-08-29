#!/bin/bash

echo "Testing kubectl exact port-forward implementation"
echo "================================================="

# Kill any existing krust process
pkill -f "target/debug/krust" 2>/dev/null

# Clean up
rm -f /tmp/krust-exact.log
rm -f /tmp/kubectl-exact.log

# Start krust with trace logging
echo "Starting krust with trace logging..."
RUST_LOG=trace cargo run > /tmp/krust-exact.log 2>&1 &
KRUST_PID=$!

# Wait for krust to start
echo "Waiting for krust to start..."
sleep 3

# Check if krust is running
if ! ps -p $KRUST_PID > /dev/null; then
    echo "Error: Krust failed to start"
    tail -20 /tmp/krust-exact.log
    exit 1
fi

echo "Krust started with PID $KRUST_PID"

# Create test namespace
echo "Creating test namespace..."
kubectl create namespace test-exact 2>/dev/null || true

# Create nginx pod
echo "Creating nginx pod..."
cat <<EOF | kubectl apply -f -
apiVersion: v1
kind: Pod
metadata:
  name: nginx-exact
  namespace: test-exact
spec:
  containers:
  - name: nginx
    image: nginx:latest
    ports:
    - containerPort: 80
      name: http
EOF

# Wait for pod to be ready
echo "Waiting for pod to be ready..."
kubectl wait --for=condition=Ready pod/nginx-exact -n test-exact --timeout=30s

# Set pod status to Running
echo "Setting pod status to Running..."
curl -X PATCH http://localhost:8080/api/v1/namespaces/test-exact/pods/nginx-exact/status \
  -H "Content-Type: application/json" \
  -d '{
    "status": {
      "phase": "Running",
      "conditions": [
        {
          "type": "Ready",
          "status": "True"
        }
      ]
    }
  }' -s > /dev/null

echo ""
echo "Testing port-forward with trace logging..."
echo "Running: kubectl port-forward -n test-exact nginx-exact 8888:80 -v=9"
echo ""

# Run kubectl with maximum verbosity
timeout 10 kubectl port-forward -n test-exact nginx-exact 8888:80 -v=9 2>&1 | tee /tmp/kubectl-exact.log &
KUBECTL_PID=$!

# Give kubectl time to establish connection
sleep 3

# Test the connection
echo ""
echo "Testing HTTP connection through port-forward..."
if curl -s http://localhost:8888 > /dev/null 2>&1; then
    echo "✓ Port-forward is working!"
    curl -s http://localhost:8888 | head -5
else
    echo "✗ Port-forward failed"
fi

# Wait a bit more to see if connection stays stable
sleep 2

# Kill kubectl
kill $KUBECTL_PID 2>/dev/null

echo ""
echo "Analyzing logs..."
echo ""

echo "=== Kubectl protocol details ==="
grep -E "(Negotiated|protocol|SPDY|portforward)" /tmp/kubectl-exact.log | head -10

echo ""
echo "=== Krust trace logs (WebSocket handling) ==="
grep -E "(WebSocket|Frame|stream|ack|init|Received.*bytes|Sending.*bytes)" /tmp/krust-exact.log | tail -30

echo ""
echo "=== Krust connection logs ==="
grep -E "(session|Connected|container|Started)" /tmp/krust-exact.log | tail -20

# Cleanup
echo ""
echo "Cleaning up..."
kill $KRUST_PID 2>/dev/null
kubectl delete namespace test-exact 2>/dev/null || true

echo ""
echo "Full logs saved to:"
echo "  - /tmp/krust-exact.log (krust trace logs)"
echo "  - /tmp/kubectl-exact.log (kubectl verbose logs)"
echo ""
echo "To see detailed frame data:"
echo "  grep 'Frame:' /tmp/krust-exact.log"