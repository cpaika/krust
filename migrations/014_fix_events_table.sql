-- Fix events table to support both legacy and new event formats
-- Add missing columns that new resources expect

-- First, rename the old events table
ALTER TABLE events RENAME TO events_old;

-- Create new events table with all needed columns
CREATE TABLE events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    uid TEXT UNIQUE,
    namespace TEXT,
    resource_type TEXT,
    resource_uid TEXT,
    resource_name TEXT,
    resource_namespace TEXT,
    event_type TEXT,
    resource_version INTEGER,
    timestamp TEXT NOT NULL,
    object TEXT,
    -- New columns for Kubernetes events
    involved_object_uid TEXT,
    involved_object_kind TEXT,
    involved_object_name TEXT,
    reason TEXT,
    message TEXT,
    event_time TEXT,
    first_timestamp TEXT,
    last_timestamp TEXT,
    count INTEGER DEFAULT 1,
    type TEXT
);

-- Copy old data to new table
INSERT INTO events (
    id, resource_type, resource_uid, resource_name, 
    resource_namespace, event_type, resource_version, 
    timestamp, object
)
SELECT 
    id, resource_type, resource_uid, resource_name,
    resource_namespace, event_type, resource_version,
    timestamp, object
FROM events_old;

-- Drop old table
DROP TABLE events_old;

-- Recreate indices
CREATE INDEX idx_events_resource ON events(resource_type, resource_uid);
CREATE INDEX idx_events_timestamp ON events(timestamp);
CREATE INDEX idx_events_namespace ON events(namespace);
CREATE INDEX idx_events_involved_object ON events(involved_object_uid);