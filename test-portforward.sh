#!/bin/bash

# Test script for Krust port forwarding functionality

set -e

echo "=== Krust Port Forwarding Test ==="
echo

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Check if kubectl is configured for Krust
CURRENT_CONTEXT=$(kubectl config current-context)
if [ "$CURRENT_CONTEXT" != "krust" ]; then
    echo -e "${YELLOW}Warning: Current kubectl context is '$CURRENT_CONTEXT', not 'krust'${NC}"
    echo "Setting up kubectl for Krust..."
    kubectl config set-cluster krust --server=http://localhost:6443
    kubectl config set-context krust --cluster=krust
    kubectl config use-context krust
fi

# Check if Krust is running
if ! curl -s http://localhost:6443/livez > /dev/null 2>&1; then
    echo -e "${RED}Error: Krust doesn't appear to be running on localhost:6443${NC}"
    echo "Please start Krust with: cargo run"
    exit 1
fi

echo -e "${GREEN}✓ Krust is running${NC}"
echo

# Clean up any existing test resources
echo "Cleaning up any existing test resources..."
kubectl delete pod nginx-test --ignore-not-found=true
kubectl delete pod echo-test --ignore-not-found=true
sleep 2

# Test 1: Basic nginx port forwarding
echo "=== Test 1: Basic nginx port forwarding ==="
echo "Creating nginx pod..."
kubectl run nginx-test --image=nginx:alpine --port=80

echo "Waiting for pod to be ready..."
for i in {1..30}; do
    if kubectl get pod nginx-test -o jsonpath='{.status.phase}' 2>/dev/null | grep -q "Running"; then
        echo -e "${GREEN}✓ Pod is running${NC}"
        break
    fi
    echo -n "."
    sleep 2
done

# Start port forwarding in background
echo "Starting port forwarding (localhost:8080 -> pod:80)..."
kubectl port-forward pod/nginx-test 8080:80 > /tmp/portforward.log 2>&1 &
PF_PID=$!
sleep 3

# Test the connection
echo "Testing connection to localhost:8080..."
if curl -s http://localhost:8080 | grep -q "Welcome to nginx"; then
    echo -e "${GREEN}✓ Port forwarding works! Received nginx welcome page${NC}"
else
    echo -e "${RED}✗ Failed to connect or receive expected response${NC}"
    echo "Port forward logs:"
    cat /tmp/portforward.log
fi

# Kill port forwarding
kill $PF_PID 2>/dev/null || true
wait $PF_PID 2>/dev/null || true

echo

# Test 2: Echo server with custom response
echo "=== Test 2: Echo server test ==="
echo "Creating echo server pod..."

cat <<EOF | kubectl apply -f -
apiVersion: v1
kind: Pod
metadata:
  name: echo-test
spec:
  containers:
  - name: echo
    image: hashicorp/http-echo:0.2.3
    args:
      - "-text=Hello from Krust port forwarding!"
      - "-listen=:5678"
    ports:
    - containerPort: 5678
EOF

echo "Waiting for echo pod to be ready..."
for i in {1..30}; do
    if kubectl get pod echo-test -o jsonpath='{.status.phase}' 2>/dev/null | grep -q "Running"; then
        echo -e "${GREEN}✓ Pod is running${NC}"
        break
    fi
    echo -n "."
    sleep 2
done

# Start port forwarding in background
echo "Starting port forwarding (localhost:9090 -> pod:5678)..."
kubectl port-forward pod/echo-test 9090:5678 > /tmp/portforward2.log 2>&1 &
PF_PID2=$!
sleep 3

# Test the connection
echo "Testing connection to localhost:9090..."
RESPONSE=$(curl -s http://localhost:9090)
if echo "$RESPONSE" | grep -q "Hello from Krust"; then
    echo -e "${GREEN}✓ Port forwarding works! Received: $RESPONSE${NC}"
else
    echo -e "${RED}✗ Failed to connect or receive expected response${NC}"
    echo "Got: $RESPONSE"
    echo "Port forward logs:"
    cat /tmp/portforward2.log
fi

# Kill port forwarding
kill $PF_PID2 2>/dev/null || true
wait $PF_PID2 2>/dev/null || true

echo

# Test 3: Multiple ports
echo "=== Test 3: Testing port-forward with multiple ports ==="
echo "Starting multi-port forwarding (8081:80 and 9091:5678)..."
kubectl port-forward pod/nginx-test 8081:80 pod/echo-test 9091:5678 > /tmp/portforward3.log 2>&1 &
PF_PID3=$!
sleep 3

echo "Testing both forwarded ports..."
NGINX_OK=false
ECHO_OK=false

if curl -s http://localhost:8081 | grep -q "Welcome to nginx"; then
    echo -e "${GREEN}✓ Port 8081 -> nginx:80 works${NC}"
    NGINX_OK=true
else
    echo -e "${RED}✗ Port 8081 -> nginx:80 failed${NC}"
fi

if curl -s http://localhost:9091 | grep -q "Hello from Krust"; then
    echo -e "${GREEN}✓ Port 9091 -> echo:5678 works${NC}"
    ECHO_OK=true
else
    echo -e "${RED}✗ Port 9091 -> echo:5678 failed${NC}"
fi

# Kill port forwarding
kill $PF_PID3 2>/dev/null || true
wait $PF_PID3 2>/dev/null || true

echo
echo "=== Test Summary ==="
echo "Cleaning up test resources..."
kubectl delete pod nginx-test --ignore-not-found=true
kubectl delete pod echo-test --ignore-not-found=true

if [ "$NGINX_OK" = true ] && [ "$ECHO_OK" = true ]; then
    echo -e "${GREEN}✓ All port forwarding tests passed!${NC}"
    exit 0
else
    echo -e "${RED}✗ Some tests failed. Please check the implementation.${NC}"
    exit 1
fi