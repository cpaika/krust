#!/bin/bash
set -e

echo "=== Krust Port-Forward Demo ==="
echo ""
echo "This demo shows how to use kubectl port-forward with Krust"
echo ""

# Function to cleanup on exit
cleanup() {
    echo ""
    echo "Cleaning up..."
    kubectl delete pod test-web-app --force --grace-period=0 2>/dev/null || true
    kubectl delete service test-web-service --force --grace-period=0 2>/dev/null || true
    pkill -f "kubectl port-forward" 2>/dev/null || true
    echo "✓ Cleanup complete"
}

trap cleanup EXIT

# Make sure Krust is running
if ! curl -s http://localhost:6443/healthz | grep -q "ok"; then
    echo "❌ Krust server is not running!"
    echo "Please start Krust first with: cargo run"
    exit 1
fi

echo "✓ Krust server is running"
echo ""

# Configure kubectl
echo "Configuring kubectl..."
kubectl config set-cluster krust --server=http://localhost:6443 >/dev/null
kubectl config set-context krust --cluster=krust >/dev/null
kubectl config use-context krust >/dev/null
echo "✓ kubectl configured"
echo ""

# Create a test pod
echo "Creating a test web application pod..."
cat <<EOF | kubectl apply -f -
apiVersion: v1
kind: Pod
metadata:
  name: test-web-app
  labels:
    app: test-web
spec:
  containers:
  - name: web
    image: hashicorp/http-echo:latest
    args:
    - "-text=Hello from Krust port-forward!"
    - "-listen=:8080"
    ports:
    - containerPort: 8080
      name: http
EOF

echo "Waiting for pod to be ready..."
for i in {1..30}; do
    if kubectl get pod test-web-app -o jsonpath='{.status.phase}' 2>/dev/null | grep -q "Running"; then
        echo "✓ Pod is running"
        break
    fi
    sleep 1
done

# Give container time to fully start
sleep 2

echo ""
echo "=== Method 1: Port-forward to Pod directly ==="
echo "Running: kubectl port-forward pod/test-web-app 8888:8080"
kubectl port-forward pod/test-web-app 8888:8080 &
PF_PID=$!
sleep 2

echo "Testing connection to localhost:8888..."
RESPONSE=$(curl -s http://localhost:8888 || echo "Failed")
if echo "$RESPONSE" | grep -q "Hello from Krust port-forward!"; then
    echo "✅ Success! Response: $RESPONSE"
else
    echo "❌ Failed. Response: $RESPONSE"
fi

# Stop the port-forward
kill $PF_PID 2>/dev/null || true
wait $PF_PID 2>/dev/null || true

echo ""
echo "=== Method 2: Port-forward to Service ==="
echo "Creating a service..."
cat <<EOF | kubectl apply -f -
apiVersion: v1
kind: Service
metadata:
  name: test-web-service
spec:
  selector:
    app: test-web
  ports:
  - port: 80
    targetPort: 8080
    name: http
EOF

sleep 1

echo "Running: kubectl port-forward service/test-web-service 8889:80"
kubectl port-forward service/test-web-service 8889:80 &
PF_PID=$!
sleep 2

echo "Testing connection to localhost:8889..."
RESPONSE=$(curl -s http://localhost:8889 || echo "Failed")
if echo "$RESPONSE" | grep -q "Hello from Krust port-forward!"; then
    echo "✅ Success! Response: $RESPONSE"
else
    echo "❌ Failed. Response: $RESPONSE"
fi

# Stop the port-forward
kill $PF_PID 2>/dev/null || true
wait $PF_PID 2>/dev/null || true

echo ""
echo "=== Method 3: Multiple Ports ==="
echo "Creating a multi-port pod..."
cat <<EOF | kubectl apply -f -
apiVersion: v1
kind: Pod
metadata:
  name: test-multi-port
  labels:
    app: multi-port
spec:
  containers:
  - name: web
    image: nicolaka/netshoot:latest
    command: ["/bin/sh"]
    args:
    - -c
    - |
      echo "Starting servers on ports 8080 and 8081..."
      while true; do echo "Port 8080 response" | nc -l -p 8080; done &
      while true; do echo "Port 8081 response" | nc -l -p 8081; done &
      sleep infinity
    ports:
    - containerPort: 8080
      name: http1
    - containerPort: 8081
      name: http2
EOF

echo "Waiting for multi-port pod to be ready..."
for i in {1..30}; do
    if kubectl get pod test-multi-port -o jsonpath='{.status.phase}' 2>/dev/null | grep -q "Running"; then
        echo "✓ Multi-port pod is running"
        break
    fi
    sleep 1
done

sleep 3

echo "Running: kubectl port-forward pod/test-multi-port 9090:8080 9091:8081"
kubectl port-forward pod/test-multi-port 9090:8080 9091:8081 &
PF_PID=$!
sleep 2

echo "Testing port 9090 (maps to container port 8080)..."
RESPONSE1=$(echo "test" | nc localhost 9090 2>/dev/null || echo "Failed")
echo "Response: $RESPONSE1"

echo "Testing port 9091 (maps to container port 8081)..."
RESPONSE2=$(echo "test" | nc localhost 9091 2>/dev/null || echo "Failed")
echo "Response: $RESPONSE2"

# Stop the port-forward
kill $PF_PID 2>/dev/null || true
wait $PF_PID 2>/dev/null || true

# Cleanup multi-port pod
kubectl delete pod test-multi-port --force --grace-period=0 2>/dev/null || true

echo ""
echo "=== Demo Complete ==="
echo ""
echo "You can now use kubectl port-forward with Krust just like with real Kubernetes!"
echo ""
echo "Examples:"
echo "  kubectl port-forward pod/<pod-name> <local-port>:<container-port>"
echo "  kubectl port-forward service/<service-name> <local-port>:<service-port>"
echo "  kubectl port-forward pod/<pod-name> <local-port1>:<container-port1> <local-port2>:<container-port2>"
echo ""