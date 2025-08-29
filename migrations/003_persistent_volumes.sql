-- PersistentVolume table
CREATE TABLE IF NOT EXISTS persistent_volumes (
    uid TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    
    -- Spec fields
    capacity TEXT NOT NULL, -- JSON object with storage amounts
    access_modes TEXT NOT NULL, -- JSON array of access modes
    reclaim_policy TEXT NOT NULL, -- Retain, Recycle, or Delete
    storage_class_name TEXT,
    volume_mode TEXT DEFAULT 'Filesystem', -- Filesystem or Block
    
    -- Volume source (only one should be set, stored as JSON)
    host_path TEXT, -- JSON object with path
    nfs TEXT, -- JSON object with server and path
    local TEXT, -- JSON object with path
    csi TEXT, -- JSON object with driver, volumeHandle, etc.
    
    -- Status fields
    phase TEXT DEFAULT 'Available', -- Available, Bound, Released, Failed
    message TEXT,
    reason TEXT,
    
    -- Claim reference (when bound)
    claim_namespace TEXT,
    claim_name TEXT,
    claim_uid TEXT,
    
    -- Metadata
    labels TEXT NOT NULL, -- JSON object
    annotations TEXT NOT NULL, -- JSON object
    resource_version INTEGER NOT NULL,
    creation_timestamp TEXT NOT NULL,
    deletion_timestamp TEXT,
    
    FOREIGN KEY (claim_namespace, claim_name) REFERENCES persistent_volume_claims(namespace, name)
);

-- PersistentVolumeClaim table
CREATE TABLE IF NOT EXISTS persistent_volume_claims (
    uid TEXT PRIMARY KEY,
    namespace TEXT NOT NULL,
    name TEXT NOT NULL,
    
    -- Spec fields
    access_modes TEXT NOT NULL, -- JSON array of access modes
    resources TEXT NOT NULL, -- JSON object with requests and limits
    storage_class_name TEXT,
    volume_mode TEXT DEFAULT 'Filesystem', -- Filesystem or Block
    volume_name TEXT, -- Reference to bound PV
    selector TEXT, -- JSON object for label selector
    
    -- Status fields
    phase TEXT DEFAULT 'Pending', -- Pending, Bound, Lost
    access_modes_status TEXT, -- JSON array of actual access modes
    capacity TEXT, -- JSON object with actual capacity
    
    -- Metadata
    labels TEXT NOT NULL, -- JSON object
    annotations TEXT NOT NULL, -- JSON object
    resource_version INTEGER NOT NULL,
    creation_timestamp TEXT NOT NULL,
    deletion_timestamp TEXT,
    
    UNIQUE(namespace, name),
    FOREIGN KEY (volume_name) REFERENCES persistent_volumes(name)
);

-- Indexes for efficient queries
CREATE INDEX IF NOT EXISTS idx_pv_phase ON persistent_volumes(phase);
CREATE INDEX IF NOT EXISTS idx_pv_storage_class ON persistent_volumes(storage_class_name);
CREATE INDEX IF NOT EXISTS idx_pv_claim ON persistent_volumes(claim_namespace, claim_name);
CREATE INDEX IF NOT EXISTS idx_pvc_namespace ON persistent_volume_claims(namespace);
CREATE INDEX IF NOT EXISTS idx_pvc_phase ON persistent_volume_claims(phase);
CREATE INDEX IF NOT EXISTS idx_pvc_storage_class ON persistent_volume_claims(storage_class_name);
CREATE INDEX IF NOT EXISTS idx_pvc_volume ON persistent_volume_claims(volume_name);