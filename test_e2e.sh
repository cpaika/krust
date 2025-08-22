#!/bin/bash
set -e

echo "=== Testing Krust with Real Containers ==="

# Clean up
pkill -f "target/debug/krust" 2>/dev/null || true
rm -f krust.db

# Start server
echo "Starting Krust server..."
cargo run > server_e2e.log 2>&1 &
SERVER_PID=$!
sleep 5

# Check server is healthy
if ! curl -s http://localhost:6443/healthz | grep -q "ok"; then
    echo "❌ Server failed to start"
    kill $SERVER_PID 2>/dev/null
    exit 1
fi
echo "✓ Server is healthy"

# Test OpenAPI endpoint
echo -n "Testing OpenAPI v2 endpoint: "
if curl -s http://localhost:6443/openapi/v2 | grep -q '"swagger":"2.0"'; then
    echo "✓ Works"
else
    echo "❌ Failed"
fi

# Create a test pod
cat > test-busybox.yaml <<EOF
apiVersion: v1
kind: Pod
metadata:
  name: busybox-e2e
  labels:
    test: e2e
spec:
  containers:
  - name: busybox
    image: busybox:1.36
    command: ["sh", "-c", "echo 'Container started!' && sleep 30"]
EOF

echo "Creating test pod..."
kubectl --server=http://localhost:6443 apply -f test-busybox.yaml --validate=false

# Wait for pod to be scheduled
echo -n "Waiting for pod to be scheduled"
for i in {1..30}; do
    if kubectl --server=http://localhost:6443 get pod busybox-e2e -o json 2>/dev/null | grep -q '"phase":"Running"'; then
        echo " ✓"
        break
    elif kubectl --server=http://localhost:6443 get pod busybox-e2e -o json 2>/dev/null | grep -q '"phase":"Scheduled"'; then
        echo -n "."
    else
        echo -n "."
    fi
    sleep 1
done

# Check pod status
echo ""
echo "Pod status:"
kubectl --server=http://localhost:6443 get pod busybox-e2e -o wide 2>/dev/null || true

# Check container status conditions
echo ""
echo "Checking pod conditions:"
kubectl --server=http://localhost:6443 get pod busybox-e2e -o json 2>/dev/null | jq '.status.conditions[] | {type: .type, status: .status}' 2>/dev/null || true

# Try to get logs
echo ""
echo "Attempting to get pod logs:"
kubectl --server=http://localhost:6443 logs busybox-e2e 2>&1 | head -5 || true

# Check if container is actually running
echo ""
echo "Docker container status:"
docker ps --filter "label=io.kubernetes.pod.name=busybox-e2e" --format "table {{.Names}}\t{{.Status}}\t{{.Image}}" 2>/dev/null || echo "No Docker containers found"

# Delete pod
echo ""
echo "Deleting pod..."
kubectl --server=http://localhost:6443 delete pod busybox-e2e 2>/dev/null

# Wait for cleanup
sleep 3

# Verify cleanup
echo "Verifying cleanup..."
if docker ps -a --filter "label=io.kubernetes.pod.name=busybox-e2e" --format "{{.Names}}" 2>/dev/null | grep -q .; then
    echo "⚠ Container still exists"
else
    echo "✓ Container cleaned up"
fi

# Clean up
kill $SERVER_PID 2>/dev/null
rm -f test-busybox.yaml

echo ""
echo "✅ E2E test completed!"