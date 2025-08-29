#!/bin/bash

echo "=== kubectl Port Forward Perfect Test ==="
echo

# Wait for pod to be ready
echo "Waiting for pod to be ready..."
kubectl wait --for=condition=Ready pod/test-nginx --timeout=10s

echo
echo "[Test 1] Starting port forward with kubectl..."
kubectl port-forward pod/test-nginx 8080:80 -v=6 > /tmp/kubectl-pf.log 2>&1 &
PF_PID=$!
echo "Port forward PID: $PF_PID"

# Give it time to establish
sleep 3

echo
echo "[Test 2] Check if port is listening..."
if lsof -i :8080 | grep -q LISTEN; then
    echo "✓ Port 8080 is listening"
else
    echo "✗ Port 8080 is NOT listening"
fi

echo
echo "[Test 3] Test HTTP request through port forward..."
echo "Sending HTTP request to localhost:8080..."
response=$(curl -s -m 3 -o /tmp/nginx-response.html -w "%{http_code}" http://localhost:8080/ 2>/dev/null)

if [ "$response" = "200" ]; then
    echo "✓ Got HTTP 200 response!"
    echo "Response preview:"
    head -5 /tmp/nginx-response.html
    
    # Test multiple requests
    echo
    echo "[Test 4] Testing multiple requests..."
    for i in {1..3}; do
        response=$(curl -s -m 2 -w "%{http_code}" http://localhost:8080/ -o /dev/null)
        echo "  Request $i: HTTP $response"
    done
else
    echo "✗ Failed to get response (HTTP $response)"
fi

echo
echo "[Test 5] kubectl output:"
tail -20 /tmp/kubectl-pf.log | grep -E "Forwarding|Handling|error"

echo
echo "[Test 6] Krust logs:"
tail -30 /tmp/krust.log | grep -E "kubectl|stream|frame|WebSocket|container"

# Cleanup
kill $PF_PID 2>/dev/null

echo
echo "=== Test Complete ===" 