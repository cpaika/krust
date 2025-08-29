-- Add ConfigMaps table
CREATE TABLE IF NOT EXISTS configmaps (
    uid TEXT PRIMARY KEY,
    namespace TEXT NOT NULL,
    name TEXT NOT NULL,
    data TEXT NOT NULL, -- JSON object for string data
    binary_data TEXT, -- JSON object for base64-encoded binary data
    immutable BOOLEAN DEFAULT FALSE,
    labels TEXT NOT NULL, -- JSON object
    annotations TEXT NOT NULL, -- JSON object
    resource_version INTEGER NOT NULL,
    creation_timestamp TEXT NOT NULL,
    deletion_timestamp TEXT,
    UNIQUE(namespace, name)
);

-- Add Secrets table (similar structure)
CREATE TABLE IF NOT EXISTS secrets (
    uid TEXT PRIMARY KEY,
    namespace TEXT NOT NULL,
    name TEXT NOT NULL,
    type TEXT NOT NULL DEFAULT 'Opaque', -- Opaque, kubernetes.io/tls, etc.
    data TEXT NOT NULL, -- JSON object for base64-encoded data
    string_data TEXT, -- JSON object for string data (converted to data on save)
    immutable BOOLEAN DEFAULT FALSE,
    labels TEXT NOT NULL, -- JSON object
    annotations TEXT NOT NULL, -- JSON object
    resource_version INTEGER NOT NULL,
    creation_timestamp TEXT NOT NULL,
    deletion_timestamp TEXT,
    UNIQUE(namespace, name)
);

-- Create indexes for efficient lookups
CREATE INDEX IF NOT EXISTS idx_configmaps_namespace ON configmaps(namespace);
CREATE INDEX IF NOT EXISTS idx_configmaps_deletion ON configmaps(deletion_timestamp);
CREATE INDEX IF NOT EXISTS idx_secrets_namespace ON secrets(namespace);
CREATE INDEX IF NOT EXISTS idx_secrets_deletion ON secrets(deletion_timestamp);