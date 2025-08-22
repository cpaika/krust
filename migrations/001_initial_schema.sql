-- Create namespaces table
CREATE TABLE IF NOT EXISTS namespaces (
    uid TEXT PRIMARY KEY,
    name TEXT UNIQUE NOT NULL,
    resource_version INTEGER NOT NULL DEFAULT 1,
    creation_timestamp TEXT NOT NULL,
    deletion_timestamp TEXT,
    labels TEXT, -- JSON
    annotations TEXT, -- JSON
    spec TEXT NOT NULL, -- JSON
    status TEXT -- JSON
);

-- Create pods table
CREATE TABLE IF NOT EXISTS pods (
    uid TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    namespace TEXT NOT NULL DEFAULT 'default',
    resource_version INTEGER NOT NULL DEFAULT 1,
    creation_timestamp TEXT NOT NULL,
    deletion_timestamp TEXT,
    labels TEXT, -- JSON
    annotations TEXT, -- JSON
    spec TEXT NOT NULL, -- JSON
    status TEXT, -- JSON
    node_name TEXT,
    phase TEXT DEFAULT 'Pending',
    UNIQUE(name, namespace),
    FOREIGN KEY (namespace) REFERENCES namespaces(name) ON DELETE CASCADE
);

-- Create services table
CREATE TABLE IF NOT EXISTS services (
    uid TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    namespace TEXT NOT NULL DEFAULT 'default',
    resource_version INTEGER NOT NULL DEFAULT 1,
    creation_timestamp TEXT NOT NULL,
    deletion_timestamp TEXT,
    labels TEXT, -- JSON
    annotations TEXT, -- JSON
    spec TEXT NOT NULL, -- JSON
    status TEXT, -- JSON
    cluster_ip TEXT,
    UNIQUE(name, namespace),
    FOREIGN KEY (namespace) REFERENCES namespaces(name) ON DELETE CASCADE
);

-- Create endpoints table
CREATE TABLE IF NOT EXISTS endpoints (
    uid TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    namespace TEXT NOT NULL DEFAULT 'default',
    resource_version INTEGER NOT NULL DEFAULT 1,
    creation_timestamp TEXT NOT NULL,
    deletion_timestamp TEXT,
    labels TEXT, -- JSON
    annotations TEXT, -- JSON
    subsets TEXT NOT NULL, -- JSON array of endpoint subsets
    UNIQUE(name, namespace),
    FOREIGN KEY (namespace) REFERENCES namespaces(name) ON DELETE CASCADE
);

-- Create deployments table
CREATE TABLE IF NOT EXISTS deployments (
    uid TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    namespace TEXT NOT NULL DEFAULT 'default',
    resource_version INTEGER NOT NULL DEFAULT 1,
    creation_timestamp TEXT NOT NULL,
    deletion_timestamp TEXT,
    labels TEXT, -- JSON
    annotations TEXT, -- JSON
    spec TEXT NOT NULL, -- JSON
    status TEXT, -- JSON
    generation INTEGER NOT NULL DEFAULT 1,
    replicas INTEGER NOT NULL DEFAULT 1,
    UNIQUE(name, namespace),
    FOREIGN KEY (namespace) REFERENCES namespaces(name) ON DELETE CASCADE
);

-- Create replicasets table
CREATE TABLE IF NOT EXISTS replicasets (
    uid TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    namespace TEXT NOT NULL DEFAULT 'default',
    resource_version INTEGER NOT NULL DEFAULT 1,
    creation_timestamp TEXT NOT NULL,
    deletion_timestamp TEXT,
    labels TEXT, -- JSON
    annotations TEXT, -- JSON
    spec TEXT NOT NULL, -- JSON
    status TEXT, -- JSON
    owner_references TEXT, -- JSON array of ownerReferences
    replicas INTEGER NOT NULL DEFAULT 1,
    UNIQUE(name, namespace),
    FOREIGN KEY (namespace) REFERENCES namespaces(name) ON DELETE CASCADE
);

-- Create nodes table (for single-node, but still needed)
CREATE TABLE IF NOT EXISTS nodes (
    uid TEXT PRIMARY KEY,
    name TEXT UNIQUE NOT NULL,
    resource_version INTEGER NOT NULL DEFAULT 1,
    creation_timestamp TEXT NOT NULL,
    deletion_timestamp TEXT,
    labels TEXT, -- JSON
    annotations TEXT, -- JSON
    spec TEXT NOT NULL, -- JSON
    status TEXT -- JSON
);

-- Create events table for audit and watch functionality
CREATE TABLE IF NOT EXISTS events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    resource_type TEXT NOT NULL,
    resource_uid TEXT NOT NULL,
    resource_name TEXT NOT NULL,
    resource_namespace TEXT,
    event_type TEXT NOT NULL, -- ADDED, MODIFIED, DELETED
    resource_version INTEGER NOT NULL,
    timestamp TEXT NOT NULL,
    object TEXT NOT NULL -- Full JSON object
);

-- Create watch cursors table
CREATE TABLE IF NOT EXISTS watch_cursors (
    id TEXT PRIMARY KEY,
    resource_type TEXT NOT NULL,
    last_event_id INTEGER NOT NULL,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL
);

-- Create indexes for performance
CREATE INDEX idx_pods_namespace ON pods(namespace);
CREATE INDEX idx_pods_node ON pods(node_name);
CREATE INDEX idx_pods_phase ON pods(phase);
CREATE INDEX idx_services_namespace ON services(namespace);
CREATE INDEX idx_endpoints_namespace ON endpoints(namespace);
CREATE INDEX idx_deployments_namespace ON deployments(namespace);
CREATE INDEX idx_replicasets_namespace ON replicasets(namespace);
CREATE INDEX idx_events_resource ON events(resource_type, resource_uid);
CREATE INDEX idx_events_timestamp ON events(timestamp);
CREATE INDEX idx_watch_cursors_expires ON watch_cursors(expires_at);

-- Insert default namespace
INSERT INTO namespaces (uid, name, creation_timestamp, spec, status)
VALUES (
    'default-namespace-uid',
    'default',
    datetime('now'),
    '{"finalizers":["kubernetes"]}',
    '{"phase":"Active"}'
) ON CONFLICT(name) DO NOTHING;

-- Insert kube-system namespace
INSERT INTO namespaces (uid, name, creation_timestamp, spec, status)
VALUES (
    'kube-system-namespace-uid',
    'kube-system',
    datetime('now'),
    '{"finalizers":["kubernetes"]}',
    '{"phase":"Active"}'
) ON CONFLICT(name) DO NOTHING;