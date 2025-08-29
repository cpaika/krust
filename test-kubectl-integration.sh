#!/bin/bash

echo "==================================================="
echo "kubectl Port-Forward Integration Test"
echo "==================================================="

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Clean up function
cleanup() {
    echo -e "\n${YELLOW}Cleaning up...${NC}"
    kill $KRUST_PID 2>/dev/null
    kubectl delete namespace test-integration 2>/dev/null || true
    rm -f $KUBECONFIG
}

trap cleanup EXIT

# Kill any existing krust
pkill -f "target/debug/krust" 2>/dev/null

echo -e "${YELLOW}Building Krust...${NC}"
cargo build 2>&1 | grep -E "(Compiling|Finished)" || {
    echo -e "${RED}Build failed${NC}"
    exit 1
}

echo -e "${GREEN}Build successful${NC}"

# Start krust
echo -e "${YELLOW}Starting Krust with debug logging...${NC}"
RUST_LOG=debug ./target/debug/krust > /tmp/krust-integration.log 2>&1 &
KRUST_PID=$!

# Wait for krust to be ready
echo -e "${YELLOW}Waiting for Krust to start...${NC}"
for i in {1..10}; do
    if curl -s http://localhost:6443/api/v1 > /dev/null 2>&1; then
        echo -e "${GREEN}Krust is ready${NC}"
        break
    fi
    sleep 1
done

# Check if krust is running
if ! ps -p $KRUST_PID > /dev/null; then
    echo -e "${RED}Error: Krust failed to start${NC}"
    tail -20 /tmp/krust-integration.log
    exit 1
fi

# Configure kubectl
export KUBECONFIG=/tmp/krust-kubeconfig-integration
cat > $KUBECONFIG <<EOF
apiVersion: v1
kind: Config
clusters:
- cluster:
    server: http://localhost:6443
  name: krust
contexts:
- context:
    cluster: krust
    user: krust
  name: krust
current-context: krust
users:
- name: krust
  user: {}
EOF

echo -e "${GREEN}kubectl configured for Krust${NC}"

# Create test namespace
echo -e "${YELLOW}Creating test namespace...${NC}"
kubectl create namespace test-integration 2>/dev/null || true

# Create nginx pod
echo -e "${YELLOW}Creating nginx pod...${NC}"
cat <<EOF | kubectl apply -f - --validate=false
apiVersion: v1
kind: Pod
metadata:
  name: nginx-integration
  namespace: test-integration
spec:
  containers:
  - name: nginx
    image: nginx:latest
    ports:
    - containerPort: 80
      name: http
EOF

# Set pod status to Running
echo -e "${YELLOW}Setting pod status to Running...${NC}"
curl -X PATCH http://localhost:6443/api/v1/namespaces/test-integration/pods/nginx-integration/status \
  -H "Content-Type: application/json" \
  -d '{
    "status": {
      "phase": "Running",
      "conditions": [
        {
          "type": "Ready",
          "status": "True",
          "lastTransitionTime": "2024-01-01T00:00:00Z"
        }
      ],
      "containerStatuses": [
        {
          "name": "nginx",
          "ready": true,
          "state": {
            "running": {
              "startedAt": "2024-01-01T00:00:00Z"
            }
          }
        }
      ]
    }
  }' -s > /dev/null

echo -e "${GREEN}Pod setup complete${NC}"

# Test port-forward
echo -e "\n${YELLOW}Testing kubectl port-forward...${NC}"
echo "Command: kubectl port-forward -n test-integration nginx-integration 8890:80"

# Start port-forward in background
kubectl port-forward -n test-integration nginx-integration 8890:80 > /tmp/kubectl-integration.log 2>&1 &
KUBECTL_PID=$!

# Wait for port-forward to establish
echo -e "${YELLOW}Waiting for port-forward to establish...${NC}"
sleep 3

# Check if kubectl is still running
if ! ps -p $KUBECTL_PID > /dev/null 2>&1; then
    echo -e "${RED}kubectl port-forward exited prematurely${NC}"
    echo "kubectl output:"
    cat /tmp/kubectl-integration.log
    echo -e "\n${YELLOW}Krust logs:${NC}"
    tail -30 /tmp/krust-integration.log | grep -E "(kubectl|WebSocket|error|failed)"
    exit 1
fi

# Test the connection
echo -e "\n${YELLOW}Testing HTTP through port-forward...${NC}"
HTTP_RESPONSE=$(curl -s -w "\n%{http_code}" http://localhost:8890 2>/dev/null || echo "FAILED")

if [[ "$HTTP_RESPONSE" == *"200"* ]] || [[ "$HTTP_RESPONSE" == *"nginx"* ]]; then
    echo -e "${GREEN}✓ SUCCESS: Port-forward is working!${NC}"
    echo "Response preview:"
    echo "$HTTP_RESPONSE" | head -5
    SUCCESS=true
else
    echo -e "${RED}✗ FAILED: Could not connect through port-forward${NC}"
    echo "Response: $HTTP_RESPONSE"
    SUCCESS=false
fi

# Kill kubectl
kill $KUBECTL_PID 2>/dev/null

# Show diagnostics
echo -e "\n${YELLOW}=== Diagnostics ===${NC}"

echo -e "\n${YELLOW}kubectl output:${NC}"
cat /tmp/kubectl-integration.log | head -10

echo -e "\n${YELLOW}Krust WebSocket logs:${NC}"
grep -E "(kubectl_exact|Acknowledgments|Frame:|Connected to container)" /tmp/krust-integration.log | tail -15

echo -e "\n${YELLOW}Krust errors (if any):${NC}"
grep -E "(error|Error|failed|Failed)" /tmp/krust-integration.log | tail -5 || echo "No errors found"

# Final result
echo -e "\n${YELLOW}=== Test Result ===${NC}"
if [[ "$SUCCESS" == "true" ]]; then
    echo -e "${GREEN}✓ Integration test PASSED${NC}"
    echo "kubectl port-forward is working correctly with Krust!"
    exit 0
else
    echo -e "${RED}✗ Integration test FAILED${NC}"
    echo "Check logs at:"
    echo "  - /tmp/krust-integration.log"
    echo "  - /tmp/kubectl-integration.log"
    exit 1
fi