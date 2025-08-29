# Krust - Kubernetes in Rust

A minimal Kubernetes API that runs on your laptop.

## Quick Start

### 1. Install Rust
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

### 2. Run Krust
```bash
cargo run
```

### 3. Configure kubectl
```bash
kubectl config set-cluster krust --server=http://localhost:6443
kubectl config set-context krust --cluster=krust
kubectl config use-context krust
```

### 4. Deploy the demo app
```bash
kubectl apply -f demo.yaml
```

### 5. Check your pods
```bash
kubectl get pods
```

## What's included

- Pods, Deployments, Services, ReplicaSets
- Docker container runtime
- SQLite storage
- Works with real kubectl
- Port forwarding support

## Port Forwarding to Pods

Krust supports port forwarding to pods, allowing you to access pod services locally.

### Basic Usage

1. **Deploy a test pod** (e.g., nginx):
```bash
kubectl run nginx --image=nginx --port=80
```

2. **Wait for the pod to be ready**:
```bash
kubectl wait --for=condition=ready pod/nginx --timeout=60s
```

3. **Port forward to the pod**:
```bash
kubectl port-forward pod/nginx 8080:80
```

This forwards local port 8080 to port 80 in the nginx pod.

### Verify Port Forwarding

In a new terminal, test the connection:

```bash
# Using curl
curl http://localhost:8080

# Or using wget
wget -O- http://localhost:8080

# You should see the nginx welcome page HTML
```

### Advanced Examples

**Forward multiple ports**:
```bash
kubectl port-forward pod/myapp 8080:80 8443:443
```

**Use a random local port** (kubectl will assign one):
```bash
kubectl port-forward pod/nginx :80
```

**Forward to a deployment** (forwards to one of its pods):
```bash
kubectl port-forward deployment/nginx 8080:80
```

**Forward to a service**:
```bash
kubectl port-forward service/nginx 8080:80
```

### Testing with a Simple Echo Server

1. **Create a pod with netcat as an echo server**:
```yaml
# echo-server.yaml
apiVersion: v1
kind: Pod
metadata:
  name: echo-server
spec:
  containers:
  - name: echo
    image: alpine
    command: ["/bin/sh"]
    args: ["-c", "while true; do nc -l -p 8080 -e echo 'HTTP/1.1 200 OK\n\nHello from Krust!'; done"]
    ports:
    - containerPort: 8080
```

2. **Apply and wait**:
```bash
kubectl apply -f echo-server.yaml
kubectl wait --for=condition=ready pod/echo-server --timeout=60s
```

3. **Port forward**:
```bash
kubectl port-forward pod/echo-server 9090:8080
```

4. **Test**:
```bash
curl http://localhost:9090
# Output: Hello from Krust!
```

### Troubleshooting Port Forwarding

If port forwarding isn't working:

1. **Port conflicts**: If you see "Address already in use" errors, it means multiple containers are trying to use the same port. Krust now automatically assigns unique host ports to each container service to avoid conflicts.

2. **Check pod status**:
```bash
kubectl get pod <pod-name> -o wide
kubectl describe pod <pod-name>
```

3. **Check pod logs**:
```bash
kubectl logs <pod-name>
```

4. **Verify the container port**:
```bash
kubectl get pod <pod-name> -o jsonpath='{.spec.containers[*].ports[*].containerPort}'
```

5. **Test connectivity inside the pod**:
```bash
kubectl exec <pod-name> -- curl localhost:<container-port>
```

6. **Check if the local port is already in use**:
```bash
lsof -i :<local-port>  # On macOS/Linux
netstat -an | grep <local-port>  # Alternative
```

### How It Works

Krust implements port forwarding using:
- WebSocket connections for bidirectional streaming
- SPDY protocol support for multiplexed streams
- Direct TCP proxying to container ports

The implementation supports the same protocol as real Kubernetes, making it fully compatible with kubectl.

## Stop Krust

Press `Ctrl+C` in the terminal running `cargo run`.