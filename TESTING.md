# Krust Testing Guide

## Overview
This document describes the testing strategy and organization for the Krust Kubernetes API server.

## Test Organization

### Core Test Suites

1. **Integration Tests** (`tests/integration_test.rs`)
   - Tests core CRUD operations for all implemented resources
   - Validates API compatibility
   - Status: ✅ All 8 tests passing

2. **Edge Cases** (`tests/edge_cases_test.rs`)  
   - Tests validation and error handling
   - Invalid inputs, boundary conditions
   - Concurrent operations
   - Status: ✅ Comprehensive coverage

3. **TDD Validation** (`tests/tdd_validation_test.rs`)
   - Test-driven development validation tests
   - Pod immutability
   - API version validation
   - Service port validation
   - Secret validation
   - Status: ✅ All validations implemented

### Running Tests

#### Quick Test (No Server Required)
```bash
# Run unit tests only
cargo test --lib
```

#### Full Integration Tests
```bash
# Use the provided script to run all tests with server
./run_all_tests.sh
```

#### Manual Test Execution
```bash
# 1. Start the server in one terminal
cargo run

# 2. In another terminal, run tests
cargo test

# Run specific test suite
cargo test --test integration_test
cargo test --test edge_cases
cargo test --test tdd_validation
```

#### Test Specific Features
```bash
# Test with server feature flag
cargo test --features test-server

# Run ignored tests (requires server)
cargo test -- --ignored
```

## Test Coverage by Resource

### Fully Implemented & Tested ✅
- **Namespaces**: CRUD, validation, edge cases
- **Pods**: CRUD, status, logs, immutability
- **Services**: CRUD, port validation
- **ServiceAccounts**: CRUD, token creation
- **ConfigMaps**: CRUD, binary data, immutability
- **Secrets**: CRUD, base64 validation, type checking
- **Deployments**: Basic CRUD operations

### Partially Implemented ⚠️
- **Port Forwarding**: WebSocket implementation (needs more testing)
- **Pod Exec/Attach**: Basic structure (needs implementation)

### Not Yet Implemented ❌
- StatefulSets
- DaemonSets
- Jobs/CronJobs
- Ingress
- NetworkPolicies
- PersistentVolumes/Claims
- RBAC (Roles, RoleBindings)
- HorizontalPodAutoscaler
- ResourceQuotas

## Validation Rules Implemented

### Name Validation
- Must be 1-253 characters
- Lowercase alphanumeric and hyphens only
- Must start/end with alphanumeric

### API Version Validation
- Auto-corrects wrong versions
- Validates kind matches endpoint
- Returns 400 for missing fields

### Port Validation
- Range: 1-65535
- Clamps high values
- Validates targetPort format

### Secret Validation
- Base64 encoding validation
- Type-specific field requirements
- Size limit (1MB)
- Immutability enforcement

### Pod Immutability
- Spec fields cannot be changed after creation
- Only metadata and status updates allowed

## Warning Fixes Applied

### Compilation Warnings
- Fixed unused imports with `#![allow(unused_imports)]` where appropriate
- Removed actually unused imports
- Fixed deprecated base64 API usage
- Fixed StatusCode constant names

### Clippy Recommendations
- Most clippy warnings addressed
- Some complex patterns left as-is for clarity

## Continuous Integration

To ensure tests stay passing:

1. **Before Commits**:
   ```bash
   cargo fmt
   cargo clippy
   cargo test
   ```

2. **Before Pull Requests**:
   ```bash
   ./run_all_tests.sh
   ```

## Troubleshooting

### Tests Hanging
- Check if server is already running on port 6443
- Kill any stuck cargo/rustc processes: `pkill -f cargo; pkill -f rustc`

### Compilation Slow
- Clean build: `cargo clean`
- Check for circular dependencies
- Consider using `cargo check` instead of `cargo build`

### Server Not Starting
- Check database migrations: `sqlx migrate run`
- Ensure SQLite is installed
- Check for port conflicts

## Future Improvements

1. Add property-based testing with quickcheck
2. Implement performance benchmarks
3. Add fuzzing for security testing
4. Increase code coverage metrics
5. Add CI/CD pipeline with GitHub Actions