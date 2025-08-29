#!/bin/bash

# Debug script to test port forwarding issue

set -e

echo "=== Port Forwarding Debug Test ==="
echo

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Check if Krust is running
if ! curl -s http://localhost:6443/livez > /dev/null 2>&1; then
    echo -e "${RED}Error: Krust doesn't appear to be running on localhost:6443${NC}"
    echo "Please start Krust with: cargo run"
    exit 1
fi

echo -e "${GREEN}✓ Krust is running${NC}"
echo

# Create or update nginx pod
echo "Creating nginx pod with port 80..."
kubectl delete pod nginx-debug --ignore-not-found=true 2>/dev/null
sleep 1

cat <<EOF | kubectl apply -f -
apiVersion: v1
kind: Pod
metadata:
  name: nginx-debug
  namespace: default
spec:
  containers:
  - name: nginx
    image: nginx
    ports:
    - containerPort: 80
      protocol: TCP
EOF

echo "Waiting for pod to be ready..."
for i in {1..10}; do
    PHASE=$(kubectl get pod nginx-debug -o jsonpath='{.status.phase}' 2>/dev/null || echo "Unknown")
    if [ "$PHASE" = "Running" ]; then
        echo -e "${GREEN}✓ Pod is running${NC}"
        break
    fi
    echo -n "."
    sleep 1
done

echo
echo -e "${BLUE}=== Test 1: Direct port-forward API call ===${NC}"
echo "Testing URL: http://localhost:6443/api/v1/namespaces/default/pods/nginx-debug/portforward?ports=8080:80"

# Make a direct API call to see what's happening
curl -s -v \
    -H "Connection: Upgrade" \
    -H "Upgrade: SPDY/3.1+portforward.k8s.io" \
    "http://localhost:6443/api/v1/namespaces/default/pods/nginx-debug/portforward?ports=8080:80" \
    2>&1 | grep -E "(GET|HTTP|< )" | head -10

echo
echo -e "${BLUE}=== Test 2: kubectl port-forward with verbose output ===${NC}"
echo "Running: kubectl port-forward pod/nginx-debug 8080:80 -v=6"

# Start port forward with verbose output in background
timeout 3 kubectl port-forward pod/nginx-debug 8080:80 -v=6 2>&1 | head -20 &
PF_PID=$!

# Wait a moment
sleep 2

# Kill the process if still running
kill $PF_PID 2>/dev/null || true

echo
echo -e "${BLUE}=== Test 3: Check Krust logs for port mapping ===${NC}"
echo "Last port forward entries in Krust log:"

# Check the Krust logs for the most recent port forward attempt
if [ -f /tmp/krust.log ]; then
    grep "Port" /tmp/krust.log | tail -5
else
    echo "No Krust log file found at /tmp/krust.log"
fi

echo
echo -e "${BLUE}=== Test 4: Testing port parsing logic ===${NC}"

# Create a small Rust test to verify parsing
cat > /tmp/test_parse.rs <<'EOF'
fn parse_port_mappings(ports_str: &str) -> Vec<(u16, u16)> {
    let mut mappings = Vec::new();
    
    for port_spec in ports_str.split(',') {
        let port_spec = port_spec.trim();
        if port_spec.is_empty() {
            continue;
        }
        
        let parts: Vec<&str> = port_spec.split(':').collect();
        let mapping = match parts.len() {
            1 => {
                if let Ok(port) = parts[0].parse::<u16>() {
                    Some((port, port))
                } else {
                    None
                }
            }
            2 => {
                if let (Ok(local), Ok(remote)) = (parts[0].parse::<u16>(), parts[1].parse::<u16>()) {
                    Some((local, remote))
                } else {
                    None
                }
            }
            _ => None,
        };
        
        if let Some(mapping) = mapping {
            mappings.push(mapping);
        }
    }
    
    mappings
}

fn main() {
    let test_cases = vec![
        "8080:80",
        "80",
        "8080:80,8443:443",
    ];
    
    for input in test_cases {
        let result = parse_port_mappings(input);
        println!("Input: '{}' -> {:?}", input, result);
    }
}
EOF

rustc /tmp/test_parse.rs -o /tmp/test_parse 2>/dev/null && /tmp/test_parse

echo
echo -e "${BLUE}=== Analysis ===${NC}"
echo "Expected behavior:"
echo "  - When kubectl sends '8080:80', it means forward local port 8080 to container port 80"
echo "  - The parse_port_mappings should return (8080, 80) where first is local, second is remote"
echo "  - The container should be created with port 80, not 8080"
echo
echo "Current issue:"
echo "  - The logs show 'remote_port: 8080' instead of 'remote_port: 80'"
echo "  - This suggests either:"
echo "    1. kubectl is not sending the ports in the query string"
echo "    2. The ports are being sent differently (maybe in WebSocket frames)"
echo "    3. There's a default fallback to 8080"

# Cleanup
kubectl delete pod nginx-debug --ignore-not-found=true 2>/dev/null

echo
echo -e "${GREEN}✅ Debug test completed${NC}"