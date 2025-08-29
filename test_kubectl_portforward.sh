#!/bin/bash
set -e

echo "=== Testing kubectl port-forward with Krust ==="

# Clean up any existing processes
pkill -f "target/debug/krust" || true
sleep 1

# Clean database
rm -f krust.db

# Start Krust server in background
echo "Starting Krust server..."
cargo run &
SERVER_PID=$!
sleep 3

# Check server is running
if ! curl -s http://localhost:6443/healthz | grep -q "ok"; then
    echo "❌ Server failed to start"
    kill $SERVER_PID 2>/dev/null || true
    exit 1
fi
echo "✓ Server is running"

# Configure kubectl
kubectl config set-cluster krust --server=http://localhost:6443
kubectl config set-context krust --cluster=krust
kubectl config use-context krust

# Create a test pod
cat <<EOF | kubectl apply -f -
apiVersion: v1
kind: Pod
metadata:
  name: test-pf-pod
  namespace: default
spec:
  containers:
  - name: http-server
    image: hashicorp/http-echo:latest
    args:
    - "-text=Hello from kubectl port-forward"
    - "-listen=:8080"
    ports:
    - containerPort: 8080
EOF

echo "Waiting for pod to be ready..."
for i in {1..30}; do
    if kubectl get pod test-pf-pod -o jsonpath='{.status.phase}' | grep -q "Running"; then
        echo "✓ Pod is running"
        break
    fi
    sleep 1
done

# Give container time to start
sleep 3

# Test kubectl port-forward
echo "Testing kubectl port-forward..."
kubectl port-forward pod/test-pf-pod 9999:8080 &
PF_PID=$!
sleep 2

# Test the connection
echo "Testing forwarded connection..."
RESPONSE=$(curl -s http://localhost:9999 2>&1 || echo "Connection failed")
echo "Response: $RESPONSE"

if echo "$RESPONSE" | grep -q "Hello from kubectl port-forward"; then
    echo "✅ kubectl port-forward is working!"
    EXIT_CODE=0
else
    echo "❌ kubectl port-forward failed"
    echo "Debug: Checking pod status..."
    kubectl get pod test-pf-pod -o yaml
    echo "Debug: Checking server logs..."
    tail -n 50 server.log 2>/dev/null || true
    EXIT_CODE=1
fi

# Cleanup
kill $PF_PID 2>/dev/null || true
kubectl delete pod test-pf-pod --force --grace-period=0 2>/dev/null || true
kill $SERVER_PID 2>/dev/null || true

exit $EXIT_CODE