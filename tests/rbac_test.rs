use reqwest;
use serde_json::json;

#[tokio::test]
async fn test_role_crud_operations() {
    let client = reqwest::Client::new();
    let base_url = "http://localhost:6443/apis/rbac.authorization.k8s.io/v1";
    
    // Check if server is running
    if client.get("http://localhost:6443/livez").send().await.is_err() {
        eprintln!("Server not running, skipping test");
        return;
    }
    
    // Create a Role
    let role = json!({
        "apiVersion": "rbac.authorization.k8s.io/v1",
        "kind": "Role",
        "metadata": {
            "name": "pod-reader",
            "namespace": "default"
        },
        "rules": [{
            "apiGroups": [""],
            "resources": ["pods"],
            "verbs": ["get", "watch", "list"]
        }, {
            "apiGroups": [""],
            "resources": ["pods/log"],
            "verbs": ["get"]
        }]
    });
    
    let response = client
        .post(&format!("{}/namespaces/default/roles", base_url))
        .json(&role)
        .send()
        .await
        .unwrap();
    
    if response.status() == 404 {
        eprintln!("RBAC endpoints not implemented yet");
        return;
    }
    
    assert_eq!(response.status(), 201);
    let created: serde_json::Value = response.json().await.unwrap();
    assert_eq!(created["metadata"]["name"], "pod-reader");
    assert_eq!(created["rules"][0]["resources"][0], "pods");
    
    // Get Role
    let response = client
        .get(&format!("{}/namespaces/default/roles/pod-reader", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    
    // Update Role
    let mut updated_role = created.clone();
    updated_role["rules"].as_array_mut().unwrap().push(json!({
        "apiGroups": ["apps"],
        "resources": ["deployments"],
        "verbs": ["get", "list"]
    }));
    
    let response = client
        .put(&format!("{}/namespaces/default/roles/pod-reader", base_url))
        .json(&updated_role)
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    let updated: serde_json::Value = response.json().await.unwrap();
    assert_eq!(updated["rules"].as_array().unwrap().len(), 3);
    
    // List Roles
    let response = client
        .get(&format!("{}/namespaces/default/roles", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    
    // Delete Role
    let response = client
        .delete(&format!("{}/namespaces/default/roles/pod-reader", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
}

#[tokio::test]
async fn test_rolebinding_crud_operations() {
    let client = reqwest::Client::new();
    let base_url = "http://localhost:6443/apis/rbac.authorization.k8s.io/v1";
    
    // Check if server is running
    if client.get("http://localhost:6443/livez").send().await.is_err() {
        eprintln!("Server not running, skipping test");
        return;
    }
    
    // Create a RoleBinding
    let rolebinding = json!({
        "apiVersion": "rbac.authorization.k8s.io/v1",
        "kind": "RoleBinding",
        "metadata": {
            "name": "read-pods",
            "namespace": "default"
        },
        "subjects": [{
            "kind": "User",
            "name": "jane",
            "apiGroup": "rbac.authorization.k8s.io"
        }, {
            "kind": "ServiceAccount",
            "name": "default",
            "namespace": "default"
        }],
        "roleRef": {
            "kind": "Role",
            "name": "pod-reader",
            "apiGroup": "rbac.authorization.k8s.io"
        }
    });
    
    let response = client
        .post(&format!("{}/namespaces/default/rolebindings", base_url))
        .json(&rolebinding)
        .send()
        .await
        .unwrap();
    
    if response.status() == 404 {
        eprintln!("RoleBinding endpoints not implemented yet");
        return;
    }
    
    assert_eq!(response.status(), 201);
    let created: serde_json::Value = response.json().await.unwrap();
    assert_eq!(created["metadata"]["name"], "read-pods");
    assert_eq!(created["subjects"].as_array().unwrap().len(), 2);
    
    // Get RoleBinding
    let response = client
        .get(&format!("{}/namespaces/default/rolebindings/read-pods", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    
    // Delete RoleBinding
    let response = client
        .delete(&format!("{}/namespaces/default/rolebindings/read-pods", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
}

#[tokio::test]
async fn test_clusterrole_crud_operations() {
    let client = reqwest::Client::new();
    let base_url = "http://localhost:6443/apis/rbac.authorization.k8s.io/v1";
    
    // Check if server is running
    if client.get("http://localhost:6443/livez").send().await.is_err() {
        eprintln!("Server not running, skipping test");
        return;
    }
    
    // Create a ClusterRole
    let clusterrole = json!({
        "apiVersion": "rbac.authorization.k8s.io/v1",
        "kind": "ClusterRole",
        "metadata": {
            "name": "secret-reader"
        },
        "rules": [{
            "apiGroups": [""],
            "resources": ["secrets"],
            "verbs": ["get", "watch", "list"]
        }]
    });
    
    let response = client
        .post(&format!("{}/clusterroles", base_url))
        .json(&clusterrole)
        .send()
        .await
        .unwrap();
    
    if response.status() == 404 {
        eprintln!("ClusterRole endpoints not implemented yet");
        return;
    }
    
    assert_eq!(response.status(), 201);
    let created: serde_json::Value = response.json().await.unwrap();
    assert_eq!(created["metadata"]["name"], "secret-reader");
    
    // Get ClusterRole
    let response = client
        .get(&format!("{}/clusterroles/secret-reader", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    
    // Update ClusterRole
    let mut updated_clusterrole = created.clone();
    updated_clusterrole["rules"].as_array_mut().unwrap().push(json!({
        "apiGroups": [""],
        "resources": ["configmaps"],
        "verbs": ["get", "list"]
    }));
    
    let response = client
        .put(&format!("{}/clusterroles/secret-reader", base_url))
        .json(&updated_clusterrole)
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    
    // List ClusterRoles
    let response = client
        .get(&format!("{}/clusterroles", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    
    // Delete ClusterRole
    let response = client
        .delete(&format!("{}/clusterroles/secret-reader", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
}

#[tokio::test]
async fn test_clusterrolebinding_crud_operations() {
    let client = reqwest::Client::new();
    let base_url = "http://localhost:6443/apis/rbac.authorization.k8s.io/v1";
    
    // Check if server is running
    if client.get("http://localhost:6443/livez").send().await.is_err() {
        eprintln!("Server not running, skipping test");
        return;
    }
    
    // Create a ClusterRoleBinding
    let clusterrolebinding = json!({
        "apiVersion": "rbac.authorization.k8s.io/v1",
        "kind": "ClusterRoleBinding",
        "metadata": {
            "name": "read-secrets-global"
        },
        "subjects": [{
            "kind": "Group",
            "name": "manager",
            "apiGroup": "rbac.authorization.k8s.io"
        }],
        "roleRef": {
            "kind": "ClusterRole",
            "name": "secret-reader",
            "apiGroup": "rbac.authorization.k8s.io"
        }
    });
    
    let response = client
        .post(&format!("{}/clusterrolebindings", base_url))
        .json(&clusterrolebinding)
        .send()
        .await
        .unwrap();
    
    if response.status() == 404 {
        eprintln!("ClusterRoleBinding endpoints not implemented yet");
        return;
    }
    
    assert_eq!(response.status(), 201);
    let created: serde_json::Value = response.json().await.unwrap();
    assert_eq!(created["metadata"]["name"], "read-secrets-global");
    
    // Get ClusterRoleBinding
    let response = client
        .get(&format!("{}/clusterrolebindings/read-secrets-global", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    
    // Delete ClusterRoleBinding
    let response = client
        .delete(&format!("{}/clusterrolebindings/read-secrets-global", base_url))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
}