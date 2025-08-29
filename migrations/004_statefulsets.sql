-- StatefulSet table
CREATE TABLE IF NOT EXISTS statefulsets (
    uid TEXT PRIMARY KEY,
    namespace TEXT NOT NULL,
    name TEXT NOT NULL,
    
    -- Spec fields
    replicas INTEGER NOT NULL DEFAULT 1,
    selector TEXT NOT NULL, -- JSON object for label selector
    service_name TEXT NOT NULL, -- Headless service name
    pod_management_policy TEXT DEFAULT 'OrderedReady', -- OrderedReady or Parallel
    update_strategy TEXT NOT NULL, -- JSON object with type and rollingUpdate
    revision_history_limit INTEGER DEFAULT 10,
    min_ready_seconds INTEGER DEFAULT 0,
    persistent_volume_claim_retention_policy TEXT, -- JSON object
    ordinals TEXT, -- JSON object with start ordinal
    
    -- Template
    template TEXT NOT NULL, -- JSON object for pod template
    volume_claim_templates TEXT, -- JSON array of PVC templates
    
    -- Status fields
    observed_generation INTEGER DEFAULT 0,
    replicas_status INTEGER DEFAULT 0,
    ready_replicas INTEGER DEFAULT 0,
    current_replicas INTEGER DEFAULT 0,
    updated_replicas INTEGER DEFAULT 0,
    current_revision TEXT,
    update_revision TEXT,
    collision_count INTEGER DEFAULT 0,
    available_replicas INTEGER DEFAULT 0,
    conditions TEXT, -- JSON array of conditions
    
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
CREATE INDEX IF NOT EXISTS idx_statefulset_namespace ON statefulsets(namespace);
CREATE INDEX IF NOT EXISTS idx_statefulset_service ON statefulsets(service_name);