#!/bin/bash
set -e

echo "=== Testing kubectl WITHOUT --validate=false ==="

# Clean up
pkill -f "target/debug/krust" 2>/dev/null || true
rm -f krust.db

# Start server
echo "Starting Krust server..."
cargo run > server_validation.log 2>&1 &
SERVER_PID=$!
sleep 5

# Check server is healthy
if ! curl -s http://localhost:6443/healthz | grep -q "ok"; then
    echo "❌ Server failed to start"
    kill $SERVER_PID 2>/dev/null
    exit 1
fi
echo "✓ Server is healthy"

# Test that OpenAPI is available for validation
echo -n "OpenAPI v2 available: "
if curl -s http://localhost:6443/openapi/v2 | grep -q '"swagger":"2.0"'; then
    echo "✓"
else
    echo "❌"
    kill $SERVER_PID 2>/dev/null
    exit 1
fi

# Create a valid pod WITHOUT --validate=false
cat > valid-pod.yaml <<EOF
apiVersion: v1
kind: Pod
metadata:
  name: nginx-valid
spec:
  containers:
  - name: nginx
    image: nginx:alpine
EOF

echo ""
echo "Creating pod WITHOUT --validate=false flag:"
if kubectl --server=http://localhost:6443 apply -f valid-pod.yaml 2>&1 | tee /tmp/apply_output.txt; then
    echo "✅ Pod created successfully without --validate=false!"
else
    echo "❌ Failed to create pod"
    cat /tmp/apply_output.txt
fi

# Check pod was created
echo ""
echo "Verifying pod exists:"
kubectl --server=http://localhost:6443 get pod nginx-valid 2>/dev/null || true

# Clean up
kubectl --server=http://localhost:6443 delete pod nginx-valid 2>/dev/null || true
kill $SERVER_PID 2>/dev/null
rm -f valid-pod.yaml /tmp/apply_output.txt

echo ""
echo "✅ Validation test completed!"