-- Create horizontalpodautoscalers table
CREATE TABLE IF NOT EXISTS horizontalpodautoscalers (
    uid TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    namespace TEXT NOT NULL,
    spec TEXT NOT NULL,
    status TEXT,
    labels TEXT,
    annotations TEXT,
    resource_version INTEGER NOT NULL DEFAULT 1,
    generation INTEGER NOT NULL DEFAULT 1,
    creation_timestamp TEXT NOT NULL,
    deletion_timestamp TEXT,
    UNIQUE(name, namespace)
);