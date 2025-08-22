#!/bin/bash
set -e

echo "=== Testing Pod Proxy Feature ==="

# Clean up any existing state
pkill -f "target/debug/krust" || true
sleep 1
rm -f krust.db

# Start Krust server in background
echo "Starting Krust server..."
cargo run &
SERVER_PID=$!
sleep 3

# Verify server is running
if ! curl -s http://localhost:6443/healthz | grep -q "ok"; then
    echo "❌ Server failed to start"
    kill $SERVER_PID 2>/dev/null || true
    exit 1
fi
echo "✓ Server is running"

# Configure kubectl
echo "Configuring kubectl..."
kubectl config set-cluster krust --server=http://localhost:6443
kubectl config set-context krust --cluster=krust
kubectl config use-context krust

# Apply the demo app
echo "Deploying demo app..."
kubectl apply -f demo.yaml

# Wait for pods to be running
echo "Waiting for pods to be running..."
for i in {1..30}; do
    PHASE=$(kubectl get pods -o json | jq -r '.items[0].status.phase' 2>/dev/null || echo "")
    if [ "$PHASE" = "Running" ]; then
        echo "✓ Pod is running"
        break
    fi
    echo "  Pod phase: $PHASE (attempt $i/30)"
    sleep 2
done

if [ "$PHASE" != "Running" ]; then
    echo "❌ Pod did not reach Running state"
    kubectl get pods
    kill $SERVER_PID 2>/dev/null || true
    exit 1
fi

# Get pod name
POD_NAME=$(kubectl get pods -o json | jq -r '.items[0].metadata.name')
echo "Pod name: $POD_NAME"

# Test the proxy endpoint
echo ""
echo "Testing proxy endpoint..."
echo "Accessing pod via: http://localhost:6443/proxy/pods/default/$POD_NAME/80"

# Give the container a moment to fully start
sleep 3

# Try to access the pod through the proxy
RESPONSE=$(curl -s http://localhost:6443/proxy/pods/default/$POD_NAME/80 2>&1 || echo "FAILED")
echo "Response: $RESPONSE"

if echo "$RESPONSE" | grep -q "Hello World from Krust"; then
    echo "✅ SUCCESS! Can access pod via proxy!"
else
    echo "❌ Failed to get expected response"
    echo "Debugging info:"
    kubectl get pods
    kubectl logs $POD_NAME || true
    # Try direct Docker access
    echo ""
    echo "Checking Docker container..."
    docker ps --filter "label=io.kubernetes.pod.name=$POD_NAME"
fi

# Clean up
echo ""
echo "Cleaning up..."
kubectl delete -f demo.yaml --force --grace-period=0 || true
kill $SERVER_PID 2>/dev/null || true

echo ""
echo "Test complete!"