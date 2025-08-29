-- Create roles table
CREATE TABLE IF NOT EXISTS roles (
    uid TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    namespace TEXT NOT NULL,
    rules TEXT NOT NULL,
    labels TEXT,
    annotations TEXT,
    resource_version INTEGER NOT NULL DEFAULT 1,
    creation_timestamp TEXT NOT NULL,
    deletion_timestamp TEXT,
    UNIQUE(name, namespace)
);

-- Create rolebindings table
CREATE TABLE IF NOT EXISTS rolebindings (
    uid TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    namespace TEXT NOT NULL,
    subjects TEXT NOT NULL,
    role_ref TEXT NOT NULL,
    labels TEXT,
    annotations TEXT,
    resource_version INTEGER NOT NULL DEFAULT 1,
    creation_timestamp TEXT NOT NULL,
    deletion_timestamp TEXT,
    UNIQUE(name, namespace)
);

-- Create clusterroles table
CREATE TABLE IF NOT EXISTS clusterroles (
    uid TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    rules TEXT NOT NULL,
    aggregation_rule TEXT,
    labels TEXT,
    annotations TEXT,
    resource_version INTEGER NOT NULL DEFAULT 1,
    creation_timestamp TEXT NOT NULL,
    deletion_timestamp TEXT
);

-- Create clusterrolebindings table
CREATE TABLE IF NOT EXISTS clusterrolebindings (
    uid TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    subjects TEXT NOT NULL,
    role_ref TEXT NOT NULL,
    labels TEXT,
    annotations TEXT,
    resource_version INTEGER NOT NULL DEFAULT 1,
    creation_timestamp TEXT NOT NULL,
    deletion_timestamp TEXT
);