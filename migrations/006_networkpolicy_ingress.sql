-- NetworkPolicy table
CREATE TABLE IF NOT EXISTS networkpolicies (
    uid TEXT PRIMARY KEY,
    namespace TEXT NOT NULL,
    name TEXT NOT NULL,
    
    -- Spec fields
    pod_selector TEXT NOT NULL, -- JSON object for label selector
    policy_types TEXT NOT NULL, -- JSON array of policy types (Ingress, Egress)
    ingress TEXT, -- JSON array of ingress rules
    egress TEXT, -- JSON array of egress rules
    
    -- Metadata
    labels TEXT NOT NULL, -- JSON object
    annotations TEXT NOT NULL, -- JSON object
    resource_version INTEGER NOT NULL,
    generation INTEGER DEFAULT 1,
    creation_timestamp TEXT NOT NULL,
    deletion_timestamp TEXT,
    
    UNIQUE(namespace, name)
);

-- Ingress table
CREATE TABLE IF NOT EXISTS ingresses (
    uid TEXT PRIMARY KEY,
    namespace TEXT NOT NULL,
    name TEXT NOT NULL,
    
    -- Spec fields
    ingress_class_name TEXT,
    default_backend TEXT, -- JSON object for default backend
    rules TEXT, -- JSON array of ingress rules
    tls TEXT, -- JSON array of TLS configurations
    
    -- Status fields
    load_balancer TEXT, -- JSON object for load balancer status
    
    -- Metadata
    labels TEXT NOT NULL, -- JSON object
    annotations TEXT NOT NULL, -- JSON object
    resource_version INTEGER NOT NULL,
    generation INTEGER DEFAULT 1,
    creation_timestamp TEXT NOT NULL,
    deletion_timestamp TEXT,
    
    UNIQUE(namespace, name)
);

-- Indexes for efficient queries
CREATE INDEX IF NOT EXISTS idx_networkpolicy_namespace ON networkpolicies(namespace);
CREATE INDEX IF NOT EXISTS idx_ingress_namespace ON ingresses(namespace);
CREATE INDEX IF NOT EXISTS idx_ingress_class ON ingresses(ingress_class_name);