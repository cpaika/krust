#!/bin/bash
set -e

echo "=== Port-Forward Validation Test ==="
echo ""

# Cleanup function
cleanup() {
    echo "Cleaning up..."
    kubectl delete pod validation-test --force --grace-period=0 2>/dev/null || true
    pkill -f "kubectl port-forward" 2>/dev/null || true
    pkill -f "target/debug/krust" 2>/dev/null || true
}

trap cleanup EXIT

# Start fresh
cleanup
rm -f krust.db

echo "1. Starting Krust server..."
cargo run > /tmp/krust.log 2>&1 &
SERVER_PID=$!
sleep 5

# Check server is running
if ! curl -s http://localhost:6443/healthz | grep -q "ok"; then
    echo "❌ Server failed to start"
    cat /tmp/krust.log
    exit 1
fi
echo "✓ Server is running"

# Configure kubectl
echo "2. Configuring kubectl..."
kubectl config set-cluster krust --server=http://localhost:6443 >/dev/null
kubectl config set-context krust --cluster=krust >/dev/null
kubectl config use-context krust >/dev/null
echo "✓ kubectl configured"

# Create test pod
echo "3. Creating test pod..."
cat <<EOF | kubectl apply -f -
apiVersion: v1
kind: Pod
metadata:
  name: validation-test
spec:
  containers:
  - name: web
    image: hashicorp/http-echo:latest
    args:
    - "-text=Port-Forward Works!"
    - "-listen=:8080"
    ports:
    - containerPort: 8080
EOF

# Wait for pod
echo "4. Waiting for pod to be ready..."
for i in {1..30}; do
    if kubectl get pod validation-test -o jsonpath='{.status.phase}' 2>/dev/null | grep -q "Running"; then
        echo "✓ Pod is running"
        break
    fi
    sleep 1
done

sleep 3

# Test port-forward
echo "5. Testing kubectl port-forward..."
kubectl port-forward pod/validation-test 28080:8080 > /tmp/pf.log 2>&1 &
PF_PID=$!
sleep 3

# Check if port-forward started
if ! ps -p $PF_PID > /dev/null; then
    echo "❌ Port-forward failed to start"
    cat /tmp/pf.log
    exit 1
fi

echo "6. Testing connection..."
RESPONSE=$(curl -s http://localhost:28080 2>&1 || echo "Connection failed")

if echo "$RESPONSE" | grep -q "Port-Forward Works!"; then
    echo "✅ SUCCESS! Port-forward is working!"
    echo "   Response: $RESPONSE"
else
    echo "❌ FAILED. Response: $RESPONSE"
    echo ""
    echo "Server logs:"
    tail -50 /tmp/krust.log
    echo ""
    echo "Port-forward logs:"
    cat /tmp/pf.log
    exit 1
fi

echo ""
echo "=== Validation Complete ==="