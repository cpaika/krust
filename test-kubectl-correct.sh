#!/bin/bash

echo "Testing kubectl port-forward with correct configuration"
echo "========================================================"

# Kill any existing krust process
pkill -f "target/debug/krust" 2>/dev/null

# Clean up
rm -f /tmp/krust-correct.log
rm -f /tmp/kubectl-correct.log

# Start krust with trace logging
echo "Starting krust with trace logging..."
RUST_LOG=trace cargo run > /tmp/krust-correct.log 2>&1 &
KRUST_PID=$!

# Wait for krust to start
echo "Waiting for krust to start..."
sleep 3

# Check if krust is running
if ! ps -p $KRUST_PID > /dev/null; then
    echo "Error: Krust failed to start"
    tail -20 /tmp/krust-correct.log
    exit 1
fi

echo "Krust started with PID $KRUST_PID"

# Configure kubectl to use krust
export KUBECONFIG=/tmp/krust-kubeconfig
cat > $KUBECONFIG <<EOF
apiVersion: v1
kind: Config
clusters:
- cluster:
    server: http://localhost:8080
  name: krust
contexts:
- context:
    cluster: krust
    user: krust
  name: krust
current-context: krust
users:
- name: krust
  user: {}
EOF

echo "Configured kubectl to use krust at localhost:8080"

# Create test namespace
echo "Creating test namespace..."
kubectl create namespace test-correct 2>/dev/null || true

# Create nginx pod
echo "Creating nginx pod..."
cat <<EOF | kubectl apply -f -
apiVersion: v1
kind: Pod
metadata:
  name: nginx-correct
  namespace: test-correct
spec:
  containers:
  - name: nginx
    image: nginx:latest
    ports:
    - containerPort: 80
      name: http
EOF

# Set pod status to Running directly via API
echo "Setting pod status to Running..."
curl -X PATCH http://localhost:8080/api/v1/namespaces/test-correct/pods/nginx-correct/status \
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

sleep 1

echo ""
echo "Testing port-forward with high verbosity..."
echo "Running: kubectl port-forward -n test-correct nginx-correct 8889:80 -v=8"
echo ""

# Run kubectl with high verbosity
timeout 10 kubectl port-forward -n test-correct nginx-correct 8889:80 -v=8 2>&1 | tee /tmp/kubectl-correct.log &
KUBECTL_PID=$!

# Give kubectl time to establish connection
sleep 3

# Test the connection
echo ""
echo "Testing HTTP connection through port-forward..."
if timeout 2 curl -s http://localhost:8889 > /dev/null 2>&1; then
    echo "✓ Port-forward is working!"
    curl -s http://localhost:8889 | head -5
else
    echo "✗ Port-forward connection failed"
fi

# Wait a bit to analyze
sleep 2

# Kill kubectl
kill $KUBECTL_PID 2>/dev/null

echo ""
echo "=== Analysis ==="
echo ""

echo "Kubectl negotiation:"
grep -i "negotiat\|protocol\|SPDY" /tmp/kubectl-correct.log | head -5

echo ""
echo "Krust WebSocket handling:"
grep -E "(exact.*session|Acknowledgments|Frame:|First data|Keep-alive)" /tmp/krust-correct.log | tail -20

echo ""
echo "Connection status:"
grep -E "(Connected to container|Session.*complete|error|failed)" /tmp/krust-correct.log | tail -10

# Cleanup
echo ""
echo "Cleaning up..."
kill $KRUST_PID 2>/dev/null
kubectl delete namespace test-correct 2>/dev/null || true
rm -f $KUBECONFIG

echo ""
echo "Full logs saved to:"
echo "  - /tmp/krust-correct.log"
echo "  - /tmp/kubectl-correct.log"