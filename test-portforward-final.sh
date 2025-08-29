#!/bin/bash

echo "=== Port Forwarding Final Test ==="
echo

# Start Krust if not running
if ! curl -s http://localhost:6443/livez > /dev/null 2>&1; then
    echo "Starting Krust..."
    pkill -f krust
    cargo run > /tmp/krust.log 2>&1 &
    sleep 5
fi

# Clean up any existing test pods
kubectl delete pod test-nginx test-http --ignore-not-found=true 2>/dev/null
sleep 2

# Test 1: Create nginx pod
echo "[Test 1] Creating nginx pod..."
kubectl run test-nginx --image=nginx --port=80
sleep 3

# Test 2: Try port forwarding
echo "[Test 2] Starting port forward on 8080:80..."
kubectl port-forward pod/test-nginx 8080:80 > /tmp/pf-output.log 2>&1 &
PF_PID=$!
echo "Port forward PID: $PF_PID"
sleep 3

# Test 3: Check if port is listening
echo "[Test 3] Checking if port 8080 is listening..."
if lsof -i :8080 | grep -q LISTEN; then
    echo "✓ Port 8080 is listening"
else
    echo "✗ Port 8080 is NOT listening"
fi

# Test 4: Try to connect
echo "[Test 4] Testing HTTP connection..."
if curl -s -m 2 http://localhost:8080 | grep -q "nginx"; then
    echo "✓ Successfully connected to nginx through port forward!"
    curl -s http://localhost:8080 | head -10
else
    echo "✗ Failed to connect or no nginx response"
fi

# Test 5: Check logs
echo
echo "[Test 5] Port forward output:"
cat /tmp/pf-output.log

echo
echo "[Test 6] Krust logs (last 20 lines):"
tail -20 /tmp/krust.log | grep -E "portforward|SPDY|WebSocket|stream"

# Cleanup
kill $PF_PID 2>/dev/null
kubectl delete pod test-nginx --ignore-not-found=true 2>/dev/null

echo
echo "=== Test completed ===">