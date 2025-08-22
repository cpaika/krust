# Krust - Kubernetes in Rust

A minimal Kubernetes implementation in Rust that conforms to the Kubernetes API and works with `kubectl`.

## Features Implemented

✅ **Core API Server**
- Kubernetes-compatible REST API on port 6443
- Health endpoints (`/healthz`, `/livez`, `/readyz`)
- API discovery endpoints
- Full CRUD operations for Pods

✅ **Pod Management**
- Create, Read, Update, Delete pods via kubectl
- Pod metadata and labels support
- Namespace support (default namespace implemented)
- Resource versioning

✅ **Storage Layer**
- SQLite-based persistence
- Event recording for audit trail
- Watch functionality for real-time updates

✅ **Scheduler**
- Automatic pod scheduling to single node
- Assigns pending pods to available node

✅ **Container Runtime Integration**
- Docker integration via bollard
- Container lifecycle management
- Pod to container mapping

✅ **kubectl Compatibility**
- Works with standard kubectl commands
- Supports apply, get, delete, describe operations
- Node listing and management

## Quick Start

1. **Build the project:**
```bash
cargo build
```

2. **Run the server:**
```bash
cargo run
```

3. **Use kubectl:**
```bash
# Get nodes
kubectl --server=http://localhost:6443 get nodes

# Create a pod
kubectl --server=http://localhost:6443 apply -f pod.yaml --validate=false

# List pods
kubectl --server=http://localhost:6443 get pods

# Delete a pod
kubectl --server=http://localhost:6443 delete pod <pod-name>
```

## Architecture

- **API Server**: Handles all Kubernetes API requests
- **Scheduler**: Assigns pods to the single node
- **Kubelet**: Manages container lifecycle (requires Docker)
- **Storage**: SQLite database for persistent state
- **Watch**: Server-sent events for real-time updates

## Testing

```bash
# Run unit tests
cargo test

# Run integration tests with kubectl
cargo test --test kubectl_integration_test -- --ignored --nocapture
```

## Limitations

- Single node only
- Limited to core v1 API resources (primarily Pods)
- No authentication/authorization
- No networking between pods
- Services and Deployments are stubbed but not fully implemented
- Requires `--validate=false` flag with kubectl (no OpenAPI schema)

## Dependencies

- Rust 1.75+
- SQLite
- Docker (optional, for actually running containers)
- kubectl (for testing)