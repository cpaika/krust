#!/bin/bash
set -e

echo "=== Port-Forward Test ==="

# Clean up
pkill -f "target/debug/krust" 2>/dev/null || true
pkill -f "kubectl port-forward" 2>/dev/null || true
kubectl delete pod test-nginx --force --grace-period=0 2>/dev/null || true
sleep 1

# Start server
echo "Starting krust server..."
./target/debug/krust >/tmp/krust.log 2>&1 &
SERVER_PID=$!
sleep 2

# Verify server is up
if ! curl -s http://localhost:6443/healthz | grep -q ok; then
    echo "❌ Server failed to start"
    tail -20 /tmp/krust.log
    exit 1
fi
echo "✓ Server is running"

# Create a simple test pod
echo "Creating test pod..."
cat <<EOF | kubectl apply -f -
apiVersion: v1
kind: Pod
metadata:
  name: test-nginx
  namespace: default
spec:
  containers:
  - name: nginx
    image: nginx:alpine
    ports:
    - containerPort: 80
EOF

# Wait for pod to be running
echo "Waiting for pod to be running..."
for i in {1..60}; do
    STATUS=$(kubectl get pod test-nginx -o jsonpath='{.status.phase}' 2>/dev/null)
    if [ "$STATUS" = "Running" ]; then
        echo "✓ Pod is running"
        break
    elif [ "$STATUS" = "Failed" ]; then
        echo "❌ Pod failed to start"
        kubectl describe pod test-nginx | tail -20
        kill $SERVER_PID 2>/dev/null
        exit 1
    fi
    echo -n "."
    sleep 1
done

if [ "$STATUS" != "Running" ]; then
    echo "❌ Pod failed to become ready"
    kubectl get pod test-nginx
    kill $SERVER_PID 2>/dev/null
    exit 1
fi

# Test port-forward
echo "Starting port-forward..."
kubectl port-forward pod/test-nginx 28080:80 2>&1 &
PF_PID=$!
sleep 3

# Test the connection
echo "Testing HTTP connection..."
RESPONSE=$(curl -s -m 5 http://localhost:28080 2>&1)

if echo "$RESPONSE" | grep -q "Welcome to nginx"; then
    echo "✅ SUCCESS! Port-forward is working!"
    echo "Response snippet:"
    echo "$RESPONSE" | grep -o "Welcome to nginx.*" | head -1
    RESULT=0
else
    echo "❌ Port-forward test failed"
    echo "Response: $RESPONSE"
    echo ""
    echo "Server logs:"
    tail -20 /tmp/krust.log | grep -E "(ERROR|WARN|port-forward|WebSocket)"
    RESULT=1
fi

# Cleanup
kill $PF_PID 2>/dev/null
kubectl delete pod test-nginx --force --grace-period=0 2>/dev/null
kill $SERVER_PID 2>/dev/null

exit $RESULT