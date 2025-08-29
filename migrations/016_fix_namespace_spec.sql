-- Fix namespace spec to allow NULL values since K8s namespaces don't always have a spec
-- First, create a new table with the correct schema
CREATE TABLE namespaces_new (
    uid TEXT PRIMARY KEY,
    name TEXT UNIQUE NOT NULL,
    resource_version INTEGER NOT NULL DEFAULT 1,
    creation_timestamp TEXT NOT NULL,
    deletion_timestamp TEXT,
    labels TEXT, -- JSON
    annotations TEXT, -- JSON
    spec TEXT, -- JSON (now nullable)
    status TEXT -- JSON
);

-- Copy data from old table
INSERT INTO namespaces_new SELECT * FROM namespaces;

-- Drop old table
DROP TABLE namespaces;

-- Rename new table
ALTER TABLE namespaces_new RENAME TO namespaces;