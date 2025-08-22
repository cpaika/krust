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

## Stop Krust

Press `Ctrl+C` in the terminal running `cargo run`.