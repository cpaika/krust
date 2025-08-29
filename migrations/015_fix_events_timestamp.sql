-- Fix events table to make timestamp nullable and add default
ALTER TABLE events RENAME TO events_backup;

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
    timestamp TEXT DEFAULT CURRENT_TIMESTAMP,  -- Make it have a default
    object TEXT,
    -- New columns for Kubernetes events
    involved_object_uid TEXT,
    involved_object_kind TEXT,
    involved_object_name TEXT,
    reason TEXT,
    message TEXT,
    event_time TEXT DEFAULT CURRENT_TIMESTAMP,
    first_timestamp TEXT DEFAULT CURRENT_TIMESTAMP,
    last_timestamp TEXT DEFAULT CURRENT_TIMESTAMP,
    count INTEGER DEFAULT 1,
    type TEXT DEFAULT 'Normal'
);

-- Copy existing data
INSERT INTO events (
    id, uid, namespace, resource_type, resource_uid, resource_name, 
    resource_namespace, event_type, resource_version, timestamp, object,
    involved_object_uid, involved_object_kind, involved_object_name,
    reason, message, event_time, first_timestamp, last_timestamp, count, type
)
SELECT 
    id, uid, namespace, resource_type, resource_uid, resource_name,
    resource_namespace, event_type, resource_version, timestamp, object,
    involved_object_uid, involved_object_kind, involved_object_name,
    reason, message, event_time, first_timestamp, last_timestamp, count, type
FROM events_backup;

DROP TABLE events_backup;

-- Recreate indices
CREATE INDEX idx_events_resource ON events(resource_type, resource_uid);
CREATE INDEX idx_events_timestamp ON events(timestamp);
CREATE INDEX idx_events_namespace ON events(namespace);
CREATE INDEX idx_events_involved_object ON events(involved_object_uid);