#!/bin/bash

# Test to understand the port-forward protocol by examining the raw frames

echo "=== Port Forward Protocol Analysis ==="
echo

# Start Krust if not running
if ! curl -s http://localhost:6443/livez > /dev/null 2>&1; then
    echo "Starting Krust..."
    cargo run > /tmp/krust.log 2>&1 &
    sleep 5
fi

# Clear the log
echo "" > /tmp/protocol_debug.log

# Test 1: Create a pod with specific port
echo "Test 1: Pod with port 80"
kubectl delete pod test-80 --ignore-not-found=true 2>/dev/null
kubectl run test-80 --image=nginx --port=80
sleep 2

# Capture the port-forward attempt
echo "Attempting port-forward 8080:80..."
timeout 1 kubectl port-forward pod/test-80 8080:80 -v=9 >> /tmp/protocol_debug.log 2>&1 &
sleep 2

# Extract the relevant frames from Krust log
echo "Frames received for 8080:80:"
grep "Received binary" /tmp/krust.log | tail -10

echo
echo "Test 2: Pod with port 3000"
kubectl delete pod test-3000 --ignore-not-found=true 2>/dev/null
kubectl run test-3000 --image=node --port=3000 -- sh -c "while true; do echo 'HTTP/1.1 200 OK\n\nHello' | nc -l -p 3000; done"
sleep 2

# Clear krust log
pkill -f krust
cargo run > /tmp/krust2.log 2>&1 &
sleep 5

# Capture the port-forward attempt
echo "Attempting port-forward 9000:3000..."
timeout 1 kubectl port-forward pod/test-3000 9000:3000 >> /tmp/protocol_debug.log 2>&1 &
sleep 2

echo "Frames received for 9000:3000:"
grep "Received binary" /tmp/krust2.log | tail -10

echo
echo "=== Analysis ==="
echo "Looking for patterns in the binary data..."

# Check if the port numbers appear in the frames
echo
echo "Checking if port 80 (0x50) appears in frames:"
grep "Received binary" /tmp/krust.log | grep -E "50|00 50" | head -3

echo
echo "Checking if port 3000 (0x0BB8) appears in frames:"
grep "Received binary" /tmp/krust2.log | grep -E "bb 8|0b b8" | head -3

# Cleanup
kubectl delete pod test-80 test-3000 --ignore-not-found=true 2>/dev/null

echo
echo "=== Key Observations ==="
echo "1. The sequence [80, 03] is always first - SPDY control frame"
echo "2. The next frames contain port configuration"
echo "3. kubectl expects acknowledgment before sending data"